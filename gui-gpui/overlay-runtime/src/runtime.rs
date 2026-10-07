use super::Event;
use std::{
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn svro_open(
        dll: *const u16,
        key: *const c_char,
        title: *const c_char,
        width: f32,
        error: *mut c_char,
        capacity: usize,
    ) -> *mut c_void;
    fn svro_close(session: *mut c_void);
    fn svro_visible(session: *mut c_void) -> i32;
    fn svro_set_width(session: *mut c_void, width: f32) -> i32;
    fn svro_adapter(session: *mut c_void) -> i32;
    fn svro_poll(session: *mut c_void, event: *mut Event) -> i32;
    fn svro_submit(session: *mut c_void, texture: *mut c_void, width: u32, height: u32) -> i32;
    fn svro_thumbnail(session: *mut c_void, rgba: *mut c_void, width: u32, height: u32) -> i32;
}
/// An owned UI-thread session. The marker prevents moving OpenVR/graphics state
/// to another thread. Closing it never shuts down SlimeVR or SteamVR itself.
pub struct Runtime {
    session: NonNull<c_void>,
    _thread: PhantomData<Rc<()>>,
}
impl Runtime {
    pub fn open(dll: &Path, key: &str, title: &str, width: f32) -> Result<Self, String> {
        if !width.is_finite() || !(0.5..=3.).contains(&width) {
            return Err("Invalid overlay width".into());
        }
        let dll = dll
            .canonicalize()
            .map_err(|e| format!("OpenVR DLL {}: {e}", dll.display()))?;
        let mut dll: Vec<u16> = dll.as_os_str().encode_wide().collect();
        if dll.contains(&0) {
            return Err("Invalid DLL path".into());
        }
        dll.push(0);
        let key = CString::new(key).map_err(|e| e.to_string())?;
        let title = CString::new(title).map_err(|e| e.to_string())?;
        let mut error = [0i8; 256];
        // SAFETY: all strings remain live and terminated; error is writable.
        let session = unsafe {
            svro_open(
                dll.as_ptr(),
                key.as_ptr(),
                title.as_ptr(),
                width,
                error.as_mut_ptr(),
                error.len(),
            )
        };
        let session = NonNull::new(session).ok_or_else(|| {
            unsafe { CStr::from_ptr(error.as_ptr()) }
                .to_string_lossy()
                .into_owned()
        })?;
        Ok(Self {
            session,
            _thread: PhantomData,
        })
    }
    pub fn visible(&self) -> bool {
        // SAFETY: session is live and only accessed from its owning thread.
        unsafe { svro_visible(self.session.as_ptr()) != 0 }
    }
    pub fn set_width(&mut self, width: f32) -> Result<(), String> {
        if !width.is_finite() || !(0.5..=3.).contains(&width) {
            return Err("Invalid overlay width".into());
        }
        // SAFETY: session is live and owned by this UI thread.
        let code = unsafe { svro_set_width(self.session.as_ptr(), width) };
        if code == 0 {
            Ok(())
        } else {
            Err(format!("SteamVR overlay resize failed ({code})"))
        }
    }
    pub fn adapter_index(&self) -> Option<u32> {
        // SAFETY: the owned IVRSystem is live on this thread.
        u32::try_from(unsafe { svro_adapter(self.session.as_ptr()) }).ok()
    }
    pub fn poll(&mut self) -> Option<Event> {
        let mut event = Event::default();
        // SAFETY: output has the C ABI layout and session is live.
        (unsafe { svro_poll(self.session.as_ptr(), &mut event) } != 0).then_some(event)
    }
    /// # Safety
    /// `texture` must be a live ID3D11Texture2D on the submitting UI thread.
    /// Its GPU must match SteamVR's compositor; ownership remains with caller.
    pub unsafe fn submit(
        &mut self,
        texture: *mut c_void,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        if texture.is_null() || width == 0 || height == 0 {
            return Err("Invalid overlay texture".into());
        }
        // SAFETY: follows the caller's texture/lifetime contract.
        let code = unsafe { svro_submit(self.session.as_ptr(), texture, width, height) };
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "SteamVR texture submission failed ({code}); check compositor GPU and texture format"
            ))
        }
    }
    pub fn thumbnail(&mut self, rgba: &[u8], width: u32, height: u32) -> Result<(), String> {
        if width == 0
            || height == 0
            || (width as usize)
                .checked_mul(height as usize)
                .and_then(|n| n.checked_mul(4))
                != Some(rgba.len())
        {
            return Err("Invalid thumbnail dimensions".into());
        }
        // SAFETY: exact buffer length checked above; OpenVR copies the data.
        let code = unsafe {
            svro_thumbnail(
                self.session.as_ptr(),
                rgba.as_ptr().cast_mut().cast(),
                width,
                height,
            )
        };
        if code == 0 {
            Ok(())
        } else {
            Err(format!("SteamVR thumbnail failed ({code})"))
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // SAFETY: exactly one owned session is closed, on the opening thread.
        unsafe { svro_close(self.session.as_ptr()) };
    }
}
