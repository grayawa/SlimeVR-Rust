//! Original Windows global keybindings, with an owned message thread and live rebinding.
use crate::api::protocol::rpc_frame;
use serde::{Deserialize, Serialize};
use solarxr_protocol::rpc;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Full,
    Yaw,
    Mounting,
    Pause,
    Feet,
}
#[cfg(windows)]
const ACTIONS: [Action; 5] = [
    Action::Full,
    Action::Yaw,
    Action::Mounting,
    Action::Pause,
    Action::Feet,
];
const NAMES: [&str; 5] = [
    "fullReset",
    "yawReset",
    "mountingReset",
    "pauseTracking",
    "feetMountingReset",
];
const LABELS: [&str; 5] = [
    "settings-keybinds-full_reset",
    "settings-keybinds-yaw_reset",
    "settings-keybinds-mounting_reset",
    "settings-keybinds-pause_tracking",
    "settings-keybinds-feet_mounting_reset",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub value: String,
    pub delay_ms: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub bindings: [Binding; 5],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            bindings: ["Y", "U", "I", "O", "P"].map(|key| Binding {
                value: format!("CTRL+ALT+SHIFT+{key}"),
                delay_ms: 0,
            }),
        }
    }
}
impl Settings {
    pub fn read(root: &serde_yaml_ng::Value) -> Result<Self, String> {
        let mut s = Self::default();
        for (index, name) in NAMES.into_iter().enumerate() {
            if let Some(value) = root["keybindings"][format!("{name}Binding")].as_str() {
                s.bindings[index].value = value.into();
            }
            if let Some(delay) = root["keybindings"][format!("{name}Delay")].as_u64() {
                s.bindings[index].delay_ms = delay;
            }
            if s.bindings[index].delay_ms > 60000 {
                return Err("keybind delay exceeds 60 seconds".into());
            }
            parse(&s.bindings[index].value)?;
        }
        Ok(s)
    }
    pub fn write(&self, root: &mut serde_yaml_ng::Value) -> Result<(), String> {
        for (index, name) in NAMES.into_iter().enumerate() {
            crate::config::put(
                root,
                &["keybindings", &format!("{name}Binding")],
                &self.bindings[index].value,
            )
            .map_err(|e| e.to_string())?;
            crate::config::put(
                root,
                &["keybindings", &format!("{name}Delay")],
                self.bindings[index].delay_ms,
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
pub fn parse(value: &str) -> Result<Option<(u32, u32)>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 128 {
        return Err("keybind is too long".into());
    }
    let mut modifiers = 0;
    let mut key = None;
    for token in value.split('+') {
        let token = token.trim().to_ascii_uppercase();
        let modifier = match token.as_str() {
            "ALT" => 1,
            "CTRL" | "CONTROL" => 2,
            "SHIFT" => 4,
            "WIN" | "META" | "SUPER" => 8,
            _ => 0,
        };
        if modifier != 0 {
            if modifiers & modifier != 0 {
                return Err("duplicate keybind modifier".into());
            }
            modifiers |= modifier;
            continue;
        }
        let code = if token.len() == 1 && token.as_bytes()[0].is_ascii_alphanumeric() {
            u32::from(token.as_bytes()[0])
        } else if let Some(n) = token
            .strip_prefix('F')
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|n| (1..=24).contains(n))
        {
            0x6f + n
        } else {
            match token.as_str() {
                "SPACE" => 0x20,
                "ENTER" | "RETURN" => 0x0d,
                "TAB" => 9,
                "ESC" | "ESCAPE" => 0x1b,
                "BACKSPACE" => 8,
                "DELETE" => 0x2e,
                "INSERT" => 0x2d,
                "HOME" => 0x24,
                "END" => 0x23,
                "PAGEUP" => 0x21,
                "PAGEDOWN" => 0x22,
                "LEFT" => 0x25,
                "UP" => 0x26,
                "RIGHT" => 0x27,
                "DOWN" => 0x28,
                _ => return Err(format!("unknown keybind key {token}")),
            }
        };
        if key.replace(code).is_some() {
            return Err("keybind needs exactly one key".into());
        }
    }
    Ok(Some((modifiers, key.ok_or("keybind is missing a key")?)))
}
pub fn frame(tx: u32, settings: &Settings) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::KeybindResponse, tx, |f| {
        let mut build = |s: &Settings| {
            let values = s
                .bindings
                .iter()
                .enumerate()
                .map(|(index, b)| {
                    let name = f.create_string(LABELS[index]);
                    let value = f.create_string(&b.value);
                    rpc::Keybind::create(
                        f,
                        &rpc::KeybindArgs {
                            keybind_id: rpc::KeybindId(index as u8),
                            keybind_name_id: Some(name),
                            keybind_value: Some(value),
                            keybind_delay: b.delay_ms as f32 / 1000.0,
                        },
                    )
                })
                .collect::<Vec<_>>();
            f.create_vector(&values)
        };
        let current = build(settings);
        let defaults = build(&Settings::default());
        rpc::KeybindResponse::create(
            f,
            &rpc::KeybindResponseArgs {
                keybind: Some(current),
                default_keybinds: Some(defaults),
            },
        )
        .as_union_value()
    })
}
pub fn change(root: &mut serde_yaml_ng::Value, r: rpc::Keybind<'_>) -> Result<(), String> {
    let mut settings = Settings::read(root)?;
    let index = usize::from(r.keybind_id().0);
    if index >= 5 {
        return Err("unknown keybind".into());
    }
    let value = r.keybind_value().ok_or("missing keybind value")?;
    parse(value)?;
    let delay = r.keybind_delay();
    if !delay.is_finite() || !(0.0..=60.0).contains(&delay) {
        return Err("keybind delay outside 0..60 seconds".into());
    }
    settings.bindings[index] = Binding {
        value: value.into(),
        delay_ms: (delay * 1000.0).round() as u64,
    };
    settings.write(root)
}
pub struct Controller {
    settings: std::sync::Arc<std::sync::Mutex<Settings>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Controller {
    pub fn start(
        settings: Settings,
        commands: tokio::sync::mpsc::Sender<crate::api::Request>,
    ) -> std::io::Result<Self> {
        let settings = std::sync::Arc::new(std::sync::Mutex::new(settings));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(windows)]
        let thread = {
            let settings = settings.clone();
            let stop = stop.clone();
            Some(
                std::thread::Builder::new()
                    .name("SlimeVR hotkeys".into())
                    .spawn(move || windows_thread(settings, stop, commands))?,
            )
        };
        #[cfg(not(windows))]
        let thread = {
            let _ = commands;
            None
        };
        Ok(Self {
            settings,
            stop,
            thread,
        })
    }
    pub fn configure(&self, settings: Settings) {
        *self.settings.lock().unwrap() = settings;
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
#[cfg(windows)]
fn windows_thread(
    settings: std::sync::Arc<std::sync::Mutex<Settings>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    commands: tokio::sync::mpsc::Sender<crate::api::Request>,
) {
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT},
        WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY},
    };
    let mut current = None;
    let mut registered = vec![];
    while !stop.load(std::sync::atomic::Ordering::Acquire) && !commands.is_closed() {
        let next = settings.lock().unwrap().clone();
        if current.as_ref() != Some(&next) {
            for id in registered.drain(..) {
                unsafe {
                    UnregisterHotKey(std::ptr::null_mut(), id);
                }
            }
            for (index, b) in next.bindings.iter().enumerate() {
                if let Ok(Some((mods, key))) = parse(&b.value) {
                    if unsafe {
                        RegisterHotKey(
                            std::ptr::null_mut(),
                            index as i32 + 1,
                            mods | MOD_NOREPEAT,
                            key,
                        )
                    } != 0
                    {
                        registered.push(index as i32 + 1);
                    } else {
                        let _ = commands.try_send(crate::api::Request::HotkeyError(format!(
                            "Unable to register {}",
                            b.value
                        )));
                    }
                }
            }
            current = Some(next);
        }
        let mut message: MSG = unsafe { std::mem::zeroed() };
        while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_HOTKEY {
                let index = message.wParam as usize;
                if (1..=5).contains(&index) {
                    let delay = current.as_ref().unwrap().bindings[index - 1].delay_ms;
                    let _ = commands.try_send(crate::api::Request::Hotkey {
                        action: ACTIONS[index - 1],
                        delay_ms: delay,
                    });
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    for id in registered {
        unsafe {
            UnregisterHotKey(std::ptr::null_mut(), id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bindings_validate_and_roundtrip_original_yaml() {
        assert_eq!(parse("CTRL+ALT+SHIFT+Y").unwrap(), Some((7, 89)));
        assert_eq!(parse("win+F24").unwrap(), Some((8, 135)));
        assert_eq!(parse("").unwrap(), None);
        for invalid in ["CTRL+CTRL+Y", "CTRL", "Y+U", "CTRL+F25"] {
            assert!(parse(invalid).is_err());
        }
        let mut root=serde_yaml_ng::from_str("keybindings:\n  fullResetBinding: CTRL+Y\n  fullResetDelay: 1250\n  other: preserved\n").unwrap();
        let s = Settings::read(&root).unwrap();
        assert_eq!(s.bindings[0].delay_ms, 1250);
        s.write(&mut root).unwrap();
        assert_eq!(Settings::read(&root).unwrap(), s);
        assert_eq!(root["keybindings"]["other"].as_str(), Some("preserved"));
    }
}
