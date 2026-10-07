//! Opt-in GPU output. Registrations and sinks never leave the UI thread.
//! Normal GPUI windows retain their original swap-chain presentation path.
use anyhow::{Result, bail};
use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    Graphics::Direct3D11::ID3D11Texture2D,
    UI::WindowsAndMessaging::{PostMessageW, WM_PAINT},
};

static ADAPTER: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
/// Select the compositor GPU before constructing the GPUI application.
pub fn prefer_adapter(index: u32) -> Result<()> {
    ADAPTER
        .set(index)
        .map_err(|_| anyhow::anyhow!("Overlay GPU was already selected"))
}
pub(crate) fn preferred_adapter() -> Option<u32> {
    ADAPTER.get().copied()
}

type Sink = Box<dyn FnMut(&ID3D11Texture2D, u32, u32)>;
struct Entry {
    active: bool,
    mirror: bool,
    sink: Sink,
}
thread_local! { static OUTPUTS: RefCell<HashMap<usize, Entry>> = RefCell::new(HashMap::new()); }

pub struct Output {
    hwnd: usize,
    _thread: PhantomData<Rc<()>>,
}
impl Output {
    /// # Safety
    /// `hwnd` must be a live GPUI window on this UI thread, and this guard must
    /// be dropped before the window is destroyed or its native handle reused.
    pub unsafe fn attach(
        hwnd: usize,
        mirror: bool,
        sink: impl FnMut(&ID3D11Texture2D, u32, u32) + 'static,
    ) -> Result<Self> {
        OUTPUTS.with(|outputs| {
            let mut outputs = outputs.borrow_mut();
            if outputs.contains_key(&hwnd) {
                bail!("Window already has an overlay output");
            }
            outputs.insert(
                hwnd,
                Entry {
                    active: false,
                    mirror,
                    sink: Box::new(sink),
                },
            );
            Ok(Self {
                hwnd,
                _thread: PhantomData,
            })
        })
    }
    pub fn set_active(&self, active: bool) {
        OUTPUTS.with(|outputs| {
            if let Some(entry) = outputs.borrow_mut().get_mut(&self.hwnd) {
                entry.active = active;
            }
        });
    }
    /// Queue a normal frame callback after the current App borrow unwinds.
    /// GPUI decides whether a dirty scene needs rendering; no CPU readback.
    pub fn request_frame(&self) -> Result<()> {
        // SAFETY: attach's native-window lifetime and UI-thread contract.
        unsafe {
            PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_PAINT,
                WPARAM(0),
                LPARAM(0),
            )?
        };
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        OUTPUTS.with(|outputs| {
            outputs.borrow_mut().remove(&self.hwnd);
        });
    }
}
pub(crate) fn mode(hwnd: HWND) -> Option<(bool, bool)> {
    OUTPUTS.with(|outputs| {
        outputs
            .borrow()
            .get(&(hwnd.0 as usize))
            .map(|e| (e.active, e.mirror))
    })
}
pub(crate) fn submit(hwnd: HWND, texture: &ID3D11Texture2D, width: u32, height: u32) {
    OUTPUTS.with(|outputs| {
        if let Some(entry) = outputs.borrow_mut().get_mut(&(hwnd.0 as usize)) {
            (entry.sink)(texture, width, height);
        }
    });
}
