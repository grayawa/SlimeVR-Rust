#[cfg(any(windows, all(target_os = "linux", feature = "desktop")))]
use std::sync::mpsc;
use std::sync::mpsc::Receiver;
#[derive(Clone, Copy)]
pub enum TrayAction {
    Show,
    Hide,
    Quit,
}
#[cfg(windows)]
pub struct Tray {
    _icon: tray_icon::TrayIcon,
    items: Vec<tray_icon::menu::MenuItem>,
    receiver: Receiver<TrayAction>,
}
#[cfg(all(target_os = "linux", feature = "desktop"))]
mod linux;
#[cfg(all(target_os = "linux", feature = "desktop"))]
pub struct Tray {
    handle: ksni::blocking::Handle<linux::Notifier>,
    online: std::sync::Arc<std::sync::atomic::AtomicBool>,
    receiver: Receiver<TrayAction>,
}
#[cfg(not(any(windows, all(target_os = "linux", feature = "desktop"))))]
pub struct Tray {
    receiver: Receiver<TrayAction>,
}
impl Tray {
    pub fn new(labels: [String; 3]) -> Result<Self, String> {
        #[cfg(windows)]
        {
            use tray_icon::{
                Icon, TrayIconBuilder,
                menu::{Menu, MenuEvent, MenuItem},
            };
            let menu = Menu::new();
            let items: Vec<_> = ["show", "hide", "quit"]
                .into_iter()
                .zip(&labels)
                .map(|(id, label)| MenuItem::with_id(id, label, true, None))
                .collect();
            for item in &items {
                menu.append(item).map_err(|e| e.to_string())?;
            }
            let image =
                image::load_from_memory(include_bytes!("../../gui/src-tauri/icons/128x128.png"))
                    .map_err(|e| e.to_string())?
                    .into_rgba8();
            let (width, height) = image.dimensions();
            let icon =
                Icon::from_rgba(image.into_raw(), width, height).map_err(|e| e.to_string())?;
            let (sender, receiver) = mpsc::channel();
            let click = sender.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = match event.id.0.as_str() {
                    "show" => TrayAction::Show,
                    "hide" => TrayAction::Hide,
                    "quit" => TrayAction::Quit,
                    _ => return,
                };
                let _ = sender.send(action);
            }));
            tray_icon::TrayIconEvent::set_event_handler(Some(
                move |event: tray_icon::TrayIconEvent| {
                    if matches!(event, tray_icon::TrayIconEvent::DoubleClick { .. }) {
                        let _ = click.send(TrayAction::Show);
                    }
                },
            ));
            let icon = TrayIconBuilder::new()
                .with_tooltip("SlimeVR-Rust — Independent Development Preview")
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .build()
                .map_err(|e| e.to_string())?;
            Ok(Self {
                _icon: icon,
                items,
                receiver,
            })
        }
        #[cfg(all(target_os = "linux", feature = "desktop"))]
        {
            use ksni::blocking::TrayMethods;
            let (sender, receiver) = mpsc::channel();
            let online = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let notifier = linux::Notifier::new(labels, sender, online.clone())?;
            let handle = notifier.spawn().map_err(|error| error.to_string())?;
            Ok(Self {
                handle,
                online,
                receiver,
            })
        }
        #[cfg(not(any(windows, all(target_os = "linux", feature = "desktop"))))]
        {
            let _ = labels;
            Err("System tray requires Windows or a Linux StatusNotifier host".into())
        }
    }
    pub fn action(&self) -> Option<TrayAction> {
        self.receiver.try_recv().ok()
    }
    pub fn labels(&self, labels: [String; 3]) {
        #[cfg(windows)]
        for (item, label) in self.items.iter().zip(labels) {
            item.set_text(label);
        }
        #[cfg(all(target_os = "linux", feature = "desktop"))]
        self.handle.update(|notifier| notifier.labels = labels);
        #[cfg(not(any(windows, all(target_os = "linux", feature = "desktop"))))]
        let _ = labels;
    }
    /// Reports whether the desktop host can expose this tray's actions.
    pub fn available(&self) -> bool {
        #[cfg(windows)]
        {
            true
        }
        #[cfg(all(target_os = "linux", feature = "desktop"))]
        {
            self.online.load(std::sync::atomic::Ordering::Acquire) && !self.handle.is_closed()
        }
        #[cfg(not(any(windows, all(target_os = "linux", feature = "desktop"))))]
        {
            false
        }
    }
}
#[cfg(all(target_os = "linux", feature = "desktop"))]
impl Drop for Tray {
    fn drop(&mut self) {
        self.handle.shutdown();
    }
}
#[cfg(all(windows, feature = "desktop"))]
pub fn visible(window: &gpui_kit::Window, show: bool) {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                handle.hwnd.get() as _,
                if show {
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_RESTORE
                } else {
                    windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE
                },
            );
        }
    }
}
#[cfg(all(not(windows), feature = "desktop"))]
pub fn visible(window: &gpui_kit::Window, show: bool) {
    if show {
        window.activate_window();
    } else {
        window.minimize_window();
    }
}

#[cfg(all(windows, feature = "desktop"))]
pub fn is_visible(window: &gpui_kit::Window) -> bool {
    use raw_window_handle::HasWindowHandle;
    if let Ok(handle) = HasWindowHandle::window_handle(window)
        && let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        unsafe {
            return windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible(
                handle.hwnd.get() as _
            ) != 0
                && windows_sys::Win32::UI::WindowsAndMessaging::IsIconic(handle.hwnd.get() as _)
                    == 0;
        }
    }
    true
}
#[cfg(all(not(windows), feature = "desktop"))]
pub fn is_visible(window: &gpui_kit::Window) -> bool {
    window.is_visible()
}
