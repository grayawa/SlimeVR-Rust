use super::*;
use gpui_kit::component::button::ButtonCustomVariant;
use serde_json::json;
use slimevr_gpui::onboarding::{Dialog, Setup, Step, TrackerSet};

impl SlimeView {
    pub(super) fn in_onboarding(&self) -> bool {
        self.navigation.page == Page::Settings(Section::Onboarding)
    }
    pub(super) fn assignment_active(&self) -> bool {
        self.navigation.page == Page::Assignment
            || self.in_onboarding() && self.onboarding.step == Step::Assignment
    }
    pub(super) fn onboarding_to(&mut self, step: Step, cx: &mut Context<Self>) {
        if self.onboarding.step == Step::Connect && step != Step::Connect {
            self.onboarding_stop_wifi();
        }
        self.assignment_role = None;
        self.mounting_target = None;
        self.mounting_wait = None;
        self.pending_height = None;
        self.onboarding.step = step;
        self.onboarding.dialog = None;
        if step == Step::Mounting {
            self.form_values
                .insert("mounting-method".into(), "choose".into());
            self.form_values.insert("mounting-step".into(), "0".into());
        }
        if step == Step::Height {
            self.form_values
                .insert("proportions-view".into(), "preview".into());
            self.camera = Default::default();
        }
        if step == Step::Assignment && self.preferences.value["assignMode"].as_str().is_none() {
            let count = self
                .snapshot
                .feed
                .as_ref()
                .map(|f| {
                    f.trackers
                        .iter()
                        .filter(|t| !t.computed && t.is_imu && t.status == 2)
                        .count()
                })
                .unwrap_or(0);
            self.preference(
                "assignMode",
                json!(slimevr_gpui::assignment::preferred_mode(count)),
                cx,
            );
        }
        self.ui_error = None;
        if let Some(scroll) = self.scrolls.get(&Page::Settings(Section::Onboarding)) {
            scroll.set_offset(point(px(0.), px(0.)));
        }
        cx.notify();
    }
    pub(super) fn onboarding_start_wifi(&mut self, cx: &mut Context<Self>) {
        self.onboarding.wifi_status = 0;
        let after = self.client.snapshot().event_sequence;
        match self.client.rpc("StartWifiProvisioningRequest",json!({"ssid":self.form_value("wifi-ssid",""),"password":self.form_value("wifi-password","")})) {
            Ok(tx)=>{self.onboarding.wifi_request=Some((self.snapshot.session,tx,after));self.onboarding_to(Step::Connect,cx);}
            Err(e)=>{self.ui_error=Some(e);cx.notify();}
        }
    }
    pub(super) fn onboarding_stop_wifi(&mut self) {
        if self.onboarding.wifi_request.take().is_some() {
            let _ = self.client.rpc("StopWifiProvisioningRequest", json!({}));
        }
    }
    fn onboarding_finish(&mut self, skip: bool, cx: &mut Context<Self>) {
        self.onboarding_stop_wifi();
        if !skip {
            let settings = self.read("SettingsResponse");
            let mut requests = vec![(
                "ChangeSettingsRequest".into(),
                self.onboarding.settings(&settings),
            )];
            if self.onboarding.mocap {
                let bone = slimevr_gpui::rpc_generated::enum_choices("SkeletonBone")
                    .iter()
                    .find(|(n, _)| *n == "HAND_Z")
                    .map(|(_, id)| *id)
                    .unwrap_or(19);
                requests.push((
                    "ChangeSkeletonConfigRequest".into(),
                    json!({"bone":bone,"value":0.0}),
                ));
                requests.push(("SkeletonConfigRequest".into(), json!({})));
            }
            requests.push(("SettingsRequest".into(), json!({})));
            self.batch(requests, cx);
            if self.ui_error.is_some() {
                return;
            }
        }
        self.preference("doneOnboarding", json!(true), cx);
        self.onboarding = Setup::default();
        self.go(Page::Home, cx);
    }
    pub(super) fn onboarding_nav(
        &self,
        back: Step,
        next: Step,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h_flex()
            .w_full()
            .gap_3()
            .justify_between()
            .child(
                Button::new("setup-prev")
                    .label(self.text("onboarding-previous_step"))
                    .on_click(cx.listener(move |this, _, _, cx| this.onboarding_to(back, cx))),
            )
            .child(
                Button::new("setup-next")
                    .primary()
                    .label(self.text("onboarding-continue"))
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.onboarding_to(next, cx))),
            )
            .into_any_element()
    }
    fn setup_title(
        &self,
        title: &str,
        desc: Option<&str>,
        width: f32,
        window: &Window,
    ) -> AnyElement {
        div()
            .v_flex()
            .gap_2()
            .child(div().text_2xl().font_bold().child(self.text(title)))
            .when_some(desc, |d, id| d.child(self.paragraph(id, width, window)))
            .into_any_element()
    }
    fn setup_card(
        &self,
        id: &str,
        label: &str,
        icon: AnyElement,
        active: bool,
        cx: &Context<Self>,
    ) -> Button {
        Button::new(id.to_owned())
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(cx.theme().popover)
                    .hover(cx.theme().accent)
                    .active(cx.theme().accent),
            )
            .w_full()
            .h_auto()
            .p_4()
            .rounded_lg()
            .bg(cx.theme().popover)
            .border_2()
            .border_color(if active {
                cx.theme().primary
            } else {
                cx.theme().popover
            })
            .child(
                div()
                    .v_flex()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .w_full()
                    .child(icon)
                    .child(div().font_bold().text_sm().child(self.text(label))),
            )
    }
    fn setup_icon(&self, name: &str, cx: &Context<Self>) -> AnyElement {
        svg()
            .path(format!("slime/{name}.svg"))
            .size(px(50.))
            .text_color(cx.theme().muted_foreground)
            .into_any_element()
    }
    fn setup_types(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let col = ((width - 40.).min(560.) - 16.) / 2.;
        let mut group =
            div()
                .v_flex()
                .w(px((width - 40.).min(560.)))
                .gap_8()
                .child(self.setup_title(
                    "onboarding-quiz-slimeset-title",
                    Some("onboarding-quiz-slimeset-description"),
                    width.min(560.),
                    window,
                ));
        for (heading, sets) in [
            (
                "official-sets",
                [TrackerSet::Regular, TrackerSet::Butterfly],
            ),
            ("thirdparty-sets", [TrackerSet::Wifi, TrackerSet::Dongle]),
        ] {
            let mut row = div().h_flex().gap_4();
            for set in sets {
                let (label, icon) = match set {
                    TrackerSet::Regular => (
                        "regular",
                        crate::ui_assets::image("slime/trackers/v1_2_slime.webp", cx)
                            .w(px((col - 32.).min(240.)))
                            .h(px((col - 32.).min(175.)))
                            .object_fit(ObjectFit::Contain)
                            .into_any_element(),
                    ),
                    TrackerSet::Butterfly => (
                        "butterfly",
                        crate::ui_assets::image("slime/trackers/butterfly_slime.webp", cx)
                            .w(px((col - 32.).min(240.)))
                            .h(px((col - 32.).min(175.)))
                            .object_fit(ObjectFit::Contain)
                            .into_any_element(),
                    ),
                    TrackerSet::Wifi => ("wifi", self.setup_icon("WifiNetwork", cx)),
                    TrackerSet::Dongle => ("dongle", self.setup_icon("USB", cx)),
                };
                row = row.child(
                    div().w(px(col)).child(
                        self.setup_card(
                            &format!("setup-type-{label}"),
                            &format!("onboarding-quiz-slimeset-answer-{label}"),
                            icon,
                            self.onboarding.tracker_set == Some(set),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.onboarding.tracker_set = Some(set);
                            this.onboarding_to(set.connection(), cx);
                        })),
                    ),
                );
            }
            group = group.child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_bold()
                            .child(self.text(&format!("onboarding-quiz-slimeset-{heading}"))),
                    )
                    .child(row),
            );
        }
        group.into_any_element()
    }
    fn setup_language(&self, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let names = slimevr_gpui::locales::LOCALES
            .iter()
            .map(|locale| slimevr_gpui::locales::name(locale).to_owned())
            .collect::<Vec<_>>();
        Button::new("setup-language")
            .label(slimevr_gpui::locales::name(
                self.preferences.value["lang"].as_str().unwrap_or("en"),
            ))
            .dropdown_menu(move |mut menu, _, _| {
                for (locale, name) in slimevr_gpui::locales::LOCALES.iter().zip(names.iter()) {
                    let view = view.clone();
                    menu = menu.item(PopupMenuItem::new(name.clone()).on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if let Ok(mut l10n) = Localizer::new(locale) {
                                if let Ok(s) =
                                    std::fs::read_to_string(this.paths.root.join("override.ftl"))
                                {
                                    let _ = l10n.add_override(&s);
                                }
                                this.l10n = l10n;
                                if let Some(tray) = &this.tray {
                                    tray.labels([
                                        this.text("tray_menu-show"),
                                        this.text("tray_menu-hide"),
                                        this.text("tray_menu-quit"),
                                    ]);
                                }
                                gpui_kit::component::set_locale(if *locale == "zh-Hans" {
                                    "zh-CN"
                                } else {
                                    locale
                                });
                                this.preference("lang", json!(locale), cx);
                            }
                        });
                    }));
                }
                menu
            })
            .into_any_element()
    }
    fn setup_welcome(&self, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let illustration = (width * 0.35).min(800.);
        div()
            .relative()
            .w_full()
            .h(px(height))
            .overflow_hidden()
            .child(
                canvas(
                    |_, _, _| (),
                    |bounds, _, window, cx| {
                        let w = f32::from(bounds.size.width);
                        let h = f32::from(bounds.size.height);
                        let x = f32::from(bounds.origin.x);
                        let y = f32::from(bounds.origin.y);
                        let mut path = PathBuilder::fill();
                        path.move_to(point(px(x), px(y + h)));
                        path.line_to(point(px(x), px(y + h - 35.)));
                        path.cubic_bezier_to(
                            point(px(x + w), px(y + h - 35.)),
                            point(px(x + w * 0.3), px(y + h - 180.)),
                            point(px(x + w * 0.7), px(y + h - 180.)),
                        );
                        path.line_to(point(px(x + w), px(y + h)));
                        path.close();
                        if let Ok(path) = path.build() {
                            window.paint_path(path, cx.theme().secondary);
                        }
                    },
                )
                .absolute()
                .inset_0(),
            )
            .child(
                crate::ui_assets::image("slime/slime-girl.webp", cx)
                    .absolute()
                    .left(px(width * 0.09))
                    .bottom(px(height * 0.01))
                    .w(px(illustration))
                    .h(px((height * 0.52).min(520.)))
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                crate::ui_assets::image("slime/slimes.webp", cx)
                    .absolute()
                    .right(px(width * 0.09))
                    .bottom(px(height * 0.01))
                    .w(px(illustration))
                    .h(px((height * 0.4).min(400.)))
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .absolute()
                    .top(px((height * 0.32).max(65.)))
                    .left(px(0.))
                    .w_full()
                    .v_flex()
                    .items_center()
                    .gap_5()
                    .child(
                        svg()
                            .path("slime/SlimeVR.svg")
                            .w(px(95.))
                            .h(px(60.))
                            .text_color(cx.theme().primary),
                    )
                    .child(
                        div()
                            .text_size(px(if width < 700. { 28. } else { 36. }))
                            .font_bold()
                            .child(self.text("onboarding-home")),
                    )
                    .child(
                        Button::new("setup-start")
                            .primary()
                            .large()
                            .w(px(168.))
                            .h(px(56.))
                            .child(div().text_lg().child(self.text("onboarding-home-start")))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.onboarding_to(Step::TrackerType, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .absolute()
                    .bottom_4()
                    .right_4()
                    .child(self.setup_language(cx)),
            )
            .into_any_element()
    }
    fn setup_wifi(
        &mut self,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let w = (width - 40.).min(410.);
        let ssid = self.local_input("wifi-ssid", "", false, window, cx);
        let password = self.local_input("wifi-password", "", true, window, cx);
        let pw = self.form_value("wifi-password", "");
        let valid =
            !self.form_value("wifi-ssid", "").trim().is_empty() && (pw.is_empty() || pw.len() >= 8);
        div()
            .v_flex()
            .w(px(w))
            .gap_4()
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .child(self.setup_icon("Wifi", cx))
                    .child(
                        div()
                            .font_bold()
                            .text_2xl()
                            .child(self.text("onboarding-wifi_creds-v2")),
                    ),
            )
            .child(self.paragraph("onboarding-wifi_creds-description-v2", w, window))
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .p_5()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .child(self.text("onboarding-wifi_creds-ssid"))
                    .child(ssid)
                    .child(self.text("onboarding-wifi_creds-password"))
                    .child(password)
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .gap_3()
                            .child(
                                Button::new("setup-wifi-back")
                                    .label(self.text("onboarding-wifi_creds-back-v2"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.onboarding_to(Step::TrackerType, cx)
                                    })),
                            )
                            .child(
                                Button::new("setup-wifi-submit")
                                    .primary()
                                    .label(self.text("onboarding-wifi_creds-submit"))
                                    .disabled(
                                        !valid || self.snapshot.connection != Connection::Connected,
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.onboarding_start_wifi(cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn setup_more(&self, cx: &mut Context<Self>) -> Button {
        Button::new("setup-connected-next")
            .primary()
            .label(self.text("onboarding-connect_tracker-next"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.onboarding.dialog = Some(Dialog::MoreSets);
                cx.notify();
            }))
    }
    fn setup_dongle(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = (width - 40.).min(600.);
        div()
            .v_flex()
            .w(px(w))
            .gap_4()
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_2()
                    .child(self.setup_icon("USB", cx))
                    .child(
                        div()
                            .font_bold()
                            .text_2xl()
                            .child(self.text("onboarding-wifi_creds-dongle-title")),
                    ),
            )
            .child(self.paragraph("onboarding-wifi_creds-dongle-description", w, window))
            .child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().warning.opacity(0.15))
                    .child(self.paragraph("onboarding-wifi_creds-dongle-wip", w - 32., window)),
            )
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .child(
                        Button::new("setup-dongle-back")
                            .label(self.text("onboarding-quiz_back"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.onboarding_to(Step::TrackerType, cx)
                            })),
                    )
                    .child(self.setup_more(cx)),
            )
            .into_any_element()
    }
    fn setup_connect(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let wide = width >= 784.;
        let col = if wide {
            if width >= 1284. { 368. } else { 318. }
        } else {
            width - 40.
        };
        let status = self.onboarding.wifi_status;
        let states = [
            "none",
            "serial_init",
            "provisioning",
            "connecting",
            "connection_error",
            "looking_for_server",
            "could_not_find_server",
            "done",
            "obtaining_mac_address",
            "no_serial_log",
            "no_serial_device_found",
        ];
        let progress =
            [0., 0.2, 0.4, 0.6, 0.6, 0.8, 0.8, 1., 0.3, 0.3, 0.2][status.min(10) as usize];
        let color = if matches!(status, 4 | 6 | 9 | 10) {
            cx.theme().danger
        } else if status == 7 {
            cx.theme().success
        } else {
            cx.theme().primary
        };
        let trackers: Vec<_> = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| {
                f.trackers
                    .iter()
                    .filter(|t| !t.computed && t.is_imu && t.status == 2)
                    .collect()
            })
            .unwrap_or_default();
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("amount", trackers.len() as i64);
        let left = div()
            .v_flex()
            .gap_4()
            .w(px(col))
            .flex_shrink_0()
            .child(self.setup_title(
                "onboarding-connect_tracker-title",
                Some("onboarding-connect_tracker-description-p0-v1"),
                col,
                window,
            ))
            .child(self.paragraph("onboarding-connect_tracker-description-p1-v1", col, window))
            .child(
                Button::new("setup-serial")
                    .ghost()
                    .label(self.text("onboarding-connect_tracker-issue-serial"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.onboarding.serial_return = true;
                        this.go(Page::Settings(Section::Serial), cx);
                    })),
            )
            .child(self.tip(
                if trackers.is_empty() {
                    "tips-turn_on_tracker"
                } else {
                    "tips-find_tracker"
                },
                col,
                window,
                cx,
            ))
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .child(
                        div()
                            .font_bold()
                            .child(self.text("onboarding-connect_tracker-usb")),
                    )
                    .child(self.text(&format!(
                        "onboarding-connect_tracker-connection_status-{}",
                        states[status.min(10) as usize]
                    )))
                    .child(
                        div()
                            .w_full()
                            .h(px(14.))
                            .rounded_lg()
                            .bg(cx.theme().border)
                            .child(div().w(relative(progress)).h_full().rounded_lg().bg(color)),
                    ),
            )
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .child(
                        Button::new("setup-connect-back")
                            .label(self.text("onboarding-previous_step"))
                            .on_click(
                                cx.listener(|this, _, _, cx| this.onboarding_to(Step::Wifi, cx)),
                            ),
                    )
                    .child(self.setup_more(cx)),
            );
        let mut cards = div().v_flex().flex_1().gap_3().child(
            div().font_bold().child(
                self.l10n
                    .format("onboarding-connect_tracker-connected_trackers", &args),
            ),
        );
        for pair in trackers.chunks(if width >= 1284. { 2 } else { 1 }) {
            let mut row = div().h_flex().gap_2();
            for tracker in pair {
                row = row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(self.tracker_card_button(tracker, cx).w_full().h(px(70.))),
                );
            }
            cards = cards.child(row);
        }
        if trackers.is_empty() {
            cards = cards.child(div().h(px(64.)).rounded_lg().bg(cx.theme().muted));
        }
        div()
            .flex()
            .gap_4()
            .p_4()
            .when(wide, |d| d.flex_row())
            .when(!wide, |d| d.flex_col())
            .child(left)
            .child(cards)
            .into_any_element()
    }
    fn setup_usage(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = (width - 40.).min(560.);
        let mut row = div().h_flex().gap_4();
        for (mocap, icon, label) in [(false, "Headset", "VRC"), (true, "Human", "mocap_vtubing")] {
            row = row.child(
                div().flex_1().child(
                    self.setup_card(
                        &format!("setup-usage-{label}"),
                        &format!("onboarding-quiz-usage-answer-{label}"),
                        self.setup_icon(icon, cx),
                        self.onboarding.usage_selected && self.onboarding.mocap == mocap,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.onboarding.mocap = mocap;
                        this.onboarding.usage_selected = true;
                        this.onboarding.standalone = false;
                        this.onboarding_to(if mocap { Step::Mocap } else { Step::Runtime }, cx);
                    })),
                ),
            );
        }
        div()
            .v_flex()
            .w(px(w))
            .gap_8()
            .child(self.setup_title(
                "onboarding-quiz-usage-title",
                Some("onboarding-quiz-usage-description"),
                w,
                window,
            ))
            .child(row)
            .child(
                div().h_flex().child(
                    Button::new("setup-usage-back")
                        .label(self.text("onboarding-quiz_back"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.onboarding_to(Step::TrackerType, cx)),
                        ),
                ),
            )
            .into_any_element()
    }
    fn setup_runtime(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = (width - 40.).min(560.);
        let mut row = div().h_flex().gap_4();
        let enabled = self.snapshot.connection == Connection::Connected
            && self.snapshot.rpc.contains_key("SettingsResponse");
        for (standalone, icon, label) in
            [(false, "Steam", "steamvr"), (true, "Headset", "standalone")]
        {
            row = row.child(
                div().flex_1().child(
                    self.setup_card(
                        &format!("setup-runtime-{label}"),
                        &format!("onboarding-quiz-runtime-answer-{label}"),
                        self.setup_icon(icon, cx),
                        self.onboarding.standalone == standalone,
                        cx,
                    )
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.onboarding.standalone = standalone;
                        this.onboarding_finish(false, cx);
                    })),
                ),
            );
        }
        div()
            .v_flex()
            .w(px(w))
            .gap_8()
            .child(self.setup_title("onboarding-quiz-runtime-title", None, w, window))
            .child(row)
            .child(
                div().h_flex().child(
                    Button::new("setup-runtime-back")
                        .label(self.text("onboarding-quiz_back"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.onboarding_to(Step::Usage, cx)),
                        ),
                ),
            )
            .into_any_element()
    }
    fn setup_mocap(&self, width: f32, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let w = (width - 40.).min(448.);
        let mut body = div().v_flex().w(px(w)).gap_8().child(self.setup_title(
            "onboarding-quiz-mocap_preferences-title",
            Some("onboarding-quiz-mocap_preferences-desc"),
            w,
            window,
        ));
        let mut head = div().h_flex().gap_4();
        for (value, label) in [(true, "yes"), (false, "no")] {
            let icon = div()
                .relative()
                .child(self.setup_icon("Headset", cx))
                .when(!value, |d| {
                    d.child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .text_color(cx.theme().danger)
                            .text_2xl()
                            .child("╱"),
                    )
                })
                .into_any_element();
            head = head.child(
                div().flex_1().child(
                    self.setup_card(
                        &format!("setup-head-{label}"),
                        &format!("onboarding-quiz-mocap_preferences-head_tracker-{label}"),
                        icon,
                        self.onboarding.head_tracker == value,
                        cx,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.onboarding.head_tracker = value;
                        if !value {
                            this.onboarding.forehead = None;
                            this.onboarding.standing = None;
                        }
                        cx.notify();
                    })),
                ),
            );
        }
        body = body
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        div().font_bold().child(
                            self.text("onboarding-quiz-mocap_preferences-head_tracker-title"),
                        ),
                    )
                    .child(head),
            )
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_bold()
                            .child(self.text("onboarding-quiz-mocap_preferences-vrm_model-title")),
                    )
                    .child(self.paragraph(
                        "onboarding-quiz-mocap_preferences-vrm_model-desc",
                        w,
                        window,
                    ))
                    .child(
                        div()
                            .p_8()
                            .border_1()
                            .border_dashed()
                            .border_color(cx.theme().border)
                            .rounded_lg()
                            .child(self.avatar_controls(cx)),
                    ),
            );
        if self.onboarding.head_tracker {
            let mut row = div().h_flex().gap_4();
            for (value, label, icon) in [(false, "sitting", "Sitting"), (true, "standing", "Human")]
            {
                row = row.child(
                    div().flex_1().child(
                        self.setup_card(
                            &format!("setup-space-{label}"),
                            &format!("onboarding-quiz-mocap_preferences-playspace-{label}"),
                            self.setup_icon(icon, cx),
                            self.onboarding.standing == Some(value),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.onboarding.standing = Some(value);
                            cx.notify();
                        })),
                    ),
                );
            }
            body = body.child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(
                        div()
                            .font_bold()
                            .child(self.text("onboarding-quiz-mocap_preferences-playspace-title")),
                    )
                    .child(self.paragraph(
                        "onboarding-quiz-mocap_preferences-playspace-desc",
                        w,
                        window,
                    ))
                    .child(row),
            );
            let mut row = div().h_flex().gap_4();
            for (value, label) in [(true, "forehead"), (false, "face")] {
                let icon =
                    crate::ui_assets::image(&format!("slime/quiz/quiz_mocap-pos_{label}.webp"), cx)
                        .w(px((w / 2. - 40.).min(176.)))
                        .h(px(176.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element();
                row = row.child(
                    div().flex_1().child(
                        self.setup_card(
                            &format!("setup-pos-{label}"),
                            &format!(
                                "onboarding-quiz-mocap_preferences-head_tracker_location-{label}"
                            ),
                            icon,
                            self.onboarding.forehead == Some(value),
                            cx,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.onboarding.forehead = Some(value);
                            cx.notify();
                        })),
                    ),
                );
            }
            body = body.child(
                div()
                    .v_flex()
                    .gap_2()
                    .child(div().font_bold().child(
                        self.text("onboarding-quiz-mocap_preferences-head_tracker_location-title"),
                    ))
                    .child(row),
            );
        }
        body.child(
            div()
                .h_flex()
                .justify_between()
                .child(
                    Button::new("setup-mocap-back")
                        .label(self.text("onboarding-quiz_back"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.onboarding_to(Step::Usage, cx)),
                        ),
                )
                .child(
                    Button::new("setup-mocap-finish")
                        .primary()
                        .label(self.text("onboarding-quiz_continue"))
                        .disabled(
                            !self.onboarding.can_finish_mocap()
                                || self.snapshot.connection != Connection::Connected
                                || !self.snapshot.rpc.contains_key("SettingsResponse"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| this.onboarding_finish(false, cx))),
                ),
        )
        .into_any_element()
    }
    pub(super) fn onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.onboarding.step == Step::Welcome {
            let _ = crate::ui_assets::image("slime/assignment-pose.webp", cx);
        }
        let width = f32::from(window.viewport_size().width) - 16.;
        let height = (f32::from(window.viewport_size().height) - 46.).max(300.);
        if self.onboarding.step == Step::Welcome {
            return self.setup_welcome(width, height, cx);
        }
        let step = self.onboarding.step;
        let inner = match step {
            Step::TrackerType => self.setup_types(width, window, cx),
            Step::Wifi => self.setup_wifi(width, window, cx),
            Step::Connect => self.setup_connect(width, window, cx),
            Step::Dongle => self.setup_dongle(width, window, cx),
            Step::Assignment => self.assignment_aligned(window, cx),
            Step::Mounting => self.mounting_aligned(window, cx),
            Step::Height => self.proportions_aligned(window, cx),
            Step::Usage => self.setup_usage(width, window, cx),
            Step::Runtime => self.setup_runtime(width, window, cx),
            Step::Mocap => self.setup_mocap(width, window, cx),
            Step::Welcome => unreachable!(),
        };
        div()
            .v_flex()
            .w_full()
            .flex_shrink_0()
            .min_h(px(height))
            .px_2()
            .py_4()
            .when(
                !matches!(
                    step,
                    Step::Connect | Step::Assignment | Step::Mounting | Step::Height | Step::Mocap
                ),
                |d| d.items_center().justify_center(),
            )
            .when(step == Step::Mocap, |d| d.items_center())
            .child(inner)
            .into_any_element()
    }
    pub(super) fn onboarding_progress(&self, cx: &Context<Self>) -> AnyElement {
        let value = self.onboarding.step.progress();
        div()
            .absolute()
            .top(px(20.))
            .left(relative(0.33))
            .w(relative(0.45))
            .h(px(3.))
            .h_flex()
            .gap_2()
            .children((0..3).map(|index| {
                div()
                    .flex_1()
                    .h_full()
                    .rounded_lg()
                    .bg(cx.theme().border)
                    .child(
                        div()
                            .h_full()
                            .rounded_lg()
                            .w(relative((value * 3. - index as f32).clamp(0., 1.)))
                            .bg(cx.theme().primary),
                    )
            }))
            .into_any_element()
    }
    pub(super) fn onboarding_escape(&self, cx: &mut Context<Self>) -> AnyElement {
        Button::new("setup-escape")
            .ghost()
            .p_0()
            .absolute()
            .right(px(20.))
            .top(px(54.))
            .w(px(42.))
            .h(px(60.))
            .child(
                div()
                    .v_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div()
                            .rounded_full()
                            .border_2()
                            .border_color(cx.theme().foreground)
                            .v_flex()
                            .items_center()
                            .justify_center()
                            .size(px(32.))
                            .text_xl()
                            .child("×"),
                    )
                    .child(div().text_xs().child("ESC")),
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.onboarding.dialog = Some(Dialog::Skip);
                cx.notify();
            }))
            .into_any_element()
    }
    pub(super) fn onboarding_dialog(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(dialog) = self.onboarding.dialog else {
            return div().into_any_element();
        };
        let width = (f32::from(window.viewport_size().width) - 40.).min(540.);
        let mut body = div()
            .id("setup-modal-body")
            .v_flex()
            .w(px(width))
            .p_6()
            .rounded_lg()
            .gap_4()
            .bg(cx.theme().popover)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        body = match dialog {
            Dialog::Skip => body
                .child(
                    div()
                        .p_4()
                        .rounded_lg()
                        .bg(cx.theme().warning.opacity(0.15))
                        .child(self.paragraph("onboarding-setup_warning", width - 80., window)),
                )
                .child(
                    Button::new("setup-keep")
                        .primary()
                        .label(self.text("onboarding-setup_warning-cancel"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.onboarding.dialog = None;
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("setup-skip-confirm")
                        .ghost()
                        .label(self.text("onboarding-setup_warning-skip"))
                        .on_click(cx.listener(|this, _, _, cx| this.onboarding_finish(true, cx))),
                ),
            Dialog::MoreSets => body
                .child(self.setup_title(
                    "onboarding-quiz-more_sets_modal-title",
                    Some("onboarding-quiz-more_sets_modal-desc"),
                    width - 48.,
                    window,
                ))
                .child(
                    Button::new("setup-more-yes")
                        .label(self.text("onboarding-quiz-more_sets_modal-cancel"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.onboarding_to(Step::TrackerType, cx)),
                        ),
                )
                .child(
                    Button::new("setup-all-connected")
                        .primary()
                        .label(self.text("onboarding-quiz-more_sets_modal-confirm"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.onboarding_to(Step::Assignment, cx)),
                        ),
                ),
            Dialog::WifiError(status) => {
                let key = if status == 9 {
                    "onboarding-connect_serial-error-modal-no_serial_log"
                } else {
                    "onboarding-connect_serial-error-modal-no_serial_device_found"
                };
                body.child(self.setup_title(key, Some(&format!("{key}-desc")), width - 48., window))
                    .child(
                        Button::new("setup-wifi-error-close")
                            .label(self.text("native-close"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.onboarding.dialog = None;
                                cx.notify();
                            })),
                    )
            }
        };
        div()
            .id("setup-modal")
            .absolute()
            .inset_0()
            .v_flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background.opacity(0.9))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.onboarding.dialog = None;
                    cx.notify();
                }),
            )
            .child(body)
            .into_any_element()
    }
}
