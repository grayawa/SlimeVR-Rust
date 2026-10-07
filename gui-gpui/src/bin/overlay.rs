#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#[cfg(windows)]
#[path = "overlay/native.rs"]
mod native;
use clap::Parser;
use gpui_kit::{
    component::{
        ActiveTheme, Disableable, StyledExt,
        button::{Button, ButtonVariants},
    },
    prelude::FluentBuilder as _,
    *,
};
use slimevr_gpui::{
    client::{Client, Connection, Snapshot},
    dashboard,
    desktop::{Paths, Preferences},
    i18n::Localizer,
    log_level::LogLevel,
    protocol::{Bone, Command, Feed, ResetKind, Tracker},
    tracker_list,
    ui::{
        components::card,
        theme::{self, Surface},
    },
    visualization::Camera,
};
use solarxr_protocol::datatypes::{BodyPart, TrackerStatus};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime},
};

#[derive(Parser)]
#[command(about = "SlimeVR SteamVR dashboard: resets, skeleton and tracker status")]
struct Options {
    #[arg(long, default_value = "ws://127.0.0.1:21110")]
    url: String,
    #[arg(long)]
    locale: Option<String>,
    /// Show a desktop window too, for layout and interaction verification.
    #[arg(long)]
    preview: bool,
    /// Show sample data only; never connects to or changes a backend.
    #[arg(long)]
    demo: bool,
    #[arg(long)]
    openvr_dll: Option<PathBuf>,
    /// Override the remembered panel width (0.5–3 meters; default 1.8).
    #[arg(long)]
    width_meters: Option<f32>,
    #[arg(long, value_enum, default_value = "info")]
    log_level: LogLevel,
}
struct Panel {
    client: Option<Client>,
    snapshot: Snapshot,
    l10n: Localizer,
    camera: Camera,
    drag: Option<Point<Pixels>>,
    mirror: bool,
    demo: bool,
    visible: bool,
    #[cfg(windows)]
    preview: bool,
    feed_visible: bool,
    error: Option<String>,
    host_error: Option<String>,
    paths: Paths,
    preferences: Preferences,
    preferences_stamp: Option<(SystemTime, u64)>,
    next_preferences_check: Instant,
    preferences_error: Option<String>,
    width_meters: f32,
    #[cfg(windows)]
    host: Option<native::Host>,
}
impl Panel {
    fn text(&self, key: &str) -> String {
        self.l10n.text(key)
    }
    fn resize(&mut self, delta: f32, window: &mut Window, cx: &mut Context<Self>) {
        let width = ((self.width_meters + delta) * 10.).round().clamp(5., 30.) / 10.;
        #[cfg(windows)]
        if let Some(host) = &mut self.host
            && let Err(error) = host.set_width(width)
        {
            self.error = Some(error);
            cx.notify();
            window.refresh();
            return;
        }
        self.width_meters = width;
        if !self.demo {
            match dashboard::save_width(&self.paths, width) {
                Ok(preferences) => self.preferences = preferences,
                Err(error) => self.error = Some(error),
            }
        }
        cx.notify();
        window.refresh();
    }
    fn sync_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if Instant::now() < self.next_preferences_check {
            return;
        }
        self.next_preferences_check = Instant::now() + Duration::from_secs(1);
        let stamp = preferences_stamp(&self.preferences.path);
        if stamp == self.preferences_stamp {
            return;
        }
        // Keep retrying partial writes instead of marking unreadable data as seen.
        match Preferences::load(&self.paths) {
            Ok(preferences) => {
                let changed = tracker_list::Settings::from_preferences(&preferences.value)
                    != tracker_list::Settings::from_preferences(&self.preferences.value);
                self.preferences = preferences;
                self.preferences_stamp = stamp;
                self.preferences_error = None;
                if changed {
                    cx.notify();
                    window.refresh();
                }
            }
            Err(error) => {
                if self.preferences_error.as_ref() != Some(&error) {
                    slimevr_gpui::logging::write(LogLevel::Warn, "overlay-preferences", &error);
                    self.preferences_error = Some(error);
                }
            }
        }
    }
    fn send(&mut self, command: Command, cx: &mut Context<Self>) {
        self.error = self
            .client
            .as_ref()
            .and_then(|client| client.send(command).err());
        cx.notify();
    }
    fn tracker_item(
        &self,
        tracker: &Tracker,
        settings: tracker_list::Settings,
        cx: &Context<Self>,
    ) -> AnyElement {
        let connected = self.snapshot.connection == Connection::Connected;
        let label = tracker
            .custom_name
            .as_ref()
            .filter(|n| !n.is_empty())
            .cloned()
            .unwrap_or_else(|| {
                if tracker.body == 0 {
                    tracker.name.clone()
                } else {
                    self.text(&format!(
                        "body_part-{}",
                        BodyPart(tracker.body).variant_name().unwrap_or("NONE")
                    ))
                }
            });
        let status = if connected {
            TrackerStatus(tracker.status)
                .variant_name()
                .unwrap_or("DISCONNECTED")
        } else {
            "DISCONNECTED"
        };
        let battery = slimevr_gpui::battery::Reading::from_tracker(tracker)
            .map(|v| format!("{}%", v.percent))
            .unwrap_or_else(|| "—".into());
        let wifi = tracker
            .rssi
            .map(|v| format!("{v} dBm"))
            .unwrap_or_else(|| "—".into());
        let ping = tracker
            .ping
            .map(|v| format!("{v} ms"))
            .unwrap_or_else(|| "—".into());
        let title = div()
            .h_flex()
            .gap_2()
            .child(div().size(px(10.)).flex_shrink_0().rounded_full().bg(
                if connected && tracker.status == 2 {
                    rgb(0x50e897)
                } else {
                    rgb(0xdf6d8c)
                },
            ))
            .child(div().flex_1().min_w_0().truncate().font_bold().child(label));
        let name =
            div()
                .v_flex()
                .flex_1()
                .min_w_0()
                .gap_1()
                .child(title)
                .when(!settings.table, |d| {
                    d.child(
                        div()
                            .text_size(px(14.))
                            .text_color(cx.theme().muted_foreground)
                            .child(self.text(&format!("tracker-status-{}", status.to_lowercase()))),
                    )
                });
        let content = if settings.table {
            div()
                .h_flex()
                .gap_2()
                .w_full()
                .child(name)
                .child(div().w(px(70.)).flex_shrink_0().child(battery))
                .child(div().w(px(80.)).flex_shrink_0().child(ping))
                .child(div().w(px(100.)).flex_shrink_0().child(wifi))
        } else {
            div()
                .v_flex()
                .gap_1()
                .w_full()
                .child(name)
                .child(div().text_size(px(17.)).child(format!(
                    "{} {battery} · {ping}",
                    self.text("native-battery")
                )))
                .child(
                    div()
                        .text_size(px(17.))
                        .child(format!("{} {wifi}", self.text("native-wifi-signal"))),
                )
        };
        card(Surface::Control, cx)
            .v_flex()
            .min_w_0()
            .p_2()
            .gap_2()
            .text_size(px(18.))
            .line_height(relative(1.15))
            .child(content)
            .when(settings.more_info, |d| {
                let tps = tracker
                    .tps
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into());
                let temperature = tracker
                    .temperature
                    .filter(|v| v.is_finite())
                    .map(|v| format!("{v:.1} °C"))
                    .unwrap_or_else(|| "—".into());
                let voltage = tracker
                    .voltage
                    .filter(|v| v.is_finite())
                    .map(|v| format!("{v:.2} V"))
                    .unwrap_or_else(|| "—".into());
                d.child(
                    div()
                        .text_size(px(16.))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("TPS {tps} · {temperature} · {voltage}")),
                )
            })
            .into_any_element()
    }
    fn skeleton(&self, cx: &mut Context<Self>) -> AnyElement {
        let bones: Vec<_> = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| {
                f.bones
                    .iter()
                    .filter(|b| b.head.iter().chain(b.tail.iter()).all(|v| v.is_finite()))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let camera = self.camera;
        let mirror = self.mirror;
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for b in &bones {
            for p in [b.head, b.tail] {
                for i in 0..3 {
                    lo[i] = lo[i].min(p[i]);
                    hi[i] = hi[i].max(p[i]);
                }
            }
        }
        let center = if bones.is_empty() {
            [0., 0.9, 0.]
        } else {
            std::array::from_fn(|i| (lo[i] + hi[i]) * 0.5)
        };
        let span = if bones.is_empty() {
            2.
        } else {
            (0..3).map(|i| hi[i] - lo[i]).fold(0.4, f32::max)
        };
        div()
            .id("dashboard-skeleton")
            .flex_1()
            .w_full()
            .min_h(px(250.))
            .overflow_hidden()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, _| this.drag = Some(event.position)),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag = None),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                    if let Some(old) = this.drag.replace(event.position) {
                        this.camera.orbit(
                            (event.position.x - old.x).into(),
                            (event.position.y - old.y).into(),
                        );
                        cx.notify();
                        window.refresh();
                    }
                } else {
                    this.drag = None;
                }
            }))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.camera
                    .zoom(f32::from(event.delta.pixel_delta(px(20.)).y) * 0.1);
                cx.stop_propagation();
                cx.notify();
                window.refresh();
            }))
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let w = f32::from(bounds.size.width);
                        let h = f32::from(bounds.size.height);
                        let scale = ((h - 30.) / span).min((w - 30.) / span).max(1.);
                        let project = |mut p: [f32; 3]| {
                            let mut c = center;
                            if mirror {
                                p[0] = -p[0];
                                c[0] = -c[0];
                            }
                            let p = camera.project(p, c, scale);
                            point(
                                bounds.origin.x + px(w * 0.5 + p[0]),
                                bounds.origin.y + px(h * 0.5 + p[1]),
                            )
                        };
                        for b in &bones {
                            let mut path = PathBuilder::stroke(px(4.));
                            path.move_to(project(b.head));
                            path.line_to(project(b.tail));
                            if let Ok(path) = path.build() {
                                window.paint_path(
                                    path,
                                    rgb(if b.body == BodyPart::HEAD.0 {
                                        0xe0c568
                                    } else {
                                        0xb994d8
                                    }),
                                );
                            }
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }
}
impl Render for Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connected = self.snapshot.connection == Connection::Connected;
        let status = match self.snapshot.connection {
            Connection::Connected => "native-connected",
            Connection::Connecting => "native-connecting",
            Connection::Disconnected => "native-disconnected",
        };
        let mut buttons = div().h_flex().w_full().gap_3();
        for kind in [ResetKind::Full, ResetKind::Yaw, ResetKind::Mounting] {
            buttons = buttons.child(
                Button::new(format!("dashboard-{}", kind.label()))
                    .label(self.text(kind.label()))
                    .flex_1()
                    .min_w_0()
                    .h(px(60.))
                    .primary()
                    .disabled(self.demo || !dashboard::reset_allowed(&self.snapshot, kind))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.send(Command::Reset(kind), cx);
                        window.refresh();
                    })),
            );
        }
        let progress = if self.demo {
            self.text("native-dashboard-demo")
        } else if let Some(reset) = &self.snapshot.reset {
            if reset.done {
                self.text("native-reset-finished")
            } else {
                format!(
                    "{} · {:.1}s",
                    self.text("native-dashboard-resetting"),
                    (reset.duration_ms.saturating_sub(reset.progress_ms)).max(0) as f32 / 1000.
                )
            }
        } else if self.snapshot.pending.is_some() {
            self.text("native-operation-pending")
        } else {
            self.text("native-dashboard-reset-hint")
        };
        let mut controls = div().h_flex().gap_2();
        for (key, yaw, pitch) in [
            ("front", 0., 0.),
            ("side", std::f32::consts::FRAC_PI_2, 0.),
            ("top", 0., std::f32::consts::FRAC_PI_2),
        ] {
            controls = controls.child(
                Button::new(format!("dashboard-view-{key}"))
                    .label(self.text(&format!("native-camera-{key}")))
                    .h(px(44.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.camera = Camera {
                            yaw,
                            pitch,
                            zoom: this.camera.zoom,
                            ..Default::default()
                        };
                        cx.notify();
                        window.refresh();
                    })),
            );
        }
        controls = controls.child(
            Button::new("dashboard-mirror")
                .label(self.text("native-camera-mirror"))
                .h(px(44.))
                .when(self.mirror, |b| b.primary())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.mirror = !this.mirror;
                    cx.notify();
                    window.refresh();
                })),
        );
        let mut zoom = div().h_flex().gap_2();
        for (id, label, delta) in [("out", "−", -20.), ("in", "+", 20.)] {
            zoom = zoom.child(
                Button::new(format!("dashboard-zoom-{id}"))
                    .label(label)
                    .h(px(44.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.camera.zoom(delta);
                        cx.notify();
                        window.refresh();
                    })),
            );
        }
        zoom = zoom.child(
            Button::new("dashboard-zoom-fit")
                .label(self.text("native-dashboard-fit-skeleton"))
                .h(px(44.))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.camera.zoom = dashboard::DEFAULT_SKELETON_ZOOM;
                    this.camera.pan = [0.; 2];
                    cx.notify();
                    window.refresh();
                })),
        );
        let bones = self
            .snapshot
            .feed
            .as_ref()
            .is_some_and(|f| !f.bones.is_empty());
        let skeleton = card(Surface::Panel, cx)
            .v_flex()
            .p_4()
            .gap_3()
            .w(px(360.))
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .child(self.text("native-skeleton"))
            .child(controls)
            .child(zoom)
            .child(self.skeleton(cx))
            .when(!bones || !connected, |d| {
                d.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.text("native-waiting")),
                )
            });
        let list = tracker_list::Settings::from_preferences(&self.preferences.value);
        let mut nodes = div().v_flex().gap_3();
        let groups = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| list.groups(f))
            .unwrap_or_default();
        if groups.iter().all(Vec::is_empty) {
            nodes = nodes.child(self.text("native-no-trackers"));
        }
        for (index, trackers) in groups.into_iter().enumerate() {
            if trackers.is_empty() {
                continue;
            }
            nodes = nodes.child(div().font_bold().text_size(px(18.)).child(self.text(
                if index == 0 {
                    "native-assigned-trackers"
                } else {
                    "native-unassigned-trackers"
                },
            )));
            if list.table {
                nodes = nodes.child(
                    div()
                        .h_flex()
                        .gap_2()
                        .px_3()
                        .text_size(px(16.))
                        .text_color(cx.theme().muted_foreground)
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(self.text("tracker-table-column-name")),
                        )
                        .child(
                            div()
                                .w(px(70.))
                                .flex_shrink_0()
                                .child(self.text("native-battery")),
                        )
                        .child(
                            div()
                                .w(px(80.))
                                .flex_shrink_0()
                                .child(self.text("native-ping")),
                        )
                        .child(
                            div()
                                .w(px(100.))
                                .flex_shrink_0()
                                .child(self.text("native-wifi-signal")),
                        ),
                );
            }
            let mut items = div()
                .w_full()
                .gap_3()
                .when(list.table, |d| d.v_flex())
                .when(!list.table, |d| d.grid().grid_cols(2));
            for tracker in trackers {
                items = items.child(self.tracker_item(tracker, list, cx));
            }
            nodes = nodes.child(items);
        }
        let nodes = card(Surface::Panel, cx)
            .v_flex()
            .p_4()
            .gap_3()
            .flex_1()
            .min_w_0()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .child(self.text("native-dashboard-nodes"))
            .child(
                div()
                    .id("dashboard-nodes-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(nodes),
            );
        let error = self
            .error
            .clone()
            .or_else(|| self.host_error.clone())
            .or_else(|| self.snapshot.last_error.clone());
        div()
            .v_flex()
            .size_full()
            .overflow_hidden()
            .p_4()
            .gap_3()
            .font_family("Poppins")
            .text_size(px(22.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("dashboard-header")
                    .h_flex()
                    .w_full()
                    .h(px(44.))
                    .flex_shrink_0()
                    .items_center()
                    .gap_3()
                    .child(div().font_bold().flex_1().min_w_0().child("SlimeVR"))
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .child(self.text("native-dashboard-panel-size")),
                            )
                            .child(
                                Button::new("dashboard-size-down")
                                    .label("−")
                                    .h(px(44.))
                                    .disabled(self.width_meters <= 0.5)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.resize(-0.1, window, cx)
                                    })),
                            )
                            .child(
                                div()
                                    .w(px(68.))
                                    .text_size(px(18.))
                                    .child(format!("{:.1} m", self.width_meters)),
                            )
                            .child(
                                Button::new("dashboard-size-up")
                                    .label("+")
                                    .h(px(44.))
                                    .disabled(self.width_meters >= 3.)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.resize(0.1, window, cx)
                                    })),
                            ),
                    )
                    .child(div().text_size(px(18.)).child(self.text(status)))
                    .child(
                        Button::new("dashboard-exit")
                            .label(self.text("native-dashboard-exit"))
                            .h(px(44.))
                            .on_click(|_, _, cx| cx.quit()),
                    ),
            )
            .child(
                card(Surface::Panel, cx)
                    .v_flex()
                    .p_4()
                    .gap_2()
                    .child(buttons)
                    .child(
                        div()
                            .text_size(px(18.))
                            .text_color(cx.theme().muted_foreground)
                            .child(progress),
                    ),
            )
            .when_some(error, |d, error| {
                d.child(
                    div()
                        .text_size(px(16.))
                        .text_color(rgb(0xdf6d8c))
                        .child(error),
                )
            })
            .child(
                div()
                    .h_flex()
                    .w_full()
                    .items_stretch()
                    .gap_3()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .child(skeleton)
                    .child(nodes),
            )
    }
}
fn demo_snapshot() -> Snapshot {
    let endpoints = [
        ([0., 1.7, 0.], [0., 1.4, 0.]),
        ([0., 1.4, 0.], [0., 0.9, 0.]),
        ([0., 0.9, 0.], [-0.16, 0.5, 0.]),
        ([-0.16, 0.5, 0.], [-0.16, 0.05, 0.]),
        ([0., 0.9, 0.], [0.16, 0.5, 0.]),
        ([0.16, 0.5, 0.], [0.16, 0.05, 0.]),
        ([0., 1.4, 0.], [-0.6, 1.15, 0.]),
        ([0., 1.4, 0.], [0.6, 1.15, 0.]),
    ];
    let roles = [
        BodyPart::CHEST,
        BodyPart::HIP,
        BodyPart::LEFT_UPPER_LEG,
        BodyPart::LEFT_LOWER_LEG,
        BodyPart::RIGHT_UPPER_LEG,
        BodyPart::RIGHT_LOWER_LEG,
    ];
    Snapshot {
        connection: Connection::Connected,
        feed: Some(Feed {
            trackers: roles
                .into_iter()
                .enumerate()
                .map(|(i, body)| Tracker {
                    key: slimevr_gpui::protocol::TrackerKey {
                        device: i as u8 + 1,
                        sensor: 0,
                    },
                    name: format!("Demo {}", i + 1),
                    body: body.0,
                    status: 2,
                    is_imu: true,
                    battery: Some(80 - i as u8 * 9),
                    rssi: Some(-45 - i as i16 * 3),
                    ping: Some(5 + i as u16),
                    ..Default::default()
                })
                .collect(),
            bones: endpoints
                .into_iter()
                .map(|(head, tail)| Bone {
                    body: BodyPart::CHEST.0,
                    head,
                    tail,
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    }
}
fn preferences_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}
fn run() -> Result<(), String> {
    let options = Options::parse();
    if options
        .width_meters
        .is_some_and(|width| !dashboard::valid_width(width))
    {
        return Err("--width-meters must be between 0.5 and 3".into());
    }
    let preview = options.preview || options.demo;
    #[cfg(not(windows))]
    if !preview {
        return Err("SteamVR texture output currently supports Windows. Use --preview for desktop validation.".into());
    }
    let paths = Paths::new()?;
    slimevr_gpui::logging::init(paths.logs.join("overlay"), options.log_level);
    // Keep this independent from the desktop client's single-instance lock.
    let _instance = if options.demo {
        None
    } else {
        use fs2::FileExt as _;
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.root.join("overlay.lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|e| format!("Dashboard is already running or cannot acquire its lock: {e}"))?;
        Some(lock)
    };
    let preferences = Preferences::load(&paths)?;
    let width_meters = options
        .width_meters
        .unwrap_or_else(|| dashboard::saved_width(preferences.store_value(dashboard::STORE_KEY)));
    let locale = options.locale.unwrap_or_else(|| {
        preferences.value["lang"]
            .as_str()
            .unwrap_or("en")
            .to_owned()
    });
    let l10n = Localizer::new(&locale)?;
    let mirror = preferences.value["mirrorView"].as_bool().unwrap_or(true);
    let theme_name = preferences.value["theme"]
        .as_str()
        .unwrap_or("slime")
        .to_owned();
    let client = if options.demo {
        None
    } else {
        Some(Client::connect(options.url, options.log_level)?)
    };
    if let Some(client) = &client {
        let f = dashboard::feed_policy(preview);
        client.configure_feed(f.0, f.1, f.2)?;
    }
    let snapshot = if options.demo {
        demo_snapshot()
    } else {
        Snapshot::default()
    };
    #[cfg(windows)]
    let dll = options
        .openvr_dll
        .unwrap_or_else(|| paths.resources.join("openvr_api.dll"));
    #[cfg(windows)]
    let initial = if options.demo {
        None
    } else {
        match slimevr_openvr_overlay::Runtime::open(
            &dll,
            "dev.grayawa.slimevr-rust.dashboard",
            &l10n.text("native-dashboard-title"),
            width_meters,
        ) {
            Ok(runtime) => {
                if let Some(index) = runtime.adapter_index() {
                    gpui_windows::overlay_output::prefer_adapter(index)
                        .map_err(|e| e.to_string())?;
                }
                Some(runtime)
            }
            Err(error) => {
                slimevr_gpui::logging::write(LogLevel::Warn, "overlay", &error);
                None
            }
        }
    };
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            cx.text_system()
                .add_fonts(vec![
                    std::borrow::Cow::Borrowed(
                        include_bytes!("../../assets/fonts/Poppins-Regular.ttf").as_slice(),
                    ),
                    std::borrow::Cow::Borrowed(
                        include_bytes!("../../assets/fonts/Poppins-Bold.ttf").as_slice(),
                    ),
                ])
                .expect("Dashboard fonts");
            theme::apply_theme(&theme_name, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let result = gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::centered(size(px(1100.), px(720.)), cx)),
                    show: preview,
                    focus: preview,
                    window_min_size: Some(size(px(1000.), px(680.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("SlimeVR — Dashboard".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    #[cfg(windows)]
                    let host = if options.demo {
                        None
                    } else {
                        match native::Host::new(
                            window,
                            dll,
                            l10n.text("native-dashboard-title"),
                            width_meters,
                            preview,
                            initial,
                        ) {
                            Ok(host) => Some(host),
                            Err(error) => {
                                slimevr_gpui::logging::write(LogLevel::Error, "overlay", &error);
                                cx.quit();
                                None
                            }
                        }
                    };
                    let entity = cx.new(|_| Panel {
                        client,
                        snapshot,
                        l10n,
                        camera: Camera {
                            zoom: dashboard::DEFAULT_SKELETON_ZOOM,
                            ..Default::default()
                        },
                        drag: None,
                        mirror,
                        demo: options.demo,
                        visible: preview,
                        #[cfg(windows)]
                        preview,
                        feed_visible: preview,
                        error: None,
                        host_error: None,
                        preferences_stamp: preferences_stamp(&preferences.path),
                        preferences,
                        paths,
                        next_preferences_check: Instant::now(),
                        preferences_error: None,
                        width_meters,
                        #[cfg(windows)]
                        host,
                    });
                    window
                        .spawn(cx, {
                            let entity = entity.downgrade();
                            async move |cx| {
                                let mut delay = 16;
                                loop {
                                    smol::Timer::after(Duration::from_millis(delay)).await;
                                    let inputs = entity.update_in(cx, |this, _window, cx| {
                                        this.sync_preferences(_window, cx);
                                        #[allow(unused_mut)]
                                        let mut inputs = Vec::<PlatformInput>::new();
                                        #[cfg(windows)]
                                        if let Some(host) = &mut this.host {
                                            let tick = host.pump(_window);
                                            if tick.quit {
                                                cx.quit();
                                                return (inputs, false);
                                            }
                                            if tick.refresh {
                                                cx.notify();
                                                _window.refresh();
                                            }
                                            this.visible = tick.visible || this.preview;
                                            let error =
                                                host.error.clone().or_else(|| host.texture_error());
                                            if this.host_error != error {
                                                this.host_error = error;
                                                cx.notify();
                                            }
                                            inputs = tick.inputs;
                                            host.request_frame(tick.visible);
                                        }
                                        if let Some(client) = &this.client {
                                            let visible = this.visible;
                                            if visible != this.feed_visible {
                                                this.feed_visible = visible;
                                                let f = dashboard::feed_policy(visible);
                                                if let Err(error) =
                                                    client.configure_feed(f.0, f.1, f.2)
                                                {
                                                    this.error = Some(error);
                                                }
                                                cx.notify();
                                            }
                                            let snapshot = client.snapshot();
                                            if snapshot.revision != this.snapshot.revision {
                                                this.snapshot = snapshot;
                                                if visible {
                                                    cx.notify();
                                                    _window.refresh();
                                                }
                                            }
                                            client.drain_events();
                                        }
                                        (inputs, this.visible)
                                    });
                                    let Ok((inputs, visible)) = inputs else {
                                        break;
                                    };
                                    delay = if visible { 16 } else { 200 };
                                    // Dispatch after releasing the Panel entity borrow: click
                                    // listeners may update that same entity synchronously.
                                    if !inputs.is_empty()
                                        && cx
                                            .update(|window, cx| {
                                                for input in inputs {
                                                    window.dispatch_event(input, cx);
                                                }
                                                window.refresh();
                                            })
                                            .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        })
                        .detach();
                    entity
                },
            );
            if let Err(error) = result {
                slimevr_gpui::logging::write(LogLevel::Error, "overlay-window", &error.to_string());
                cx.quit();
            }
        });
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("SlimeVR dashboard: {error}");
        slimevr_gpui::logging::write(LogLevel::Error, "overlay", &error);
        std::process::exit(1);
    }
}
