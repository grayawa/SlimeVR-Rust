#![cfg(all(target_os = "linux", feature = "desktop"))]
use slimevr_gpui::tray::{Tray, TrayAction};
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

struct Watcher(mpsc::Sender<String>);
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(&self, service: String) {
        self.0.send(service).unwrap();
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "D-Bus state transition timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
#[ignore = "Run inside a dedicated dbus-run-session"]
fn status_notifier_lifecycle_and_menu() {
    let (registered, services) = mpsc::channel();
    let watcher = zbus::blocking::connection::Builder::session()
        .unwrap()
        .name("org.kde.StatusNotifierWatcher")
        .unwrap()
        .serve_at("/StatusNotifierWatcher", Watcher(registered))
        .unwrap()
        .build()
        .unwrap();
    let tray = Tray::new(["Show".into(), "Hide".into(), "Quit".into()]).unwrap();
    let service = services.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(tray.available());
    let connection = zbus::blocking::Connection::session().unwrap();
    let item = zbus::blocking::Proxy::new(
        &connection,
        service.as_str(),
        "/StatusNotifierItem",
        "org.kde.StatusNotifierItem",
    )
    .unwrap();
    let icon: Vec<(i32, i32, Vec<u8>)> = item.get_property("IconPixmap").unwrap();
    assert_eq!((icon[0].0, icon[0].1), (128, 128));
    let rgba = image::load_from_memory(include_bytes!("../../gui/src-tauri/icons/128x128.png"))
        .unwrap()
        .into_rgba8();
    assert_eq!(icon[0].2.len(), rgba.len());
    for (argb, pixel) in icon[0].2.as_chunks::<4>().0.iter().zip(rgba.pixels()) {
        assert_eq!(argb, &[pixel[3], pixel[0], pixel[1], pixel[2]]);
    }
    item.call::<_, _, ()>("Activate", &(0i32, 0i32)).unwrap();
    wait_until(|| matches!(tray.action(), Some(TrayAction::Show)));
    let menu = zbus::blocking::Proxy::new(
        &connection,
        service.as_str(),
        "/MenuBar",
        "com.canonical.dbusmenu",
    )
    .unwrap();
    tray.labels(["显示".into(), "隐藏".into(), "退出".into()]);
    wait_until(|| {
        let label: zbus::zvariant::OwnedValue = menu.call("GetProperty", &(1i32, "label")).unwrap();
        String::try_from(label).unwrap() == "显示"
    });
    for (id, action) in [
        (1, TrayAction::Show),
        (2, TrayAction::Hide),
        (3, TrayAction::Quit),
    ] {
        menu.call::<_, _, ()>(
            "Event",
            &(id, "clicked", zbus::zvariant::Value::from(0i32), 0u32),
        )
        .unwrap();
        wait_until(|| {
            matches!(
                (tray.action(), action),
                (Some(TrayAction::Show), TrayAction::Show)
                    | (Some(TrayAction::Hide), TrayAction::Hide)
                    | (Some(TrayAction::Quit), TrayAction::Quit)
            )
        });
    }
    drop(watcher);
    wait_until(|| !tray.available());
    drop(tray);
    let bus = zbus::blocking::fdo::DBusProxy::new(&connection).unwrap();
    wait_until(|| {
        !bus.name_has_owner(service.as_str().try_into().unwrap())
            .unwrap()
    });
    assert!(Tray::new(["Show".into(), "Hide".into(), "Quit".into()]).is_err());
}
