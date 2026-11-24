//! Finding a station by name in the public Radio Browser directory.
//!
//! The macOS app puts a search field and a result list in a panel; this is the
//! same thing as a small modal window, because the port has no window toolkit
//! beyond what the tray needs. It stays open until a result is picked or the
//! window is closed, and it adds nothing on its own: the caller saves what comes
//! back, exactly like the Add Station dialog.

pub use win32_find::find_station;

#[cfg(target_os = "windows")]
mod win32_find {
    use radio_core::directory::{self, Found};
    use radio_core::station::Station;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{GetStockObject, DEFAULT_GUI_FONT};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
        GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, PostMessageW, PostQuitMessage,
        RegisterClassW, SendMessageW, SetWindowLongPtrW, SetWindowTextW, TranslateMessage,
        GWLP_USERDATA, MSG, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_SETFONT, WS_CAPTION,
        WS_CHILD, WS_EX_CLIENTEDGE, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
        WNDCLASSW,
    };

    const ID_QUERY: i32 = 200;
    const ID_SEARCH: i32 = 201;
    const ID_RESULTS: i32 = 202;
    const ID_DETAIL: i32 = 203;
    const ID_ADD: i32 = 204;
    const ID_CANCEL: i32 = 205;
    const ID_NOTE: i32 = 206;

    const EN_CHANGE: u16 = 0x0300;
    const LBN_SELCHANGE: u16 = 1;
    const LBN_DBLCLK: u16 = 2;

    // List box messages, spelled out so this file needs no more of the
    // Windows bindings than the window itself does.
    const LB_ADDSTRING: u32 = 0x0180;
    const LB_RESETCONTENT: u32 = 0x0184;
    const LB_SETCURSEL: u32 = 0x0186;
    const LB_GETCURSEL: u32 = 0x0188;

    const BS_DEFPUSHBUTTON: u32 = 0x0001;
    const WM_RESULTS: u32 = WM_APP + 1;

    struct State {
        query: HWND,
        search: HWND,
        results: HWND,
        detail: HWND,
        add: HWND,
        note: HWND,
        found: Vec<Found>,
        /// Bumped for every search, so a slow answer to an abandoned query is
        /// dropped instead of filling the list with yesterday's request.
        generation: u64,
        chosen: Option<Station>,
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

    /// Runs the query on its own thread and posts the answer back, because the
    /// dialog is inside the window's message loop and a 10-second fetch there
    /// would freeze it.
    unsafe fn start_search(state: &mut State, window: HWND) {
        let text = text_of(state.query);
        if text.trim().is_empty() {
            set_text(state.note, "Type a name to look for.");
            return;
        }

        state.generation += 1;
        let ticket = state.generation;
        set_text(state.note, "Searching…");
        EnableWindow(state.add, 0);
        SendMessageW(state.results, LB_RESETCONTENT, 0, 0);
        state.found.clear();
        set_text(state.detail, "");

        let target = window as usize;
        std::thread::spawn(move || {
            let answer = directory::search(&text, 30);
            let payload = Box::into_raw(Box::new(answer));
            PostMessageW(
                target as *mut core::ffi::c_void,
                WM_RESULTS,
                ticket as usize,
                payload as isize,
            );
        });
    }

    unsafe fn fill_results(state: &mut State, answer: Result<Vec<Found>, String>) {
        match answer {
            Ok(found) => {
                for entry in &found {
                    let label = wide(&entry.label());
                    SendMessageW(state.results, LB_ADDSTRING, 0, label.as_ptr() as isize);
                }
                if found.is_empty() {
                    set_text(state.note, "Nothing matched. Try a shorter name.");
                } else {
                    set_text(
                        state.note,
                        &format!("{} station(s). Pick one, then Add.", found.len()),
                    );
                }
                state.found = found;
            }
            Err(message) => {
                state.found.clear();
                set_text(state.note, &message);
            }
        }
    }

    /// The entry the list is pointing at, if any.
    unsafe fn selected(state: &State) -> Option<usize> {
        let index = SendMessageW(state.results, LB_GETCURSEL, 0, 0);
        if index < 0 {
            None
        } else {
            Some(index as usize)
        }
    }

    unsafe fn update_selection(state: &mut State) {
        let Some(index) = selected(state) else {
            EnableWindow(state.add, 0);
            set_text(state.detail, "");
            return;
        };
        let Some(entry) = state.found.get(index) else {
            EnableWindow(state.add, 0);
            return;
        };

        set_text(state.detail, &entry.detail());
        EnableWindow(state.add, 1);
        state.chosen = Some(Station::new(entry.name.clone(), entry.url.clone()));
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
                        ID_SEARCH => {
                            start_search(state, hwnd);
                            return 0;
                        }
                        ID_ADD => {
                            if state.chosen.is_some() {
                                DestroyWindow(hwnd);
                            }
                            return 0;
                        }
                        ID_CANCEL => {
                            state.chosen = None;
                            DestroyWindow(hwnd);
                            return 0;
                        }
                        ID_QUERY if notification == EN_CHANGE => {
                            // A new query invalidates the picked result.
                            state.chosen = None;
                            EnableWindow(state.add, 0);
                        }
                        ID_RESULTS if notification == LBN_SELCHANGE => {
                            update_selection(state);
                            return 0;
                        }
                        ID_RESULTS if notification == LBN_DBLCLK => {
                            update_selection(state);
                            if state.chosen.is_some() {
                                DestroyWindow(hwnd);
                            }
                            return 0;
                        }
                        _ => {}
                    }
                }
                return 0;
            }
            WM_RESULTS => {
                let payload = lparam as *mut Result<Vec<Found>, String>;
                if !payload.is_null() {
                    let answer = *Box::from_raw(payload);
                    if !pointer.is_null() {
                        let state = &mut *pointer;
                        if wparam as u64 == state.generation {
                            fill_results(state, answer);
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
                PostQuitMessage(0);
                return 0;
            }
            _ => {}
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    pub fn find_station() -> Option<Station> {
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            if instance.is_null() {
                return None;
            }

            let class_name = wide("RadioFindStation");
            let mut class: WNDCLASSW = std::mem::zeroed();
            class.lpfnWndProc = Some(window_proc);
            class.hInstance = instance;
            class.lpszClassName = class_name.as_ptr();
            RegisterClassW(&class);

            let caption = wide("Find Stations");
            let dialog = CreateWindowExW(
                0,
                class_name.as_ptr(),
                caption.as_ptr(),
                WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
                220,
                160,
                560,
                560,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if dialog.is_null() {
                return None;
            }

            let query = child(
                dialog,
                "EDIT",
                "",
                ID_QUERY,
                WS_TABSTOP,
                WS_EX_CLIENTEDGE,
                16,
                16,
                416,
                26,
            );
            let search = child(
                dialog,
                "BUTTON",
                "Search",
                ID_SEARCH,
                WS_TABSTOP | BS_DEFPUSHBUTTON,
                0,
                444,
                16,
                84,
                26,
            );
            let results = child(
                dialog,
                "LISTBOX",
                "",
                ID_RESULTS,
                WS_TABSTOP | WS_VSCROLL,
                WS_EX_CLIENTEDGE,
                16,
                52,
                512,
                300,
            );
            let detail = child(dialog, "STATIC", "", ID_DETAIL, 0, 0, 16, 360, 512, 18);
            let note = child(dialog, "STATIC", "", ID_NOTE, 0, 0, 16, 384, 512, 18);

            let add = child(
                dialog,
                "BUTTON",
                "Add",
                ID_ADD,
                WS_TABSTOP | BS_DEFPUSHBUTTON,
                0,
                312,
                420,
                100,
                28,
            );
            child(
                dialog,
                "BUTTON",
                "Cancel",
                ID_CANCEL,
                WS_TABSTOP,
                0,
                428,
                420,
                100,
                28,
            );

            let state = Box::new(State {
                query,
                search,
                results,
                detail,
                add,
                note,
                found: Vec::new(),
                generation: 0,
                chosen: None,
            });
            let pointer = Box::into_raw(state);
            SetWindowLongPtrW(dialog, GWLP_USERDATA, pointer as isize);

            EnableWindow(add, 0);
            set_text(note, "Type part of a station name and press Search.");
            SetFocus(query);

            let mut message: MSG = std::mem::zeroed();
            loop {
                let result = GetMessageW(&mut message, std::ptr::null_mut(), 0, 0);
                if result <= 0 {
                    break;
                }
                // Enter in the query field runs the search rather than closing
                // the window, which is what a default button would do otherwise.
                if message.message == 0x0100 && message.hwnd == query && message.wParam == 13 {
                    let state = &mut *pointer;
                    start_search(state, dialog);
                    continue;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            let _ = search;
            let state = Box::from_raw(pointer);
            state.chosen.clone()
        }
    }
}
