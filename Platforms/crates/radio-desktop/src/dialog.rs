//! The Add Station dialog.
//!
//! The macOS app asks for a URL and a title in one alert, validates both, and
//! fills the title in from the station's own name when it can. This is the same
//! flow in each platform's own toolkit: a GTK dialog on Linux, a small modal
//! window on Windows. Both run on the app's thread, because they need that
//! thread's event loop.

use radio_core::station::Station;

#[cfg(target_os = "linux")]
pub use gtk_dialog::add_station;

#[cfg(target_os = "windows")]
pub use win32_dialog::add_station;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn add_station() -> Option<Station> {
    None
}

#[cfg(target_os = "linux")]
mod gtk_dialog {
    use super::Station;
    use gtk::prelude::*;
    use gtk::{Dialog, Entry, Label, ResponseType};
    use radio_core::station;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::time::Duration;

    pub fn add_station() -> Option<Station> {
        let dialog = Dialog::new();
        dialog.set_title("Add Station");
        dialog.set_modal(true);
        dialog.set_default_size(460, -1);
        dialog.set_border_width(18);
        dialog.add_button("Cancel", ResponseType::Cancel);
        dialog.add_button("Add", ResponseType::Ok);
        dialog.set_default_response(ResponseType::Ok);
        dialog.set_response_sensitive(ResponseType::Ok, false);

        let area = dialog.content_area();
        area.set_spacing(8);

        let url = Entry::new();
        url.set_placeholder_text(Some("Stream URL"));
        url.set_width_chars(48);

        let title = Entry::new();
        title.set_placeholder_text(Some("Title"));
        title.set_activates_default(true);

        let hint = Label::new(Some("The title fills in when the station announces one."));
        hint.set_xalign(0.0);

        area.add(&url);
        area.add(&title);
        area.add(&hint);

        let result: Rc<RefCell<Option<Station>>> = Rc::new(RefCell::new(None));
        let enabled = Rc::new(Cell::new(false));
        let generation = Rc::new(Cell::new(0u64));
        let suggestion: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let validate = {
            let dialog = dialog.clone();
            let url = url.clone();
            let title = title.clone();
            let enabled = enabled.clone();
            move || {
                let valid_url = station::stream_url(&url.text()).is_some();
                let valid_title = !title.text().trim().is_empty();
                let valid = valid_url && valid_title;
                enabled.set(valid);
                dialog.set_response_sensitive(ResponseType::Ok, valid);
            }
        };

        // Return in the URL field submits, or moves on to the title, exactly as
        // the macOS form does.
        {
            let dialog = dialog.clone();
            let title = title.clone();
            let enabled = enabled.clone();
            url.connect_activate(move |_| {
                if enabled.get() {
                    dialog.response(ResponseType::Ok);
                } else {
                    title.grab_focus();
                }
            });
        }

        let (sender, receiver) = mpsc::channel::<(u64, Option<String>)>();
        {
            let generation = generation.clone();
            url.connect_changed(move |entry| {
                generation.set(generation.get() + 1);
                let ticket = generation.get();
                let text = entry.text().to_string();
                let sender = sender.clone();

                // Ask the station what it calls itself, but only once typing has
                // paused: the macOS form debounces by 400 ms for the same reason.
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(400));
                    let answer = station::stream_url(&text)
                        .and_then(|url| radio_core::metadata::suggested_title(&url));
                    let _ = sender.send((ticket, answer));
                });
            });
        }

        let source = {
            let suggestion = suggestion.clone();
            let title_entry = title.clone();
            let generation = generation.clone();
            gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
                while let Ok((ticket, answer)) = receiver.try_recv() {
                    if ticket != generation.get() {
                        continue;
                    }
                    let current = title_entry.text().to_string();
                    let remembered = suggestion.borrow().clone();
                    let untouched = current.is_empty() || Some(current.clone()) == remembered;

                    match answer {
                        Some(name) if untouched => {
                            *suggestion.borrow_mut() = Some(name.clone());
                            title_entry.set_text(&name);
                        }
                        None if Some(current) == remembered => {
                            // The station stopped naming itself; do not keep a
                            // suggestion the user never typed.
                            *suggestion.borrow_mut() = None;
                            title_entry.set_text("");
                        }
                        _ => {}
                    }
                }
                gtk::glib::ControlFlow::Continue
            })
        };

        validate();
        {
            let validate = validate.clone();
            url.connect_changed(move |_| validate());
        }
        {
            let validate = validate.clone();
            title.connect_changed(move |_| validate());
        }

        {
            let result = result.clone();
            let url = url.clone();
            let title = title.clone();
            dialog.connect_response(move |dialog, response| {
                if response == ResponseType::Ok {
                    let url_text = url.text();
                    let title_text = title.text().trim().to_string();
                    if let Some(stream) = station::stream_url(&url_text) {
                        if !title_text.is_empty() {
                            *result.borrow_mut() =
                                Some(Station::new(title_text, stream.to_string()));
                        }
                    }
                }
                dialog.close();
            });
        }

        dialog.show_all();
        url.grab_focus();
        while dialog.is_visible() {
            gtk::main_iteration_do(true);
        }

        // The app loop iterates GTK events forever, so an abandoned timeout would
        // keep waking for every dialog that was ever opened.
        source.remove();

        let station = result.borrow_mut().take();
        station
    }
}

#[cfg(target_os = "windows")]
mod win32_dialog {
    use super::Station;
    use radio_core::station;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, PostMessageW, PostQuitMessage,
        RegisterClassW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, TranslateMessage,
        GWLP_USERDATA, MSG, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WS_CAPTION,
        WS_CHILD, WS_EX_CLIENTEDGE, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WNDCLASSW,
    };

    const ID_URL: i32 = 100;
    const ID_TITLE: i32 = 101;
    const ID_ADD: i32 = 102;
    const ID_CANCEL: i32 = 103;
    const EN_CHANGE: u16 = 0x0300;
    const WM_SUGGESTION: u32 = WM_APP + 1;
    const BS_DEFPUSHBUTTON: u32 = 0x0001;

    struct State {
        url: HWND,
        title: HWND,
        add: HWND,
        suggested: Option<String>,
        generation: u64,
        station: Option<Station>,
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn text_of(handle: HWND) -> String {
        let length = GetWindowTextLengthW(handle).max(0) as usize;
        let mut buffer = vec![0u16; length + 1];
        let written = GetWindowTextW(handle, buffer.as_mut_ptr(), buffer.len() as i32);
        String::from_utf16_lossy(&buffer[..written.max(0) as usize])
    }

    unsafe fn set_text(handle: HWND, text: &str) {
        let encoded = wide(text);
        SetWindowTextW(handle, encoded.as_ptr());
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn child(
        parent: HWND,
        class: &str,
        text: &str,
        id: i32,
        style: u32,
        extended_style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        let class = wide(class);
        let text = wide(text);
        let handle = CreateWindowExW(
            extended_style,
            class.as_ptr(),
            text.as_ptr(),
            style | WS_CHILD | WS_VISIBLE,
            x,
            y,
            width,
            height,
            parent,
            id as usize as *mut core::ffi::c_void,
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );

        let font = GetStockObject(DEFAULT_GUI_FONT);
        SendMessageW(handle, WM_SETFONT, font as WPARAM, 1);
        handle
    }

    unsafe fn validate(state: &State) {
        let valid_url = station::stream_url(&text_of(state.url)).is_some();
        let valid_title = !text_of(state.title).trim().is_empty();
        EnableWindow(state.add, i32::from(valid_url && valid_title));
    }

    unsafe fn collect(state: &State) -> Option<Station> {
        let stream = station::stream_url(&text_of(state.url))?;
        let title = text_of(state.title).trim().to_string();
        if title.is_empty() {
            return None;
        }
        Some(Station::new(title, stream.to_string()))
    }

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;

        match message {
            WM_COMMAND => {
                let id = (wparam & 0xffff) as i32;
                let notification = ((wparam >> 16) & 0xffff) as u16;

                if !pointer.is_null() {
                    let state = &mut *pointer;
                    match id {
                        ID_ADD => {
                            if let Some(station) = collect(state) {
                                state.station = Some(station);
                                DestroyWindow(hwnd);
                            }
                            return 0;
                        }
                        ID_CANCEL => {
                            DestroyWindow(hwnd);
                            return 0;
                        }
                        ID_URL if notification == EN_CHANGE => {
                            state.generation += 1;
                            let ticket = state.generation;
                            let text = text_of(state.url);
                            let target = hwnd as usize;

                            std::thread::spawn(move || {
                                std::thread::sleep(std::time::Duration::from_millis(400));
                                let answer = station::stream_url(&text)
                                    .and_then(|url| radio_core::metadata::suggested_title(&url));
                                let payload = Box::into_raw(Box::new(answer));
                                PostMessageW(
                                    target as *mut core::ffi::c_void,
                                    WM_SUGGESTION,
                                    ticket as usize,
                                    payload as isize,
                                );
                            });
                            validate(state);
                        }
                        ID_TITLE if notification == EN_CHANGE => validate(state),
                        _ => {}
                    }
                }
                return 0;
            }
            WM_SUGGESTION => {
                let payload = lparam as *mut Option<String>;
                if !payload.is_null() {
                    let answer = *Box::from_raw(payload);
                    if !pointer.is_null() {
                        let state = &mut *pointer;
                        if wparam as u64 == state.generation {
                            let current = text_of(state.title);
                            let untouched =
                                current.is_empty() || Some(current.clone()) == state.suggested;

                            match answer {
                                Some(name) if untouched => {
                                    state.suggested = Some(name.clone());
                                    set_text(state.title, &name);
                                }
                                None if Some(current) == state.suggested => {
                                    state.suggested = None;
                                    set_text(state.title, "");
                                }
                                _ => {}
                            }
                            validate(state);
                        }
                    }
                }
                return 0;
            }
            WM_CLOSE => {
                DestroyWindow(hwnd);
                return 0;
            }
            WM_DESTROY => {
                // The state box is freed by `add_station` after the loop: the
                // window can be gone while its answer is still being read.
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    pub fn add_station() -> Option<Station> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            if instance.is_null() {
                return None;
            }

            let class_name = wide("RadioAddStation");
            let mut class: WNDCLASSW = std::mem::zeroed();
            class.lpfnWndProc = Some(window_proc);
            class.hInstance = instance;
            class.lpszClassName = class_name.as_ptr();
            RegisterClassW(&class);

            let caption = wide("Add Station");
            let dialog = CreateWindowExW(
                0,
                class_name.as_ptr(),
                caption.as_ptr(),
                WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                240,
                200,
                492,
                232,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );

            if dialog.is_null() {
                return None;
            }

            child(dialog, "STATIC", "Stream URL", 0, 0, 0, 16, 12, 460, 18);
            let url = child(dialog, "EDIT", "", ID_URL, WS_TABSTOP, WS_EX_CLIENTEDGE, 16, 32, 460, 26);
            child(dialog, "STATIC", "Title", 0, 0, 0, 16, 68, 460, 18);
            let title = child(dialog, "EDIT", "", ID_TITLE, WS_TABSTOP, WS_EX_CLIENTEDGE, 16, 88, 460, 26);
            child(
                dialog,
                "STATIC",
                "The title fills in when the station announces one.",
                0,
                0,
                0,
                16,
                122,
                460,
                18,
            );

            let add = child(
                dialog,
                "BUTTON",
                "Add",
                ID_ADD,
                WS_TABSTOP | BS_DEFPUSHBUTTON,
                0,
                292,
                160,
                88,
                28,
            );
            child(dialog, "BUTTON", "Cancel", ID_CANCEL, WS_TABSTOP, 0, 388, 160, 88, 28);

            let state = Box::new(State {
                url,
                title,
                add,
                suggested: None,
                generation: 0,
                station: None,
            });
            let pointer = Box::into_raw(state);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, pointer as isize);

            EnableWindow(add, 0);
            SetFocus(url);

            let mut message: MSG = std::mem::zeroed();
            loop {
                let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            let state = Box::from_raw(pointer);
            state.station.clone()
        }
    }
}
