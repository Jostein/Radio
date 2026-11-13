use crate::app::App;
use std::time::Duration;

/// Runs the app forever.
///
/// There is no window, so there is no toolkit event loop to host the tray in.
/// Each platform needs its own idle loop: GTK's on Linux (libappindicator
/// delivers menu clicks through it), a plain message pump on Windows (the tray
/// needs one for menu commands), and a bare sleep anywhere else so the core can
/// be compiled and poked at on a development machine.
pub fn run() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        gtk::init().map_err(|error| format!("The GTK main loop could not start: {error}"))?;
    }

    // SMTC is bound to a top-level window, so the Windows build keeps a hidden
    // one around purely to own it. Nothing is ever shown.
    #[cfg(target_os = "windows")]
    {
        let window = create_host_window()?;
        crate::smtc::set_window(window);
    }

    let mut app = App::new()?;

    loop {
        app.pump();

        #[cfg(target_os = "linux")]
        {
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
        }

        #[cfg(target_os = "windows")]
        pump_windows_messages();

        std::thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(target_os = "windows")]
fn create_host_window() -> Result<isize, String> {
    use windows_sys::Win32::Foundation::GetLastError;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, RegisterClassW, CS_HREDRAW, CS_VREDRAW, WNDCLASSW,
        WS_OVERLAPPED,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    let class_name = wide("RadioHostWindow");
    let window_title = wide("Radio");

    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        if instance.is_null() {
            return Err("The application module could not be found.".to_string());
        }

        let mut class: WNDCLASSW = std::mem::zeroed();
        class.style = CS_HREDRAW | CS_VREDRAW;
        class.lpfnWndProc = Some(DefWindowProcW);
        class.hInstance = instance;
        class.lpszClassName = class_name.as_ptr();

        // A second instance may have registered the class already; that is fine.
        if RegisterClassW(&class) == 0 {
            let _ = GetLastError();
        }

        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );

        if window.is_null() {
            return Err(format!(
                "The hidden window for media controls could not be created (error {}).",
                GetLastError()
            ));
        }

        Ok(window as isize)
    }
}

#[cfg(target_os = "windows")]
fn pump_windows_messages() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
    };

    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
