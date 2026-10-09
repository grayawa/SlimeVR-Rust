mod alignment_ui;
mod assignment_ui;
mod checklist_ui;
mod firmware_ui;
mod mounting_ui;
mod onboarding_ui;
mod page_widgets_ui;
pub(super) mod preferences_ui;
mod proportions_ui;
mod serial_ui;
mod settings_layout_ui;
mod settings_ui;
mod tracker_telemetry_ui;
mod tracker_ui;
mod visualization_ui;
mod workflows;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
    switch::Switch,
};
use gpui_kit::{prelude::FluentBuilder as _, *};
use slimevr_gpui::{
    client::{Client, Connection, Snapshot},
    host::Backend,
    i18n::Localizer,
    navigation::{Navigation, Page, Section},
    protocol::{Command, ResetKind, Tracker},
};
use solarxr_protocol::datatypes::{BodyPart, TrackerStatus};
use std::{collections::HashMap, time::Duration};

pub struct SlimeView {
    client: Client,
    backend: Option<Backend>,
    snapshot: Snapshot,
    l10n: Localizer,
    navigation: Navigation,
    scrolls: HashMap<Page, ScrollHandle>,
    ui_error: Option<String>,
    backend_failed: bool,
    _refresh: Task<()>,
    draft: slimevr_gpui::settings::Draft,
    inputs: HashMap<String, (Entity<gpui_kit::component::input::InputState>, String)>,
    input_subscriptions: Vec<Subscription>,
    form_values: HashMap<String, String>,
    paths: slimevr_gpui::desktop::Paths,
    preferences: slimevr_gpui::desktop::Preferences,
    instance: slimevr_gpui::desktop::SingleInstance,
    return_page: Page,
    confirmation: Option<(String, serde_json::Value, String)>,
    autobone_auto: bool,
    unknown_devices: Vec<String>,
    highlight: Option<(u8, u8, std::time::Instant)>,
    assignment_role: Option<u8>,
    mounting_target: Option<mounting_ui::MountTarget>,
    mounting_wait: Option<(u64, u32, u8)>,
    pending_height: Option<proportions_ui::HeightAction>,
    serial_scroll: ScrollHandle,
    tracker_role_target: Option<slimevr_gpui::protocol::TrackerKey>,
    tracker_motion: slimevr_gpui::assignment::Motion,
    neck_warning_accepted: bool,
    notice: Option<String>,
    driver_notice: bool,
    global_mag: bool,
    checklist_closed: bool,
    checklist_completion: String,
    checklist_sequence: u64,
    checklist_session_ignored: std::collections::BTreeSet<u64>,
    checklist_open: std::collections::BTreeSet<u64>,
    saved_file: Option<std::path::PathBuf>,
    http: HashMap<String, serde_json::Value>,
    http_pending: std::collections::BTreeSet<String>,
    firmware_status: HashMap<String, serde_json::Value>,
    audio: std::sync::Arc<slimevr_gpui::sounds::Player>,
    sounds: slimevr_gpui::sounds::Sequencer,
    tray: Option<slimevr_gpui::tray::Tray>,
    exit_confirm: bool,
    allow_exit: bool,
    camera: slimevr_gpui::visualization::Camera,
    camera_drag: Option<Point<Pixels>>,
    presence: slimevr_gpui::presence::Presence,
    feed_profile: Option<(u16, u16, bool)>,
    setup_mode: Option<(u64, bool)>,
    focus: FocusHandle,
    key_capture: Option<u64>,
    temporary_tweaks: HashMap<String, bool>,
    autobone_valid: bool,
    autobone_result: Option<serde_json::Value>,
    next_build_poll: std::time::Instant,
    settings_sequence: u64,
    overlay: slimevr_gpui::overlay::Overlay,
    font_slider: Entity<gpui_kit::component::slider::SliderState>,
    proportion_pending: HashMap<String, String>,
    onboarding: slimevr_gpui::onboarding::Setup,
}

impl SlimeView {
    pub fn new(
        client: Client,
        backend: Option<Backend>,
        l10n: Localizer,
        paths: slimevr_gpui::desktop::Paths,
        preferences: slimevr_gpui::desktop::Preferences,
        instance: slimevr_gpui::desktop::SingleInstance,
        cx: &mut Context<Self>,
    ) -> Self {
        let tray = slimevr_gpui::tray::Tray::new([
            l10n.text("tray_menu-show"),
            l10n.text("tray_menu-hide"),
            l10n.text("tray_menu-quit"),
        ])
        .ok();
        let audio = std::sync::Arc::new(slimevr_gpui::sounds::Player::new());
        audio.configure_feedback(&preferences.value);
        let reset_audio = audio.clone();
        client.on_reset(move |session, reset| reset_audio.reset(session, reset));
        let snapshot = client.snapshot();
        let refresh = cx.spawn(async move |this, cx| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;
                if this
                    .update(cx, |this, cx| {
                        let action=this.tray.as_ref().and_then(|t|t.action());
                        let show=this.instance.requested() || matches!(action,Some(slimevr_gpui::tray::TrayAction::Show));
                        if show {for handle in cx.windows(){let _=handle.update(cx,|_,window,_|{slimevr_gpui::tray::visible(window,true);window.activate_window();});}}
                        if matches!(action,Some(slimevr_gpui::tray::TrayAction::Hide)) {for handle in cx.windows(){let _=handle.update(cx,|_,window,_|slimevr_gpui::tray::visible(window,false));}}
                        if matches!(action,Some(slimevr_gpui::tray::TrayAction::Quit)){this.request_exit(cx);}
                        let visible=cx.windows().into_iter().any(|h|h.update(cx,|_,window,_|slimevr_gpui::tray::is_visible(window)).unwrap_or(false));
                        if this.draft.save_expired(){this.ui_error=Some(this.text("native-save-timeout"));cx.notify();}
                        let fast=this.preferences.value["debug"]==true&&this.preferences.value["devSettings"]["fastDataFeed"]==true;
                        let profile=(if !visible{500}else if fast{11}else{100},if fast{11}else{25},visible&&(this.navigation.page==Page::Proportions || this.in_onboarding() && matches!(this.onboarding.step,slimevr_gpui::onboarding::Step::Mounting|slimevr_gpui::onboarding::Step::Height) || this.preferences.value["skeletonPreview"]!=false&&matches!(this.navigation.page,Page::Home|Page::VrMode)));
                        if this.feed_profile!=Some(profile){let _=this.client.configure_feed(profile.0,profile.1,profile.2);this.feed_profile=Some(profile);}
                        this.audio.configure_feedback(&this.preferences.value);
                        let snapshot = this.client.snapshot();
                        if snapshot.session != this.snapshot.session || snapshot.connection != Connection::Connected {
                            this.tracker_motion = Default::default();
                            this.highlight = None;
                        }
                        if this.tracker_motion.expire(std::time::Instant::now()) && visible { cx.notify(); }
                        if snapshot.connection==Connection::Connected {
                            let active=this.assignment_active();
                            let target=(snapshot.session,active);
                            if this.setup_mode!=Some(target)&& this.client.rpc("ChangeSettingsRequest",serde_json::json!({"tap_detection_settings":{"setup_mode":active}})).is_ok(){this.setup_mode=Some(target);}
                        } else {this.setup_mode=None;}
                        let count=snapshot.feed.as_ref().map(|f|f.trackers.iter().filter(|t|t.key.device!=0&&t.status==2).count()).unwrap_or(0);
                        this.presence.set(slimevr_gpui::presence::Settings{enable:this.preferences.value["discordPresence"]==true,activity:Some(format!("{count} trackers")),icon_text:Some(format!("SlimeVR GPUI {}",env!("CARGO_PKG_VERSION")))});
                        if snapshot.revision != this.snapshot.revision {
                            if let Some(state)=snapshot.rpc.get("SettingsResponse") && state.sequence!=this.settings_sequence {
                                this.settings_sequence=state.sequence;
                                if this.draft.confirm(snapshot.session,&state.value) {this.ui_error=None;}
                                this.draft.merge(&state.value);
                            }
                            if let Some(state)=snapshot.rpc.get("TrackingChecklistResponse") && state.sequence!=this.checklist_sequence {
                                this.checklist_sequence=state.sequence;
                                let derived=slimevr_gpui::checklist::derive(&state.value,&this.checklist_session_ignored);
                                if this.checklist_completion!=derived.completion {
                                    if derived.completion=="complete"{this.checklist_closed=true;}
                                    else if derived.completion=="incomplete"{this.checklist_closed=false;}
                                    this.checklist_completion=derived.completion.into();
                                }
                            }
                            if snapshot.session!=this.snapshot.session || snapshot.connection!=Connection::Connected {this.draft.saving=None;}
                            if snapshot.last_error.is_some() {this.draft.saving=None;}
                            if snapshot.connection!=Connection::Connected { this.onboarding.wifi_request=None;this.onboarding.dialog=None;this.autobone_auto=false;this.autobone_valid=false;this.temporary_tweaks.clear();this.sounds.disconnected();this.overlay=Default::default();this.assignment_role=None;this.mounting_target=None;this.mounting_wait=None;this.tracker_role_target=None;this.pending_height=None; }
                            if let Some(feed) = &snapshot.feed { this.tracker_motion.update(&feed.trackers, std::time::Instant::now()); }
                            this.snapshot = snapshot;
                            this.events(this.client.drain_events(),cx);
                            if visible{cx.notify();}
                        }
                        if !this.backend_failed
                            && let Some(error) = this.backend.as_mut().and_then(Backend::failure)
                        {
                            this.backend_failed = true;
                            this.ui_error = Some(error);
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        let font_slider = cx.new(|_| {
            gpui_kit::component::slider::SliderState::new()
                .min(10.)
                .max(15.)
                .step(1.)
                .default_value(preferences.value["textSize"].as_f64().unwrap_or(12.) as f32)
        });
        let font_subscription = cx.subscribe(
            &font_slider,
            |this, _, event: &gpui_kit::component::slider::SliderEvent, cx| {
                let gpui_kit::component::slider::SliderEvent::Release(value) = event else {
                    return;
                };
                this.preference(
                    "textSize",
                    serde_json::json!(value.start().round() as u32),
                    cx,
                );
            },
        );
        Self {
            client,
            backend,
            snapshot,
            l10n,
            navigation: {
                let mut nav = Navigation::default();
                if preferences.value["doneOnboarding"] != true {
                    nav.go(Page::Settings(Section::Onboarding));
                }
                nav
            },
            scrolls: HashMap::new(),
            ui_error: None,
            backend_failed: false,
            _refresh: refresh,
            draft: Default::default(),
            inputs: HashMap::new(),
            input_subscriptions: vec![font_subscription],
            form_values: HashMap::new(),
            paths,
            preferences,
            instance,
            return_page: Page::Home,
            confirmation: None,
            autobone_auto: false,
            unknown_devices: Vec::new(),
            highlight: None,
            assignment_role: None,
            mounting_target: None,
            mounting_wait: None,
            pending_height: None,
            serial_scroll: ScrollHandle::default(),
            tracker_role_target: None,
            tracker_motion: Default::default(),
            neck_warning_accepted: false,
            notice: None,
            driver_notice: false,
            global_mag: false,
            checklist_closed: true,
            checklist_completion: String::new(),
            checklist_sequence: 0,
            checklist_session_ignored: Default::default(),
            checklist_open: Default::default(),
            saved_file: None,
            http: HashMap::new(),
            http_pending: Default::default(),
            firmware_status: HashMap::new(),
            audio,
            sounds: Default::default(),
            tray,
            exit_confirm: false,
            allow_exit: false,
            camera: Default::default(),
            camera_drag: None,
            presence: slimevr_gpui::presence::Presence::start(),
            feed_profile: None,
            setup_mode: None,
            focus: cx.focus_handle(),
            key_capture: None,
            temporary_tweaks: HashMap::new(),
            autobone_valid: false,
            autobone_result: None,
            next_build_poll: std::time::Instant::now(),
            settings_sequence: 0,
            overlay: Default::default(),
            font_slider,
            proportion_pending: HashMap::new(),
            onboarding: Default::default(),
        }
    }
    fn text(&self, id: &str) -> String {
        self.l10n.text(id)
    }
    fn body_name(&self, body: u8) -> String {
        self.text(&format!(
            "body_part-{}",
            BodyPart(body).variant_name().unwrap_or("NONE")
        ))
    }
    fn send(&mut self, command: Command, cx: &mut Context<Self>) {
        self.ui_error = self.client.send(command).err();
        self.snapshot = self.client.snapshot();
        cx.notify();
    }
    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        slimevr_gpui::logging::write(
            slimevr_gpui::log_level::LogLevel::Debug,
            "navigation",
            &format!("{:?} -> {:?}", self.navigation.page, page),
        );
        if self.navigation.page == Page::Settings(Section::Serial)
            && page != self.navigation.page
            && self.snapshot.serial_device.is_some()
        {
            let _ = self.client.rpc("CloseSerialRequest", serde_json::json!({}));
        }
        if matches!(page, Page::Tracker(_) | Page::Checklist | Page::VrMode)
            && self.navigation.page != page
        {
            self.return_page = self.navigation.page;
        }
        if self.in_onboarding() && page != self.navigation.page {
            self.onboarding_stop_wifi();
        }
        if self.navigation.page == Page::Settings(Section::Serial)
            && page != Page::Settings(Section::Onboarding)
        {
            self.onboarding.serial_return = false;
        }
        if page == Page::Settings(Section::Onboarding) && !self.in_onboarding() {
            if self.onboarding.serial_return {
                self.onboarding.serial_return = false;
                if self.onboarding.step == slimevr_gpui::onboarding::Step::Connect {
                    self.onboarding_start_wifi(cx);
                }
            } else if !(matches!(self.navigation.page, Page::Tracker(_))
                && self.return_page == page)
            {
                self.onboarding = Default::default();
            }
        }
        self.assignment_role = None;
        self.mounting_target = None;
        self.mounting_wait = None;
        self.pending_height = None;
        self.tracker_role_target = None;
        if page == Page::Assignment && self.preferences.value["assignMode"].as_str().is_none() {
            let count = self
                .snapshot
                .feed
                .as_ref()
                .map(|f| {
                    f.trackers
                        .iter()
                        .filter(|t| {
                            !t.computed && t.is_imu && t.status != TrackerStatus::DISCONNECTED.0
                        })
                        .count()
                })
                .unwrap_or(0);
            self.preference(
                "assignMode",
                serde_json::json!(slimevr_gpui::assignment::preferred_mode(count)),
                cx,
            );
        }
        if page == Page::Proportions && self.navigation.page != page {
            self.camera = slimevr_gpui::visualization::Camera {
                yaw: 0.78,
                pitch: 0.45,
                zoom: 0.8,
                pan: [0., 25.],
            };
        }
        self.navigation.go(page);
        self.ui_error = None;
        // Opening VRChat warnings requests the current settings for display.
        if page == Page::VrchatWarnings && self.snapshot.connection == Connection::Connected {
            let _ = self.client.send(Command::ReadVrchat);
        }
        cx.notify();
    }
    fn sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut items = div().v_flex().gap_2().flex_1();
        for page in Page::SIDEBAR.into_iter().take(5) {
            items = items.child(self.nav_button(page, page.label(), cx));
        }
        div()
            .v_flex()
            .w(px(110.))
            .h_full()
            .flex_shrink_0()
            .p_2()
            .child(items)
            .child(self.nav_button(Page::Settings(Section::SteamVr), "navbar-settings", cx))
            .into_any_element()
    }
    fn nav_button(&self, page: Page, label: &str, cx: &mut Context<Self>) -> AnyElement {
        let selected = if page.in_settings() {
            self.navigation.page.in_settings()
        } else {
            self.navigation.page == page
        };
        let icon = match page {
            Page::Home => "Home",
            Page::Assignment => "Human",
            Page::Mounting => "Ski",
            Page::Proportions => "Ruler",
            Page::Connect => "Wifi",
            _ => "Gear",
        };
        let color = if selected {
            cx.theme().link
        } else {
            cx.theme().muted_foreground
        };
        Button::new(format!("nav-{page:?}"))
            .ghost()
            .selected(selected)
            .w_full()
            .h(px(80.))
            .rounded_lg()
            .when(selected, |b| b.bg(cx.theme().secondary))
            .text_color(color)
            .child(
                div()
                    .v_flex()
                    .w_full()
                    .items_center()
                    .gap_3()
                    .child(
                        svg()
                            .path(format!("slime/{icon}.svg"))
                            .size(px(30.))
                            .text_color(color),
                    )
                    .child(div().text_xs().child(self.text(label))),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.go(page, cx)))
            .into_any_element()
    }

    fn settings_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("settings-navigation")
            .v_flex()
            .gap_2()
            .overflow_y_scroll()
            .h_full()
            .w(px(200.))
            .flex_shrink_0()
            .p_4()
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("navbar-settings")),
            );
        for (group, sections) in Section::GROUPS {
            list = list.child(
                div()
                    .mt_2()
                    .text_size(px(16.))
                    .font_bold()
                    .child(self.text(group)),
            );
            for section in *sections {
                if *section == Section::Advanced {
                    list = list.child(self.section_button(
                        Page::VrchatWarnings,
                        "settings-sidebar-vrc_warnings",
                        cx,
                    ));
                }
                list =
                    list.child(self.section_button(Page::Settings(*section), section.label(), cx));
            }
        }
        list.into_any_element()
    }
    fn section_button(&self, page: Page, label: &str, cx: &mut Context<Self>) -> AnyElement {
        Button::new(format!("section-{page:?}"))
            .ghost()
            .small()
            .selected(self.navigation.page == page)
            .w_full()
            .h(px(34.))
            .rounded_lg()
            .when(self.navigation.page == page, |b| {
                b.bg(cx.theme().secondary).text_color(cx.theme().link)
            })
            .child(div().w_full().text_size(px(12.)).child(self.text(label)))
            .on_click(cx.listener(move |this, _, _, cx| this.go(page, cx)))
            .into_any_element()
    }

    fn resets(&self, cx: &mut Context<Self>) -> AnyElement {
        let connected =
            self.snapshot.connection == Connection::Connected && self.snapshot.feed.is_some();
        let busy = self.snapshot.pending.is_some();
        let mut row = div().h_flex().gap_3().flex_wrap();
        for kind in [ResetKind::Full, ResetKind::Yaw, ResetKind::Mounting] {
            let permitted = match kind {
                ResetKind::Full => true,
                ResetKind::Yaw => self.snapshot.feed.as_ref().is_some_and(|f| f.can_yaw),
                ResetKind::Mounting => self.snapshot.feed.as_ref().is_some_and(|f| f.can_mount),
            };
            row = row.child(
                Button::new(format!("reset-{kind:?}"))
                    .label(self.text(kind.label()))
                    .primary()
                    .disabled(!connected || busy || !permitted)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.send(Command::Reset(kind), cx)),
                    ),
            );
        }
        let paused = self.snapshot.paused.unwrap_or(false);
        row.child(
            Button::new("pause")
                .label(self.text(if paused {
                    "tracking-paused"
                } else {
                    "tracking-unpaused"
                }))
                .disabled(!connected || busy || self.snapshot.paused.is_none())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.send(Command::Pause(!paused), cx)),
                ),
        )
        .into_any_element()
    }
    fn tracker(&self, tracker: &Tracker, assigning: bool, cx: &mut Context<Self>) -> AnyElement {
        let status = TrackerStatus(tracker.status)
            .variant_name()
            .unwrap_or("NONE")
            .to_lowercase();
        let grid = !assigning && self.preferences.value["homeLayout"] != "table";
        if grid {
            return self.tracker_card(tracker, cx);
        }
        let highlight = self.tracker_highlight(tracker);
        let mut row = div()
            .flex()
            .when(grid, |d| d.flex_col().items_start().w(px(265.)))
            .when(!grid, |d| d.flex_row().items_center())
            .gap_4()
            .p_4()
            .rounded_lg()
            .bg(cx.theme().muted)
            .map(|row| self.motion_glow(row, highlight, 8., cx))
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .flex_1()
                    .min_w_0()
                    .child(div().font_bold().child(tracker.name.clone()))
                    .child(div().text_xs().text_color(rgb(0x92a9b8)).child(format!(
                        "{} · {}",
                        tracker.hardware,
                        self.text(&format!("tracker-status-{status}"))
                    ))),
            );
        if assigning && tracker.key.device != 0 {
            let key = tracker.key;
            let view = cx.entity().downgrade();
            let mode = self.preferences.value["assignMode"]
                .as_str()
                .unwrap_or("core");
            let labels: Vec<_> = BodyPart::ENUM_VALUES
                .iter()
                .filter(|b| !matches!(b.variant_name(), Some("LEFT_HIP" | "RIGHT_HIP")))
                .filter(|b| {
                    slimevr_gpui::assignment::allowed(mode, b.variant_name().unwrap_or("NONE"))
                })
                .map(|b| (b.0, self.body_name(b.0)))
                .collect();
            row = row.child(
                Button::new(format!("assign-{}-{}", key.device, key.sensor))
                    .label(self.body_name(tracker.body))
                    .disabled(
                        self.snapshot.pending.is_some()
                            || self.snapshot.connection != Connection::Connected,
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        for (body, label) in &labels {
                            let view = view.clone();
                            let body = *body;
                            menu = menu.item(PopupMenuItem::new(label.clone()).on_click(
                                move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.send(Command::Assign { key, body }, cx)
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            );
        } else {
            row = row.child(div().w(px(130.)).child(self.body_name(tracker.body)));
        }
        let key = tracker.key;
        row = row.child(
            Button::new(format!("details-{}-{}", key.device, key.sensor))
                .label(self.text("native-tracker-settings"))
                .small()
                .ghost()
                .on_click(cx.listener(move |this, _, _, cx| this.go(Page::Tracker(key), cx))),
        );
        if self.preferences.value["debug"] == true
            && self.preferences.value["devSettings"]["moreInfo"] == true
        {
            row = row.child(format!(
                "TPS {} · RSSI {}",
                tracker.tps.unwrap_or_default(),
                tracker
                    .rssi
                    .map(|v| format!("{v} dBm"))
                    .unwrap_or_else(|| "—".into())
            ));
        }
        row.child(self.tracker_battery(tracker, true, cx))
            .child(self.tracker_wifi(tracker, true, cx))
            .into_any_element()
    }
    fn warnings(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = div()
            .id("vrchat-warning-table")
            .v_flex()
            .gap_4()
            .overflow_x_scroll()
            .child(
                Button::new("warnings-back")
                    .label(self.text("native-back"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.navigation.back_from_warning();
                        cx.notify();
                    })),
            );
        match &self.snapshot.vrchat {
            None => page = page.child(self.text("native-waiting")),
            Some(vrchat) if !vrchat.supported => {
                page = page.child(self.text("native-vrchat-unsupported"))
            }
            Some(vrchat) => {
                page = page.child(
                    div()
                        .h_flex()
                        .gap_4()
                        .min_w(px(740.))
                        .p_3()
                        .font_bold()
                        .child(div().flex_1().child(self.text("native-vrchat-setting")))
                        .child(div().w(px(165.)).child(self.text("native-current")))
                        .child(div().w(px(165.)).child(self.text("native-recommended")))
                        .child(div().w(px(120.))),
                );
                for row in &vrchat.rows {
                    let key = row.key.clone();
                    let value = |s: &str| {
                        s.split(" / ")
                            .map(|part| {
                                if part.starts_with("vrc_config-") {
                                    self.text(part)
                                } else {
                                    part.to_owned()
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" / ")
                    };
                    page = page.child(
                        div()
                            .h_flex()
                            .gap_4()
                            .items_center()
                            .min_w(px(740.))
                            .p_3()
                            .bg(cx.theme().popover)
                            .rounded_lg()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .font_bold()
                                    .text_color(if row.valid {
                                        rgb(0x50e897)
                                    } else {
                                        rgb(0xffe135)
                                    })
                                    .child(self.text(&row.label)),
                            )
                            .child(div().w(px(165.)).child(value(&row.current)))
                            .child(div().w(px(165.)).child(value(&row.recommended)))
                            .child(
                                Button::new(format!("mute-{key}"))
                                    .w(px(120.))
                                    .small()
                                    .label(self.text(if row.muted {
                                        "vrc_config-unmute-btn"
                                    } else {
                                        "vrc_config-mute-btn"
                                    }))
                                    .disabled(
                                        self.snapshot.connection != Connection::Connected
                                            || self.snapshot.pending.is_some(),
                                    )
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.send(Command::MuteVrchat(key.clone()), cx)
                                    })),
                            ),
                    );
                }
            }
        }
        page.into_any_element()
    }
}

impl Render for SlimeView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            entity
                .update(cx, |this, cx| {
                    if this.allow_exit {
                        return true;
                    }
                    this.request_close(window, cx);
                    false
                })
                .unwrap_or(true)
        });
        let current = self.navigation.page;
        let onboarding = self.in_onboarding();
        let scroll = self.scrolls.entry(current).or_default().clone();
        let mut content = div()
            .id("page-content")
            .v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_4()
            .gap_4()
            .rounded_lg()
            .bg(cx.theme().muted)
            // Settings cards own their background and corners. The scroll
            // viewport must leave the page background visible between them.
            .when(current.in_settings(), |d| d.rounded_none().bg(cx.theme().background))
            .when(onboarding,|d|d.p_0().gap_0())
            .when(matches!(current,Page::Home|Page::Tracker(_)) || matches!(current,Page::Settings(section) if slimevr_gpui::settings_layout::pane(section).is_some() || section==Section::Home || section==Section::Checklist), |d| d.p_0().bg(cx.theme().background))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .when(!onboarding && !matches!(current, Page::Home | Page::Assignment | Page::Mounting | Page::Proportions | Page::Tracker(_)) && !matches!(current,Page::Settings(section) if slimevr_gpui::settings_layout::pane(section).is_some() || section==Section::Checklist || section==Section::Home), |d| {
                d.child(
                    div()
                        .text_2xl()
                        .font_bold()
                        .child(self.text(current.label())),
                )
            });
        if let Some(error) = self.ui_error.as_ref().or(self.snapshot.last_error.as_ref()) {
            content = content.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0x3d2228))
                    .child(error.clone()),
            );
        }
        if current != Page::Home
            && let Some(reset) = &self.snapshot.reset
            && !reset.done
        {
            let progress = if reset.done {
                self.text("native-reset-finished")
            } else {
                format!(
                    "{:.1}s",
                    (reset.duration_ms - reset.progress_ms).max(0) as f32 / 1000.0
                )
            };
            content = content.child(div().text_color(rgb(0xb994d8)).child(progress));
        }
        content = content.child(match current {
            Page::Home => self.home_aligned(window, cx),
            Page::Assignment => self.assignment_aligned(window, cx),
            Page::Mounting => self.mounting_aligned(window, cx),
            Page::Connect => self.connect_aligned(window, cx),
            Page::Proportions => self.proportions_aligned(window, cx),
            Page::Tracker(key) => self.tracker_details(key, window, cx),
            Page::Checklist => self.checklist(cx),
            Page::VrMode => self.vr_mode(cx),
            Page::Settings(
                Section::Appearance
                | Section::Behavior
                | Section::Notifications
                | Section::Home
                | Section::Checklist,
            ) => {
                if let Page::Settings(section) = current {
                    self.interface_page(section, window, cx)
                } else {
                    unreachable!()
                }
            }
            Page::Settings(Section::Onboarding) => self.onboarding(window, cx),
            Page::StayAlignedSetup => div()
                .v_flex()
                .gap_4()
                .child(
                    Button::new("aligned-back")
                        .label(self.text("native-back"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go(Page::Settings(Section::StayAligned), cx)
                        })),
                )
                .child(self.relaxed_poses(cx))
                .into_any_element(),
            Page::Settings(Section::Advanced) => self.advanced_page(window, cx),
            Page::Settings(Section::Firmware) => self.firmware_page(window, cx),
            Page::Settings(Section::Serial) => {
                let serial = self.serial(window, cx);
                div()
                    .v_flex()
                    .gap_3()
                    .when(self.onboarding.serial_return, |d| {
                        d.child(
                            Button::new("setup-serial-return")
                                .label(self.text("onboarding-previous_step"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.go(Page::Settings(Section::Onboarding), cx)
                                })),
                        )
                    })
                    .child(serial)
                    .into_any_element()
            }
            Page::Settings(Section::OscVmc) => div()
                .v_flex()
                .gap_4()
                .child(self.settings_form(Section::OscVmc, window, cx))
                .into_any_element(),
            Page::Settings(Section::StayAligned) => div()
                .v_flex()
                .gap_2()
                .child(self.settings_form(Section::StayAligned, window, cx))
                .child(self.settings_pane(
                    "native-advanced-settings",
                    "Wrench",
                    self.relaxed_poses(cx),
                    cx,
                ))
                .into_any_element(),
            Page::Settings(section) => self.settings_form(section, window, cx),
            Page::VrchatWarnings => self.warnings(cx),
        });
        for mac in self.unknown_devices.clone().into_iter().filter(|_| {
            self.preferences.value["watchNewDevices"] != false
                || matches!(self.navigation.page, Page::Connect | Page::Assignment)
                || self.in_onboarding()
        }) {
            content = content.child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(format!("{} {mac}", self.text("native-unknown-device")))
                    .child(
                        Button::new(format!("accept-{mac}"))
                            .label(self.text("native-accept-device"))
                            .disabled(self.snapshot.connection != Connection::Connected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.rpc(
                                    "AddUnknownDeviceRequest",
                                    serde_json::json!({"mac_address":mac}),
                                    cx,
                                );
                                if this.ui_error.is_none() {
                                    this.unknown_devices.retain(|m| *m != mac);
                                }
                            })),
                    ),
            );
        }
        if let Some(notice) = self.notice.clone() {
            content = content.child(
                div().h_flex().gap_3().p_3().child(notice).child(
                    Button::new("notice-close")
                        .label(self.text("native-close"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.notice = None;
                            cx.notify();
                        })),
                ),
            );
        }
        let status = match self.snapshot.connection {
            Connection::Connecting => "native-connecting",
            Connection::Connected => "native-connected",
            Connection::Disconnected => "native-disconnected",
        };
        div()
            .relative()
            .v_flex()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if this.exit_confirm {
                    if event.keystroke.key == "escape" {
                        this.exit_confirm = false;
                        cx.notify();
                    }
                    cx.stop_propagation();
                    return;
                }
                if event.keystroke.key == "escape" && this.confirmation.is_some() {
                    this.confirmation = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" && this.tracker_role_target.is_some() {
                    this.tracker_role_target = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" && this.pending_height.is_some() {
                    this.pending_height = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" && this.mounting_target.is_some() {
                    this.mounting_target = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" && this.assignment_role.is_some() {
                    this.assignment_role = None;
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if event.keystroke.key == "escape" && this.in_onboarding() {
                    this.onboarding.dialog = if this.onboarding.dialog.is_some() {
                        None
                    } else {
                        Some(slimevr_gpui::onboarding::Dialog::Skip)
                    };
                    cx.stop_propagation();
                    cx.notify();
                    return;
                }
                if let Some(id) = this.key_capture {
                    if event.keystroke.key == "escape" {
                        this.key_capture = None;
                        cx.notify();
                        return;
                    }
                    let mut modifiers = Vec::new();
                    if event.keystroke.modifiers.control {
                        modifiers.push("CTRL");
                    }
                    if event.keystroke.modifiers.alt {
                        modifiers.push("ALT");
                    }
                    if event.keystroke.modifiers.shift {
                        modifiers.push("SHIFT");
                    }
                    if event.keystroke.modifiers.platform {
                        modifiers.push("WIN");
                    }
                    let key = event.keystroke.key.to_ascii_uppercase();
                    if matches!(
                        key.as_str(),
                        "CTRL" | "ALT" | "SHIFT" | "CONTROL" | "WIN" | "SUPER"
                    ) {
                        return;
                    }
                    modifiers.push(&key);
                    this.form_values
                        .insert(format!("keybind-{id}"), modifiers.join("+"));
                    this.key_capture = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_size(px(
                self.preferences.value["textSize"].as_f64().unwrap_or(12.0) as f32,
            ))
            .font_family(slimevr_gpui::desktop::font_family(&self.preferences.value))
            .child(self.topbar(status, cx))
            .when(onboarding, |d| d.child(self.onboarding_progress(cx)))
            .child(
                div()
                    .h_flex()
                    .flex_1()
                    .min_h_0()
                    .when(!onboarding, |d| d.child(self.sidebar(cx)))
                    .when(current.in_settings() && !onboarding, |row| {
                        row.child(self.settings_sidebar(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .when(current.in_settings() && !onboarding, |d| d.pl_2())
                            .pr_2()
                            .pb_2()
                            .child(content),
                    ),
            )
            .when(onboarding, |root| root.child(self.onboarding_escape(cx)))
            .when(self.onboarding.dialog.is_some(), |root| {
                root.child(self.onboarding_dialog(window, cx))
            })
            .when(self.confirmation.is_some(), |root| {
                root.child(self.confirmation_dialog(window, cx))
            })
            .when_some(self.tracker_role_target, |root, key| {
                root.child(self.tracker_role_selection(key, window, cx))
            })
            .when_some(self.pending_height, |root, height| {
                root.child(self.height_warning(height, window, cx))
            })
            .when_some(self.mounting_target, |root, target| {
                root.child(self.mounting_selection(target, window, cx))
            })
            .when_some(self.assignment_role, |root, body| {
                root.child(self.assignment_selection(body, window, cx))
            })
            .when(self.exit_confirm, |root| {
                root.child(self.exit_dialog(window, cx))
            })
    }
}
