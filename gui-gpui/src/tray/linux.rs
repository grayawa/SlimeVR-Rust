use super::TrayAction;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::Sender,
};

/// Owns the StatusNotifier data served by ksni's background D-Bus executor.
pub(super) struct Notifier {
    pub labels: [String; 3],
    sender: Sender<TrayAction>,
    online: Arc<AtomicBool>,
    icon: ksni::Icon,
}

impl Notifier {
    pub fn new(
        labels: [String; 3],
        sender: Sender<TrayAction>,
        online: Arc<AtomicBool>,
    ) -> Result<Self, String> {
        let image =
            image::load_from_memory(include_bytes!("../../../gui/src-tauri/icons/128x128.png"))
                .map_err(|error| error.to_string())?
                .into_rgba8();
        let (width, height) = image.dimensions();
        // StatusNotifier pixmaps carry ARGB bytes in network byte order.
        let data = image
            .pixels()
            .flat_map(|pixel| {
                let [red, green, blue, alpha] = pixel.0;
                [alpha, red, green, blue]
            })
            .collect();
        Ok(Self {
            labels,
            sender,
            online,
            icon: ksni::Icon {
                width: width as i32,
                height: height as i32,
                data,
            },
        })
    }
}

impl ksni::Tray for Notifier {
    fn id(&self) -> String {
        "slimevr-rust".into()
    }
    fn title(&self) -> String {
        "SlimeVR-Rust — Independent Development Preview".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.sender.send(TrayAction::Show);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        [TrayAction::Show, TrayAction::Hide, TrayAction::Quit]
            .into_iter()
            .zip(&self.labels)
            .map(|(action, label)| {
                ksni::menu::StandardItem {
                    label: label.clone(),
                    activate: Box::new(move |notifier: &mut Self| {
                        let _ = notifier.sender.send(action);
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect()
    }
    fn watcher_online(&self) {
        self.online.store(true, Ordering::Release);
    }
    fn watcher_offline(&self, _reason: ksni::OfflineReason) -> bool {
        self.online.store(false, Ordering::Release);
        true
    }
}
