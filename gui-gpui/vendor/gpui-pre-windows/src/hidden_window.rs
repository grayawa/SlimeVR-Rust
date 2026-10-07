//! Apply drawable bounds without showing or activating an offscreen host.
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos},
};

pub(crate) fn apply_bounds(hwnd: HWND, rect: RECT) -> windows::core::Result<()> {
    // No SWP_SHOWWINDOW: the host stays hidden. Unlike merely saving placement,
    // this sends WM_SIZE synchronously and initializes the DirectX render target.
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    }
}
