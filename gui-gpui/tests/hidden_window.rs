#![cfg(windows)]

#[path = "../vendor/gpui-pre-windows/src/hidden_window.rs"]
mod hidden_window;

#[test]
fn hidden_host_gets_drawable_bounds_without_becoming_visible() {
    use windows::{
        Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, GetClientRect, GetForegroundWindow,
                IsWindowVisible, WINDOW_EX_STYLE, WS_POPUP,
            },
        },
        core::w,
    };
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0).unwrap() };
        }
    }
    // Use a built-in window class: the test needs neither a GPU nor SteamVR.
    let window = Window(unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("hidden overlay test"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    });
    let foreground = unsafe { GetForegroundWindow() };
    for (width, height) in [(1100, 720), (1650, 1080)] {
        hidden_window::apply_bounds(
            window.0,
            RECT {
                left: 100,
                top: 100,
                right: 100 + width,
                bottom: 100 + height,
            },
        )
        .unwrap();
        let mut client = RECT::default();
        unsafe { GetClientRect(window.0, &mut client).unwrap() };
        assert_eq!(client.right - client.left, width);
        assert_eq!(client.bottom - client.top, height);
        assert!(!unsafe { IsWindowVisible(window.0).as_bool() });
        assert_eq!(unsafe { GetForegroundWindow() }, foreground);
    }
}
