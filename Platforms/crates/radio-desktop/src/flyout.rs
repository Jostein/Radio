//! The Windows flyout: the panel a left click on the tray icon opens.
//!
//! Windows offers no modern tray menu to move to. The notification area hosts an
//! icon and nothing else, its own flyouts and Quick Settings are closed to
//! third-party controls, and a popup menu cannot host a slider or a text field at
//! all. So this panel is ours: one borderless window, drawn with GDI, with a real
//! volume slider, a search field and the station list. Right click still opens
//! the native menu, which stays the complete list of commands.
//!
//! Deliberately Windows-only. macOS keeps AppKit's menu and Linux keeps its GTK
//! indicator menu; neither grows a window for this. Everything runs on the app
//! thread — `platform.rs` already pumps messages for the tray, so this is simply
//! another window in that pump and needs no event loop of its own.

use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush,
    DeleteDC, DeleteObject, DrawTextW, Ellipse, EndPaint, FillRect, InvalidateRect, RoundRect,
    SelectObject, SetBkColor, SetBkMode, SetTextColor, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DT_CENTER,
    DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE, DT_VCENTER, HDC, HFONT,
    PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, SetWindowTextW, ShowWindow,
    SystemParametersInfoW, CS_DROPSHADOW, ES_AUTOHSCROLL, ES_LEFT, GWLP_USERDATA, GWLP_WNDPROC,
    HWND_TOPMOST, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW,
    WA_INACTIVE, WM_ACTIVATE, WM_CHAR, WM_COMMAND, WM_CTLCOLOREDIT, WM_ERASEBKGND, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_SETFONT, WNDCLASSW,
    WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
};

const WIDTH: i32 = 300;
const PAD: i32 = 12;
const ROW_HEIGHT: i32 = 28;
const VISIBLE_ROWS: usize = 7;
const FIELD_HEIGHT: i32 = 30;
const EDIT_ID: i32 = 200;
const EM_SETCUEBANNER: u32 = 0x1501;
const EN_CHANGE: u16 = 0x0300;
const VK_ESCAPE: usize = 0x1b;
const VK_RETURN: usize = 0x0d;

/// One station in the list the panel shows.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Row {
    pub id: String,
    pub title: String,
}

/// Everything the panel draws, handed over by the app on each change.
#[derive(Clone, PartialEq, Debug)]
pub struct Content {
    /// Station and state, the line the native menu shows at the top.
    pub header: String,
    /// The track, when the stream publishes one.
    pub detail: String,
    pub volume: f64,
    pub rows: Vec<Row>,
    pub current: Option<String>,
}

/// Where the panel should appear: the tray icon's screen rectangle.
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub x: i32,
    pub y: i32,
    pub width: i32,
}

/// Something the user did in the panel.
#[derive(Clone, Debug)]
pub enum Action {
    Play(String),
    Toggle,
    Volume(f64),
}

struct Theme {
    background: u32,
    text: u32,
    dim: u32,
    field: u32,
    border: u32,
    accent: u32,
    hover: u32,
    field_brush: usize,
}

impl Theme {
    unsafe fn detect() -> Theme {
        let dark = apps_use_dark_mode();
        let field = if dark { 0x002B2B2B } else { 0x00F4F4F4 };
        Theme {
            background: if dark { 0x00202020 } else { 0x00FFFFFF },
            text: if dark { 0x00F0F0F0 } else { 0x00141414 },
            dim: if dark { 0x00A0A0A0 } else { 0x00666666 },
            field,
            border: if dark { 0x00383838 } else { 0x00DFDFDF },
            // #3B82F6, the tray icon's blue, as Windows stores it: 0x00BBGGRR.
            accent: 0x00F6823B,
            hover: if dark { 0x002A2A2A } else { 0x00F2F2F2 },
            field_brush: CreateSolidBrush(field) as usize,
        }
    }
}

struct State {
    hwnd: HWND,
    edit: HWND,
    /// The EDIT control's original window procedure, for the subclass below.
    edit_proc: isize,
    content: Content,
    filter: String,
    scroll: usize,
    hover: Option<usize>,
    dragging: bool,
    actions: Vec<Action>,
    font: HFONT,
    font_strong: HFONT,
    font_small: HFONT,
    theme: Theme,
    visible: bool,
    /// When the panel last hid itself, so the click that closed it through the
    /// tray icon does not immediately reopen it.
    closed_at: Option<Instant>,
}

/// The panel window. `None` from `new` means the app keeps only the native menu.
pub struct Panel {
    state: *mut State,
}

struct Layout {
    width: i32,
    height: i32,
    header: RECT,
    volume_label: RECT,
    slider: RECT,
    field: RECT,
    rows_top: i32,
    visible: usize,
}

impl State {
    /// Rows the filter keeps, as indices into `content.rows`.
    fn filtered(&self) -> Vec<usize> {
        let needle = self.filter.trim().to_lowercase();
        self.content
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| needle.is_empty() || row.title.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect()
    }

    fn max_scroll(&self) -> usize {
        self.filtered().len().saturating_sub(VISIBLE_ROWS)
    }

    unsafe fn invalidate(&self) {
        InvalidateRect(self.hwnd, std::ptr::null(), 0);
    }
}

impl Panel {
    /// Creates the panel, hidden. `None` when the window cannot be made; the app
    /// then behaves exactly as it did before this module existed.
    pub fn new() -> Option<Panel> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            if instance.is_null() {
                return None;
            }

            let class_name = wide("RadioFlyout");
            let mut class: WNDCLASSW = std::mem::zeroed();
            class.style = CS_DROPSHADOW;
            class.lpfnWndProc = Some(window_proc);
            class.hInstance = instance;
            class.lpszClassName = class_name.as_ptr();
            // A second instance of the class is harmless: the panel attaches to
            // whichever registration won.
            RegisterClassW(&class);

            let window = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                wide("Radio").as_ptr(),
                WS_POPUP,
                0,
                0,
                WIDTH,
                200,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if window.is_null() {
                return None;
            }

            let edit = CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                wide("").as_ptr(),
                WS_CHILD | WS_VISIBLE | ES_LEFT as u32 | ES_AUTOHSCROLL as u32,
                0,
                0,
                10,
                10,
                window,
                EDIT_ID as usize as *mut core::ffi::c_void,
                instance,
                std::ptr::null(),
            );
            if edit.is_null() {
                DestroyWindow(window);
                return None;
            }

            let state = Box::new(State {
                hwnd: window,
                edit,
                edit_proc: 0,
                content: Content {
                    header: String::new(),
                    detail: String::new(),
                    volume: 1.0,
                    rows: Vec::new(),
                    current: None,
                },
                filter: String::new(),
                scroll: 0,
                hover: None,
                dragging: false,
                actions: Vec::new(),
                font: std::ptr::null_mut(),
                font_strong: std::ptr::null_mut(),
                font_small: std::ptr::null_mut(),
                theme: Theme::detect(),
                visible: false,
                closed_at: None,
            });
            let pointer = Box::into_raw(state);
            let state = &mut *pointer;

            SetWindowLongPtrW(window, GWLP_USERDATA, pointer as isize);
            SetWindowLongPtrW(edit, GWLP_USERDATA, pointer as isize);

            make_fonts(state);
            SendMessageW(edit, WM_SETFONT, state.font_small as usize, 1);
            let cue = wide("Search stations");
            SendMessageW(edit, EM_SETCUEBANNER, 1, cue.as_ptr() as isize);

            state.edit_proc = GetWindowLongPtrW(edit, GWLP_WNDPROC);
            SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize);

            apply_dwm(window, apps_use_dark_mode());

            Some(Panel { state: pointer })
        }
    }

    fn state(&self) -> &mut State {
        unsafe { &mut *self.state }
    }

    /// Opens the panel, or closes it when it is already open.
    pub fn toggle(&mut self, content: Content, anchor: Anchor) {
        let state = self.state();
        if state.visible {
            unsafe { hide(state) };
            return;
        }
        if let Some(closed) = state.closed_at {
            if closed.elapsed() < Duration::from_millis(300) {
                return;
            }
        }
        unsafe { show(state, content, anchor) };
    }

    pub fn is_open(&self) -> bool {
        self.state().visible
    }

    /// Keeps an open panel in step with the player. Cheap: a changed panel is
    /// repainted, an unchanged one costs a comparison.
    pub fn update(&mut self, content: Content) {
        let state = self.state();
        if state.content == content {
            return;
        }
        state.content = content;
        state.scroll = state.scroll.min(state.max_scroll());
        if state.visible {
            state.hover = None;
            unsafe { state.invalidate() };
        }
    }

    /// What the user did since the last call.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.state().actions)
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        unsafe {
            let state = Box::from_raw(self.state);
            DestroyWindow(state.hwnd);
            for font in [state.font, state.font_strong, state.font_small] {
                if !font.is_null() {
                    DeleteObject(font);
                }
            }
            DeleteObject(state.theme.field_brush as *mut core::ffi::c_void);
        }
    }
}

// MARK: - Showing and hiding

unsafe fn show(state: &mut State, content: Content, anchor: Anchor) {
    state.content = content;
    state.filter.clear();
    state.scroll = 0;
    state.hover = None;
    state.dragging = false;
    set_edit_text(state.edit, "");

    let layout = layout_for(state);
    let (x, y) = place(layout.width, layout.height, anchor);

    SetWindowPos(
        state.hwnd,
        HWND_TOPMOST,
        x,
        y,
        layout.width,
        layout.height,
        SWP_SHOWWINDOW | SWP_NOACTIVATE,
    );
    position_edit(state, &layout);
    ShowWindow(state.hwnd, SW_SHOW);
    SetForegroundWindow(state.hwnd);
    SetFocus(state.edit);
    state.invalidate();

    state.visible = true;
}

unsafe fn hide(state: &mut State) {
    if !state.visible {
        return;
    }
    ShowWindow(state.hwnd, SW_HIDE);
    state.visible = false;
    state.closed_at = Some(Instant::now());
    if state.dragging {
        state.dragging = false;
        ReleaseCapture();
    }
}

/// Above the tray icon when there is room, otherwise just below it, and always
/// inside the work area.
unsafe fn place(width: i32, height: i32, anchor: Anchor) -> (i32, i32) {
    let mut area: RECT = std::mem::zeroed();
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        &mut area as *mut RECT as *mut core::ffi::c_void,
        0,
    );

    let mut x = anchor.x + anchor.width - width;
    let mut y = anchor.y - height - 8;
    if y < area.top {
        y = anchor.y + 40;
    }

    x = x.clamp(area.left + 8, (area.right - width - 8).max(area.left + 8));
    y = y.clamp(area.top + 8, (area.bottom - height - 8).max(area.top + 8));
    (x, y)
}

// MARK: - Layout

fn layout_for(state: &State) -> Layout {
    let visible = state.filtered().len().min(VISIBLE_ROWS);
    let mut y = PAD;

    let header = RECT {
        left: PAD,
        top: y,
        right: WIDTH - PAD,
        bottom: y + 36,
    };
    y += 36;

    let volume_label = RECT {
        left: PAD,
        top: y,
        right: WIDTH - PAD,
        bottom: y + 16,
    };
    y += 16 + 6;

    let slider = RECT {
        left: PAD,
        top: y,
        right: WIDTH - PAD,
        bottom: y + 16,
    };
    y += 16 + 12;

    let field = RECT {
        left: PAD,
        top: y,
        right: WIDTH - PAD,
        bottom: y + FIELD_HEIGHT,
    };
    y += FIELD_HEIGHT + 8;

    let rows_top = y;
    y += visible as i32 * ROW_HEIGHT;

    Layout {
        width: WIDTH,
        height: y + PAD,
        header,
        volume_label,
        slider,
        field,
        rows_top,
        visible,
    }
}

unsafe fn position_edit(state: &State, layout: &Layout) {
    SetWindowPos(
        state.edit,
        std::ptr::null_mut(),
        layout.field.left + 9,
        layout.field.top + 6,
        layout.field.right - layout.field.left - 18,
        18,
        SWP_NOACTIVATE | SWP_NOZORDER,
    );
}

// MARK: - Painting

unsafe fn paint(state: &State, hdc: HDC) {
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(state.hwnd, &mut client);
    let width = client.right;
    let height = client.bottom;
    if width <= 0 || height <= 0 {
        return;
    }

    // Drawn off-screen and blitted once: without it the rows and the field
    // flicker on every repaint.
    let memory = CreateCompatibleDC(hdc);
    let bitmap = CreateCompatibleBitmap(hdc, width, height);
    let previous = SelectObject(memory, bitmap);

    let theme = &state.theme;
    fill(memory, client, theme.background);

    let layout = layout_for(state);

    draw_text(
        memory,
        state.font_strong,
        theme.text,
        layout.header,
        &state.content.header,
        DT_LEFT,
    );
    if !state.content.detail.is_empty() {
        let detail = RECT {
            top: layout.header.top + 20,
            ..layout.header
        };
        draw_text(
            memory,
            state.font_small,
            theme.dim,
            detail,
            &state.content.detail,
            DT_LEFT,
        );
    }

    draw_text(
        memory,
        state.font_small,
        theme.dim,
        layout.volume_label,
        "Volume",
        DT_LEFT,
    );
    let percent = format!("{}%", (state.content.volume * 100.0).round() as i64);
    draw_text(
        memory,
        state.font_small,
        theme.dim,
        layout.volume_label,
        &percent,
        DT_RIGHT,
    );

    // Slider: a rail, the filled part, and a thumb.
    let rail_height = 4;
    let rail_top = layout.slider.top + (layout.slider.bottom - layout.slider.top - rail_height) / 2;
    let rail = RECT {
        left: layout.slider.left + 6,
        top: rail_top,
        right: layout.slider.right - 6,
        bottom: rail_top + rail_height,
    };
    rounded(memory, rail, rail_height / 2, theme.border);

    let volume = state.content.volume.clamp(0.0, 1.0);
    let reach = ((rail.right - rail.left) as f64 * volume).round() as i32;
    if reach > 0 {
        rounded(
            memory,
            RECT {
                right: rail.left + reach,
                ..rail
            },
            rail_height / 2,
            theme.accent,
        );
    }
    let thumb_y = layout.slider.top + (layout.slider.bottom - layout.slider.top) / 2;
    disc(memory, rail.left + reach, thumb_y, 6, theme.accent);

    // Search field: our own rounded box with the borderless EDIT control inset
    // inside it, so the control's background sits on the drawn field.
    outline(memory, layout.field, 8, theme.border, theme.field);

    let filtered = state.filtered();
    if filtered.is_empty() {
        let empty = RECT {
            left: PAD,
            top: layout.rows_top,
            right: WIDTH - PAD,
            bottom: layout.rows_top + ROW_HEIGHT,
        };
        draw_text(
            memory,
            state.font_small,
            theme.dim,
            empty,
            if state.content.rows.is_empty() {
                "No stations yet"
            } else {
                "No station matches"
            },
            DT_CENTER,
        );
    }

    for slot in 0..layout.visible {
        let Some(index) = filtered.get(state.scroll + slot).copied() else {
            break;
        };
        let row = &state.content.rows[index];
        let top = layout.rows_top + slot as i32 * ROW_HEIGHT;
        let area = RECT {
            left: PAD / 2,
            top,
            right: WIDTH - PAD / 2,
            bottom: top + ROW_HEIGHT,
        };

        let current = state.content.current.as_deref() == Some(row.id.as_str());
        if state.hover == Some(index) {
            rounded(memory, area, 6, theme.hover);
        }
        if current {
            let marker = RECT {
                left: PAD / 2 + 4,
                top: top + 8,
                right: PAD / 2 + 7,
                bottom: top + ROW_HEIGHT - 8,
            };
            rounded(memory, marker, 2, theme.accent);
        }

        let text = RECT {
            left: area.left + 12,
            right: area.right - 8,
            ..area
        };
        draw_text(
            memory,
            if current {
                state.font_strong
            } else {
                state.font
            },
            if current { theme.accent } else { theme.text },
            text,
            &row.title,
            DT_LEFT,
        );
    }

    BitBlt(hdc, 0, 0, width, height, memory, 0, 0, SRCCOPY);
    SelectObject(memory, previous);
    DeleteObject(bitmap);
    DeleteDC(memory);
}

unsafe fn draw_text(hdc: HDC, font: HFONT, color: u32, mut rect: RECT, text: &str, align: u32) {
    if text.is_empty() || rect.right <= rect.left {
        return;
    }
    let previous = SelectObject(hdc, font as *mut core::ffi::c_void);
    SetBkMode(hdc, TRANSPARENT as i32);
    SetTextColor(hdc, color);
    let encoded: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(
        hdc,
        encoded.as_ptr(),
        encoded.len() as i32,
        &mut rect,
        align | DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    SelectObject(hdc, previous);
}

unsafe fn fill(hdc: HDC, rect: RECT, color: u32) {
    let brush = CreateSolidBrush(color);
    FillRect(hdc, &rect, brush);
    DeleteObject(brush);
}

unsafe fn rounded(hdc: HDC, rect: RECT, radius: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let previous = SelectObject(hdc, brush);
    RoundRect(
        hdc,
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        radius * 2,
        radius * 2,
    );
    SelectObject(hdc, previous);
    DeleteObject(brush);
}

/// A rounded border with the inside painted over it.
unsafe fn outline(hdc: HDC, rect: RECT, radius: i32, border: u32, inside: u32) {
    rounded(hdc, rect, radius, border);
    rounded(
        hdc,
        RECT {
            left: rect.left + 1,
            top: rect.top + 1,
            right: rect.right - 1,
            bottom: rect.bottom - 1,
        },
        (radius - 1).max(1),
        inside,
    );
}

unsafe fn disc(hdc: HDC, center_x: i32, center_y: i32, radius: i32, color: u32) {
    let brush = CreateSolidBrush(color);
    let previous = SelectObject(hdc, brush);
    Ellipse(
        hdc,
        center_x - radius,
        center_y - radius,
        center_x + radius,
        center_y + radius,
    );
    SelectObject(hdc, previous);
    DeleteObject(brush);
}

unsafe fn make_fonts(state: &mut State) {
    for font in [state.font, state.font_strong, state.font_small] {
        if !font.is_null() {
            DeleteObject(font);
        }
    }
    state.font = font(13, 400);
    state.font_strong = font(13, 600);
    state.font_small = font(12, 400);
}

unsafe fn font(size: i32, weight: i32) -> HFONT {
    CreateFontW(
        -size,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        0,
        0,
        CLEARTYPE_QUALITY as u32,
        0,
        wide("Segoe UI").as_ptr(),
    )
}

// MARK: - Window procedures

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let state = &mut *pointer;

    match message {
        // The panel paints its whole surface; letting Windows erase first makes
        // it flash on every open.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut paint_struct: PAINTSTRUCT = std::mem::zeroed();
            let hdc = BeginPaint(hwnd, &mut paint_struct);
            paint(state, hdc);
            EndPaint(hwnd, &paint_struct);
            0
        }
        WM_ACTIVATE => {
            if (wparam & 0xffff) as u32 == WA_INACTIVE {
                hide(state);
            }
            0
        }
        WM_MOUSEMOVE => {
            let x = (lparam & 0xffff) as i16 as i32;
            let y = ((lparam >> 16) & 0xffff) as i16 as i32;

            if state.dragging {
                set_volume_from(state, x);
                return 0;
            }

            let hover = row_at(state, x, y);
            if hover != state.hover {
                state.hover = hover;
                state.invalidate();
            }
            0
        }
        WM_LBUTTONDOWN => {
            let x = (lparam & 0xffff) as i16 as i32;
            let y = ((lparam >> 16) & 0xffff) as i16 as i32;
            let layout = layout_for(state);

            if contains(&layout.slider, x, y) {
                state.dragging = true;
                SetCapture(hwnd);
                set_volume_from(state, x);
                return 0;
            }

            if let Some(index) = row_at(state, x, y) {
                let row = state.content.rows[index].clone();
                if state.content.current.as_deref() == Some(row.id.as_str()) {
                    state.actions.push(Action::Toggle);
                } else {
                    state.actions.push(Action::Play(row.id));
                }
                hide(state);
            }
            0
        }
        WM_LBUTTONUP => {
            if state.dragging {
                state.dragging = false;
                ReleaseCapture();
            }
            0
        }
        WM_MOUSEWHEEL => {
            // Windows sends the wheel to the window with focus, and that is
            // usually the search field, which forwards it here (see `edit_proc`).
            // One row per notch, clamped.
            let delta = ((wparam >> 16) & 0xffff) as i16;
            let max = state.max_scroll();
            if delta < 0 {
                state.scroll = (state.scroll + 1).min(max);
            } else if delta > 0 {
                state.scroll = state.scroll.saturating_sub(1);
            }
            state.invalidate();
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xffff) as i32;
            let notification = ((wparam >> 16) & 0xffff) as u16;
            if id == EDIT_ID && notification == EN_CHANGE {
                state.filter = edit_text(state.edit);
                state.scroll = 0;
                state.hover = None;
                state.invalidate();
            }
            0
        }
        WM_CTLCOLOREDIT => {
            let hdc = wparam as HDC;
            SetTextColor(hdc, state.theme.text);
            SetBkColor(hdc, state.theme.field);
            state.theme.field_brush as LRESULT
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

/// The search field's procedure: Escape closes the panel and Return plays the
/// first match. A parent cannot see either, because the EDIT control eats both.
unsafe extern "system" fn edit_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let state = &mut *pointer;

    match message {
        // A single-line EDIT does nothing with the wheel, so scrolling the list
        // only works if its messages are handed to the panel.
        WM_MOUSEWHEEL => {
            SendMessageW(
                windows_sys::Win32::UI::WindowsAndMessaging::GetParent(hwnd),
                message,
                wparam,
                lparam,
            );
            return 0;
        }
        WM_KEYDOWN if wparam == VK_ESCAPE => {
            hide(state);
            return 0;
        }
        WM_CHAR if wparam == VK_RETURN => {
            if let Some(index) = state.filtered().first().copied() {
                let row = state.content.rows[index].clone();
                if state.content.current.as_deref() == Some(row.id.as_str()) {
                    state.actions.push(Action::Toggle);
                } else {
                    state.actions.push(Action::Play(row.id));
                }
            }
            hide(state);
            return 0;
        }
        _ => {}
    }

    if state.edit_proc == 0 {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let previous: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
        std::mem::transmute(state.edit_proc);
    CallWindowProcW(Some(previous), hwnd, message, wparam, lparam)
}

// MARK: - Hits and platform helpers

fn contains(rect: &RECT, x: i32, y: i32) -> bool {
    x >= rect.left && x < rect.right && y >= rect.top && y < rect.bottom
}

fn row_at(state: &State, x: i32, y: i32) -> Option<usize> {
    let layout = layout_for(state);
    if y < layout.rows_top || x < PAD / 2 || x >= WIDTH - PAD / 2 {
        return None;
    }
    let slot = ((y - layout.rows_top) / ROW_HEIGHT) as usize;
    if slot >= layout.visible {
        return None;
    }
    state.filtered().get(state.scroll + slot).copied()
}

unsafe fn set_volume_from(state: &mut State, x: i32) {
    let layout = layout_for(state);
    let left = layout.slider.left + 6;
    let right = layout.slider.right - 6;
    if right <= left {
        return;
    }
    let volume = ((x - left) as f64 / (right - left) as f64).clamp(0.0, 1.0);
    if (volume - state.content.volume).abs() < 0.005 {
        return;
    }
    state.content.volume = volume;
    state.actions.push(Action::Volume(volume));
    state.invalidate();
}

unsafe fn apply_dwm(hwnd: HWND, dark: bool) {
    let preference: i32 = DWMWCP_ROUND;
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
        &preference as *const i32 as *const core::ffi::c_void,
        std::mem::size_of::<i32>() as u32,
    );
    let dark: i32 = dark.into();
    let _ = DwmSetWindowAttribute(
        hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
        &dark as *const i32 as *const core::ffi::c_void,
        std::mem::size_of::<i32>() as u32,
    );
}

/// Whether Windows is in dark mode, read from the value Settings writes.
unsafe fn apps_use_dark_mode() -> bool {
    let mut light: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = RegGetValueW(
        HKEY_CURRENT_USER,
        wide("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize").as_ptr(),
        wide("AppsUseLightTheme").as_ptr(),
        RRF_RT_REG_DWORD,
        std::ptr::null_mut(),
        &mut light as *mut u32 as *mut core::ffi::c_void,
        &mut size,
    );
    status == 0 && light == 0
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn set_edit_text(handle: HWND, text: &str) {
    let encoded = wide(text);
    SetWindowTextW(handle, encoded.as_ptr());
}

unsafe fn edit_text(handle: HWND) -> String {
    let length = GetWindowTextLengthW(handle).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let written = GetWindowTextW(handle, buffer.as_mut_ptr(), buffer.len() as i32);
    String::from_utf16_lossy(&buffer[..written.max(0) as usize])
}
