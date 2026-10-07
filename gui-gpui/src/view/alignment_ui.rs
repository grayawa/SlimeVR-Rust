use super::*;
use serde_json::json;

impl SlimeView {
    pub(super) fn tracker_highlight(&self, tracker: &Tracker) -> f32 {
        if self.snapshot.connection != Connection::Connected || !matches!(tracker.status, 2 | 3) {
            return 0.;
        }
        if self.highlight.is_some_and(|(device, sensor, at)| {
            device == tracker.key.device
                && sensor == tracker.key.sensor
                && at.elapsed() < Duration::from_secs(2)
        }) {
            1.
        } else {
            self.tracker_motion.level(tracker.key)
        }
    }
    pub(super) fn motion_glow<E: Styled>(
        &self,
        mut element: E,
        level: f32,
        radius: f32,
        cx: &Context<Self>,
    ) -> E {
        let size = (level * radius).floor();
        if size > 0. {
            element.style().box_shadow = Some(vec![
                BoxShadow::new(px(0.), px(0.), cx.theme().primary)
                    .blur_radius(px(size))
                    .spread_radius(px(size)),
            ]);
        }
        element
    }
    pub(super) fn topbar(&self, status: &str, cx: &mut Context<Self>) -> AnyElement {
        let bar = div()
            .h_flex()
            .h(px(42.))
            .w_full()
            .flex_shrink_0()
            .px_2()
            .gap_3()
            .child(
                svg()
                    .path("slime/SlimeVR.svg")
                    .size(px(32.))
                    .text_color(cx.theme().primary),
            )
            .child(div().font_bold().child("SlimeVR"))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded_lg()
                    .bg(rgb(0x10352f))
                    .text_color(rgb(0x50e897))
                    .child(format!("{} · GPUI test14", env!("CARGO_PKG_VERSION"))),
            )
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.text(status)),
            )
            .child(
                Button::new("topbar-settings")
                    .ghost()
                    .small()
                    .child(
                        svg()
                            .path("slime/Gear.svg")
                            .size(px(18.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .on_click(
                        cx.listener(|this, _, _, cx| this.go(Page::Settings(Section::SteamVr), cx)),
                    ),
            )
            .child(
                Button::new("topbar-help")
                    .ghost()
                    .small()
                    .label("?")
                    .on_click(|_, _, cx| cx.open_url("https://docs.slimevr.dev/")),
            );
        gpui_kit::component::TitleBar::new()
            .h(px(42.))
            .bg(cx.theme().background)
            .border_0()
            .child(bar)
            .on_close_window(cx.listener(|this, _, window, cx| this.request_close(window, cx)))
            .into_any_element()
    }
    pub(super) fn tracker_card_button(&self, tracker: &Tracker, cx: &mut Context<Self>) -> Button {
        let key = tracker.key;
        let highlight = self.tracker_highlight(tracker);
        let icon = match BodyPart(tracker.body).variant_name().unwrap_or("NONE") {
            "CHEST" => "Chest",
            "UPPER_CHEST" => "UpperChest",
            "HIP" => "Hip",
            "WAIST" => "Waist",
            "HEAD" => "Headset",
            "NECK" => "Neck",
            n if n.ends_with("UPPER_LEG") => "UpperLeg",
            n if n.ends_with("LOWER_LEG") => "Ankle",
            n if n.ends_with("FOOT") => "Foot",
            n if n.ends_with("HAND") => "Controller",
            n if n.ends_with("UPPER_ARM") => "UpperArm",
            n if n.ends_with("LOWER_ARM") => "LowerArm",
            n if n.ends_with("SHOULDER") => "Shoulder",
            _ => "SlimeVR",
        };
        let warning = slimevr_gpui::checklist::highlighted_trackers(
            &self.read("TrackingChecklistResponse"),
            &self.checklist_session_ignored,
        )
        .contains(&key);
        let status = TrackerStatus(tracker.status)
            .variant_name()
            .unwrap_or("NONE")
            .to_lowercase();
        Button::new(format!("tracker-card-{}-{}", key.device, key.sensor))
            .ghost()
            .text_size(px(12.))
            .w(relative(0.48))
            .min_w(px(210.))
            .h(px(74.))
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().popover)
            .cursor_pointer()
            .map(|card| self.motion_glow(card, highlight, 8., cx))
            .child(
                div()
                    .flex_shrink_0()
                    .size(px(44.))
                    .when(warning, |d| d.border_2().border_color(rgb(0xffe135)))
                    .rounded_lg()
                    .bg(cx.theme().secondary)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .path(format!("slime/{icon}.svg"))
                            .size(px(28.))
                            .text_color(cx.theme().link),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div().truncate().font_bold().child(
                            tracker
                                .custom_name
                                .clone()
                                .filter(|n| !n.is_empty())
                                .unwrap_or_else(|| {
                                    if tracker.body != 0 {
                                        self.body_name(tracker.body)
                                    } else {
                                        tracker.name.clone()
                                    }
                                }),
                        ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(
                                div()
                                    .size(px(7.))
                                    .rounded_full()
                                    .bg(if tracker.status == 2 {
                                        rgb(0x50e897)
                                    } else {
                                        rgb(0xdf6d8c)
                                    }),
                            )
                            .child(self.text(&format!("tracker-status-{status}"))),
                    ),
            )
            .child(
                div()
                    .v_flex()
                    .gap_1()
                    .text_xs()
                    .items_end()
                    .text_color(cx.theme().muted_foreground)
                    .child(self.tracker_battery(tracker, false, cx))
                    .child(self.tracker_wifi(tracker, false, cx)),
            )
    }
    pub(super) fn tracker_card(&self, tracker: &Tracker, cx: &mut Context<Self>) -> AnyElement {
        let key = tracker.key;
        self.tracker_card_button(tracker, cx)
            .on_click(cx.listener(move |this, _, _, cx| this.go(Page::Tracker(key), cx)))
            .into_any_element()
    }
    pub(super) fn home_aligned(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let wide = f32::from(window.viewport_size().width) >= 1050.;
        let height = (f32::from(window.viewport_size().height) - 58.).max(450.);
        let connected = self.snapshot.connection == Connection::Connected;
        let busy = self.snapshot.pending.is_some();
        let mut reset_row = div().h_flex().gap_3().flex_wrap();
        for kind in [ResetKind::Full, ResetKind::Yaw, ResetKind::Mounting] {
            let permitted = self.snapshot.feed.as_ref().is_some_and(|f| match kind {
                ResetKind::Full => true,
                ResetKind::Yaw => f.can_yaw,
                ResetKind::Mounting => f.can_mount,
            });
            reset_row = reset_row.child(
                Button::new(format!("reset-home-{kind:?}"))
                    .label(self.text(kind.label()))
                    .bg(cx.theme().popover)
                    .h(px(72.))
                    .flex_1()
                    .min_w(px(115.))
                    .disabled(!connected || busy || !permitted)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.send(Command::Reset(kind), cx)),
                    ),
            );
        }
        let feet = vec![BodyPart::LEFT_FOOT.0, BodyPart::RIGHT_FOOT.0];
        let has_feet = self.snapshot.feed.as_ref().is_some_and(|f| {
            f.trackers
                .iter()
                .any(|t| !t.computed && feet.contains(&t.body))
        });
        reset_row = reset_row.child(
            Button::new("reset-home-feet")
                .label(self.text("reset-mounting-feet"))
                .bg(cx.theme().popover)
                .h(px(72.))
                .flex_1()
                .min_w(px(115.))
                .disabled(!connected || busy || !has_feet)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.rpc(
                        "ResetRequest",
                        json!({"reset_type":2,"body_parts":feet}),
                        cx,
                    )
                })),
        );
        let mut main = div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().muted)
            .min_h(px(height))
            .child(
                div()
                    .font_bold()
                    .child(self.text("native-group-resets_settings")),
            )
            .child(reset_row)
            .when_some(self.snapshot.reset.as_ref(), |d, reset| {
                d.child(div().text_color(cx.theme().link).child(if reset.done {
                    self.text("native-reset-finished")
                } else {
                    format!(
                        "{:.1}s",
                        (reset.duration_ms - reset.progress_ms).max(0) as f32 / 1000.
                    )
                }))
            });
        if let Some(feed) = &self.snapshot.feed {
            for assigned in [true, false] {
                let trackers: Vec<_> = feed
                    .trackers
                    .iter()
                    .filter(|t| {
                        (!t.computed
                            || self.preferences.value["debug"] == true
                                && self.preferences.value["devSettings"]["filterSlimesAndHMD"]
                                    != true)
                            && (t.body != 0) == assigned
                    })
                    .collect();
                if trackers.is_empty() {
                    continue;
                }
                let title = if assigned {
                    "native-assigned-trackers"
                } else {
                    "native-unassigned-trackers"
                };
                let table = self.preferences.value["homeLayout"] == "table";
                let mut cards = div()
                    .flex()
                    .gap_3()
                    .when(table, |d| d.flex_col())
                    .when(!table, |d| d.flex_row().flex_wrap());
                let mut trackers = trackers;
                if self.preferences.value["devSettings"]["sortByName"] == true {
                    trackers.sort_by(|a, b| a.name.cmp(&b.name));
                }
                for tracker in trackers {
                    cards = cards.child(self.tracker(tracker, false, cx));
                }
                main = main
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .child(div().font_bold().child(self.text(title)))
                            .child(div().flex_1().h(px(1.)).bg(cx.theme().border))
                            .child(
                                Button::new(format!("home-layout-{assigned}"))
                                    .ghost()
                                    .small()
                                    .label(if table { "▦" } else { "☷" })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.preference(
                                            "homeLayout",
                                            json!(if table { "default" } else { "table" }),
                                            cx,
                                        )
                                    })),
                            ),
                    )
                    .child(cards);
            }
            if !feed.trackers.iter().any(|t| !t.computed) {
                main = main.child(self.text("native-no-trackers"));
            }
        } else {
            main = main.child(self.text("native-waiting"));
        }
        let paused = self.snapshot.paused.unwrap_or(false);
        let mut side = div()
            .v_flex()
            .gap_3()
            .when(wide, |d| d.w(px(376.)).flex_shrink_0())
            .when(!wide, |d| d.w_full())
            .when(!self.checklist_closed, |d| d.h(px(height)))
            .child(self.checklist_panel(true, cx));
        if self.checklist_closed && self.preferences.value["skeletonPreview"] != false {
            side = side.child(self.skeleton_panel((height - 260.).max(270.), cx));
        }
        if self.checklist_closed {
            side = side
                .child(
                    div()
                        .h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(self.bvh(cx))
                        .child(
                            Button::new("home-pause")
                                .small()
                                .label(self.text(if paused {
                                    "tracking-paused"
                                } else {
                                    "tracking-unpaused"
                                }))
                                .disabled(!connected || busy || self.snapshot.paused.is_none())
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.send(Command::Pause(!paused), cx)
                                })),
                        )
                        .child(
                            Button::new("home-vr-mode")
                                .small()
                                .label(self.text("vrmode-title"))
                                .on_click(cx.listener(|this, _, _, cx| this.go(Page::VrMode, cx))),
                        ),
                )
                .child(self.preview_menu(cx));
        }
        div()
            .flex()
            .gap_2()
            .when(wide, |d| d.flex_row())
            .when(!wide, |d| d.flex_col())
            .child(main)
            .child(side)
            .into_any_element()
    }
    fn role_picker(
        &self,
        id: String,
        body: u8,
        dot: bool,
        left: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let assigned: Vec<_> = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| {
                f.trackers
                    .iter()
                    .filter(|t| !t.computed && t.body == body)
                    .collect()
            })
            .unwrap_or_default();
        let highlight = assigned
            .iter()
            .map(|t| self.tracker_highlight(t))
            .fold(0., f32::max);
        let warnings = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| slimevr_gpui::assignment::warnings(&f.trackers))
            .unwrap_or_default();
        let invalid = warnings.iter().any(|w| w.body == body);
        let missing = warnings.first().is_some_and(|w| w.affected.contains(&body));
        let mut button = Button::new(id).ghost().small().disabled(
            self.snapshot.connection != Connection::Connected || self.snapshot.pending.is_some(),
        );
        if dot {
            button = button.size(px(24.)).p_0().child(
                div()
                    .size(px(15.))
                    .flex_shrink_0()
                    .rounded_full()
                    .border_color(cx.theme().link)
                    .border(px(2. + highlight * 2.))
                    .bg(if missing {
                        rgb(0xffe135).into()
                    } else if highlight > 0. {
                        cx.theme().link
                    } else if !assigned.is_empty() {
                        rgb(0x50e897).into()
                    } else {
                        cx.theme().foreground
                    }),
            );
        } else {
            let mut label = div()
                .w_full()
                .v_flex()
                .gap_1()
                .min_w_0()
                .when(left, |d| d.items_end().text_right())
                .when(!left, |d| d.items_start())
                .child(div().font_bold().child(format!(
                    "{}{}",
                    if invalid { "⚠ " } else { "" },
                    self.body_name(body)
                )));
            if assigned.is_empty() {
                label = label.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.text("tracker-part_card-unassigned")),
                );
            } else {
                for tracker in assigned {
                    label = label.child(div().truncate().child(tracker.name.clone()));
                }
            }
            button = button
                .w(px(125.))
                .min_h(px(44.))
                .h_auto()
                .px_2()
                .py_1()
                .when(left, |d| d.justify_end())
                .when(!left, |d| d.justify_start())
                .map(|label| self.motion_glow(label, highlight, 3., cx))
                .child(label);
        }
        if (self.navigation.page == Page::Mounting
            || self.in_onboarding()
                && self.onboarding.step == slimevr_gpui::onboarding::Step::Mounting)
            && !self.mounting_body_active()
        {
            return button.into_any_element();
        }
        button
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(key) = this.tracker_role_target {
                    this.assign_tracker_role(key, body, cx);
                } else if this.mounting_body_active() {
                    this.mounting_target = Some(super::mounting_ui::MountTarget::Body(body));
                } else {
                    this.assignment_role = Some(body);
                }
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .into_any_element()
    }
    pub(super) fn body_diagram(
        &self,
        pane_width: f32,
        height: f32,
        points: Vec<slimevr_gpui::assignment::Target>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_height = height
            .min(
                (pane_width
                    - if self.mounting_body_active() {
                        140.
                    } else {
                        190.
                    })
                    * 392.
                    / 163.,
            )
            .max(260.);
        let image_width = image_height * 163. / 392.;
        let image_x = (pane_width - image_width) / 2.;
        let image_y = (height - image_height) / 2.;
        let lines: Vec<_> = points
            .iter()
            .map(|t| {
                let dot = (
                    image_x + t.x / 163. * image_width,
                    image_y + t.y / 392. * image_height,
                );
                let label_y = t.label_y * height;
                (
                    dot,
                    if t.left {
                        if self.mounting_body_active() {
                            50.
                        } else {
                            130.
                        }
                    } else {
                        pane_width
                            - if self.mounting_body_active() {
                                60.
                            } else {
                                130.
                            }
                    },
                    label_y + 20.,
                )
            })
            .collect();
        let mut pane = div()
            .relative()
            .w(px(pane_width))
            .h(px(height))
            .flex_shrink_0()
            .child(
                crate::ui_assets::image("slime/assignment-pose.webp", cx)
                    .id("assignment-character")
                    .object_fit(ObjectFit::Fill)
                    .absolute()
                    .left(px(image_x))
                    .top(px(image_y))
                    .w(px(image_width))
                    .h(px(image_height)),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        for ((x, y), end, ey) in &lines {
                            let mut path = PathBuilder::stroke(px(2.));
                            path.move_to(point(bounds.origin.x + px(*x), bounds.origin.y + px(*y)));
                            path.line_to(point(
                                bounds.origin.x
                                    + px(*end + if *end < pane_width / 2. { 40. } else { -40. }),
                                bounds.origin.y + px(*ey),
                            ));
                            path.line_to(point(
                                bounds.origin.x + px(*end),
                                bounds.origin.y + px(*ey),
                            ));
                            if let Ok(path) = path.build() {
                                window.paint_path(path, rgb(0x608aab));
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );
        for t in points {
            let x = image_x + t.x / 163. * image_width;
            let y = image_y + t.y / 392. * image_height;
            pane =
                pane.child(div().absolute().left(px(x - 12.)).top(px(y - 12.)).child(
                    self.role_picker(format!("role-dot-{}", t.body), t.body, true, t.left, cx),
                ))
                .child(
                    div()
                        .absolute()
                        .left(px(if t.left {
                            if self.mounting_body_active() {
                                -80.
                            } else {
                                0.
                            }
                        } else {
                            pane_width
                                - if self.mounting_body_active() {
                                    60.
                                } else {
                                    125.
                                }
                        }))
                        .top(px(t.label_y * height))
                        .child(self.role_picker(
                            format!("role-label-{}", t.body),
                            t.body,
                            false,
                            t.left,
                            cx,
                        )),
                );
        }
        pane.into_any_element()
    }
    pub(super) fn assignment_aligned(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let width = (f32::from(window.viewport_size().width)
            - if self.in_onboarding() { 32. } else { 150. }
            - if self.navigation.page.in_settings() && !self.in_onboarding() {
                202.
            } else {
                0.
            })
        .max(300.);
        let narrow = width < 950.;
        let column = if narrow { width } else { 448. };
        let pane_width = if narrow { width } else { width - column - 16. };
        let height = (f32::from(window.viewport_size().height) - 82.).max(550.);
        let connected_count = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| {
                f.trackers
                    .iter()
                    .filter(|t| {
                        !t.computed && t.status != TrackerStatus::DISCONNECTED.0 && t.is_imu
                    })
                    .count()
            })
            .unwrap_or(0);
        let mode = self.preferences.value["assignMode"]
            .as_str()
            .unwrap_or_else(|| slimevr_gpui::assignment::preferred_mode(connected_count));
        let mirror = self.preferences.value["mirrorView"] != false;
        let mut points = slimevr_gpui::assignment::targets(mode, mirror);
        slimevr_gpui::assignment::position_labels(&mut points, height);
        let physical: Vec<_> = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| f.trackers.iter().filter(|t| !t.computed).collect())
            .unwrap_or_default();
        let mut args = fluent_bundle::FluentArgs::new();
        args.set(
            "assigned",
            physical.iter().filter(|t| t.body != 0).count() as f64,
        );
        args.set("trackers", physical.len() as f64);
        let mut controls = div()
            .v_flex()
            .w(px(column))
            .flex_shrink_0()
            .gap_3()
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("onboarding-assign_trackers-title")),
            )
            .child(self.text("onboarding-assign_trackers-description"))
            .child(
                self.l10n
                    .format("onboarding-assign_trackers-assigned", &args),
            )
            .child(
                div()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().secondary)
                    .text_color(cx.theme().link)
                    .h_flex()
                    .gap_4()
                    .child(
                        svg()
                            .path("slime/Bulb.svg")
                            .size(px(20.))
                            .flex_shrink_0()
                            .text_color(cx.theme().link),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.text("tips-find_tracker")),
                    ),
            );
        if let Some(warning) = self.snapshot.feed.as_ref().and_then(|f| {
            slimevr_gpui::assignment::warnings(&f.trackers)
                .into_iter()
                .next()
        }) {
            let mut args = fluent_bundle::FluentArgs::new();
            args.set("unassigned", warning.satisfied as f64);
            let label = self.l10n.format(
                &format!(
                    "onboarding-assign_trackers-warning-{}",
                    BodyPart(warning.body).variant_name().unwrap_or("NONE")
                ),
                &args,
            );
            controls = controls.child(
                div()
                    .p_3()
                    .rounded_lg()
                    .bg(rgb(0xffe135))
                    .text_color(rgb(0x081e30))
                    .child(label),
            );
        }
        let mut modes = div().v_flex().gap_2();
        if narrow {
            let (amount, label, description) = self.assignment_mode_strings(mode);
            let view = cx.entity().downgrade();
            let options: Vec<_> = ["lower-body", "core", "enhanced-core", "full-body", "all"]
                .into_iter()
                .map(|mode| (mode, self.assignment_mode_strings(mode)))
                .collect();
            modes = modes.child(
                Button::new("assignment-mode-select")
                    .ghost()
                    .w_full()
                    .h(px(66.))
                    .p_4()
                    .bg(cx.theme().popover)
                    .child(
                        div()
                            .h_flex()
                            .w_full()
                            .gap_3()
                            .child(div().flex_1().min_w_0().child(assignment_ui::mode_option(
                                amount,
                                label,
                                description,
                            )))
                            .child("⌄"),
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = menu.min_w(px(column)).max_w(px(column));
                        for (mode, (amount, label, description)) in &options {
                            let view = view.clone();
                            let mode = *mode;
                            let amount = amount.clone();
                            let label = label.clone();
                            let description = description.clone();
                            menu = menu.item(
                                PopupMenuItem::element(move |_, _| {
                                    assignment_ui::mode_option(
                                        amount.clone(),
                                        label.clone(),
                                        description.clone(),
                                    )
                                })
                                .on_click(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        this.preference("assignMode", json!(mode), cx)
                                    });
                                }),
                            );
                        }
                        menu
                    }),
            );
        } else {
            for (option, count) in [
                ("lower-body", 5),
                ("core", 6),
                ("enhanced-core", 8),
                ("full-body", 10),
                ("all", 20),
            ] {
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("mode", option);
                args.set("trackersCount", count);
                let selected = mode == option;
                modes =
                    modes.child(
                        Button::new(format!("assignment-mode-{option}"))
                            .ghost()
                            .w_full()
                            .h(px(66.))
                            .justify_start()
                            .gap_3()
                            .p_4()
                            .rounded_lg()
                            .border_2()
                            .border_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().popover
                            })
                            .bg(cx.theme().popover)
                            .child(
                                div()
                                    .h_flex()
                                    .w_full()
                                    .gap_3()
                                    .child(div().text_2xl().font_bold().child(
                                        self.l10n.format(
                                            "onboarding-assign_trackers-option-amount",
                                            &args,
                                        ),
                                    ))
                                    .child(
                                        div()
                                            .v_flex()
                                            .items_start()
                                            .min_w_0()
                                            .child(self.l10n.format(
                                                "onboarding-assign_trackers-option-label",
                                                &args,
                                            ))
                                            .child(self.l10n.format(
                                                "onboarding-assign_trackers-option-description",
                                                &args,
                                            )),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preference("assignMode", json!(option), cx)
                            })),
                    );
            }
        }
        controls = controls.child(modes);
        controls = controls.child(div().h_flex().gap_3().child(Switch::new("assignment-mirror").checked(mirror).small()
            .accessibility_label(self.text("onboarding-assign_trackers-mirror_view"))
            .on_click(cx.listener(|this,enabled,_,cx|this.preference("mirrorView",json!(*enabled),cx))))
            .child(self.text("onboarding-assign_trackers-mirror_view")))
            .when(!self.in_onboarding(),|d|d.child(Button::new("unassign-all").label(self.text("onboarding-assign_trackers-unassign_all"))
                .on_click(cx.listener(|this,_,_,cx| {
                    let requests=this.snapshot.feed.as_ref().map(|f|f.trackers.iter().filter(|t|!t.computed&&t.editable).map(|t|("AssignTrackerRequest".into(),json!({"tracker_id":{"device_id":{"id":t.key.device},"tracker_num":t.key.sensor},"body_position":0}))).collect()).unwrap_or_default();this.batch(requests,cx);
                }))));
        if self.in_onboarding() {
            let back = self
                .onboarding
                .tracker_set
                .map(|s| s.connection())
                .unwrap_or(slimevr_gpui::onboarding::Step::Wifi);
            controls = controls.child(self.onboarding_nav(
                back,
                slimevr_gpui::onboarding::Step::Mounting,
                !physical.is_empty() && !physical.iter().any(|t| t.body != 0),
                cx,
            ));
        }
        let pane = self.body_diagram(pane_width, height, points, cx);
        div()
            .flex()
            .when(narrow, |d| d.flex_col())
            .when(!narrow, |d| d.flex_row())
            .items_start()
            .gap_4()
            .child(controls)
            .child(pane)
            .into_any_element()
    }
}

impl SlimeView {}

impl SlimeView {
    fn preview_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let toggles = self.read("SettingsResponse")["model_settings"]["toggles"].clone();
        let choices: Vec<_> = ["floor_clip", "skating_correction", "toe_snap", "foot_plant"]
            .into_iter()
            .map(|field| {
                (
                    field,
                    self.text(&format!("native-tmp-{field}")),
                    self.temporary_tweaks
                        .get(field)
                        .copied()
                        .unwrap_or(toggles[field] == true),
                )
            })
            .collect();
        let clear = self.text("native-tmp-clear");
        let view = cx.entity().downgrade();
        Button::new("home-preview-options").ghost().small()
            .child(svg().path("slime/Gear.svg").size(px(16.)).text_color(cx.theme().muted_foreground))
            .dropdown_menu(move |mut menu,_,_| {
                for (field,label,enabled) in &choices {
                    let view=view.clone();let field=*field;let enabled=*enabled;
                    menu=menu.item(PopupMenuItem::new(label.clone()).checked(enabled).on_click(move |_,_,cx| {
                        let _=view.update(cx,|this,cx| {
                            let mut value=json!({});value[field]=json!(!enabled);
                            if this.client.rpc("LegTweaksTmpChange",value).is_ok(){this.temporary_tweaks.insert(field.into(),!enabled);}
                            cx.notify();
                        });
                    }));
                }
                let view=view.clone();menu.item(PopupMenuItem::new(clear.clone()).on_click(move |_,_,cx| {
                    let _=view.update(cx,|this,cx| {
                        this.rpc("LegTweaksTmpClear",json!({"floor_clip":true,"skating_correction":true,"toe_snap":true,"foot_plant":true}),cx);
                        this.temporary_tweaks.clear();
                    });
                }))
            }).into_any_element()
    }
}

impl SlimeView {
    pub(super) fn connect_aligned(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let wide = f32::from(window.viewport_size().width) >= 1050.;
        let ssid = self.local_input("wifi-ssid", "", false, window, cx);
        let password = self.local_input("wifi-password", "", true, window, cx);
        let left = div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_4()
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(
                        svg()
                            .path("slime/Wifi.svg")
                            .size(px(32.))
                            .text_color(cx.theme().link),
                    )
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("onboarding-wifi_creds-dongle-title")),
                    ),
            )
            .child(self.text("onboarding-wifi_creds-dongle-description"))
            .child(
                Button::new("connect-dongle-continue")
                    .primary()
                    .label(self.text("onboarding-wifi_creds-dongle-continue"))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Page::Home, cx))),
            );
        let status = self.read("WifiProvisioningStatusResponse");
        let state = status["status"].as_u64().and_then(|id| {
            slimevr_gpui::rpc_generated::enum_choices("WifiProvisioningStatus")
                .iter()
                .find(|(_, n)| *n == id)
                .map(|(n, _)| self.text(&format!("native-wifi-state-{n}")))
        });
        let right=div().v_flex().flex_1().min_w_0().gap_4()
            .child(div().h_flex().gap_3().child(svg().path("slime/Wifi.svg").size(px(32.)).text_color(cx.theme().link))
                .child(div().text_2xl().font_bold().child(self.text("onboarding-wifi_creds-v2"))))
            .child(self.text("onboarding-wifi_creds-description-v2"))
            .child(div().v_flex().gap_3().p_6().rounded_lg().bg(cx.theme().popover)
                .child(self.text("onboarding-wifi_creds-ssid")).child(ssid)
                .child(self.text("onboarding-wifi_creds-password")).child(password)
                .child(div().h_flex().gap_3().flex_wrap()
                    .child(Button::new("wifi-start").primary().label(self.text("onboarding-wifi_creds-submit"))
                        .disabled(self.snapshot.connection!=Connection::Connected)
                        .on_click(cx.listener(|this,_,_,cx| {
                            let ssid=this.form_value("wifi-ssid","");
                            if ssid.trim().is_empty(){this.ui_error=Some(this.text("onboarding-wifi_creds-ssid-required"));cx.notify();return;}
                            this.rpc("StartWifiProvisioningRequest",json!({"ssid":ssid,"password":this.form_value("wifi-password",""),"port":this.form_value("serial-port","")}),cx);
                        })))
                    .child(self.rpc_button("wifi-stop","native-wifi-stop","StopWifiProvisioningRequest",json!({}),cx)))
                .when_some(state,|d,state|d.child(state)))
            .child(Button::new("connect-serial-tools").ghost().label(self.text("native-serial-tools"))
                .on_click(cx.listener(|this,_,_,cx|this.go(Page::Settings(Section::Serial),cx))));
        div()
            .flex()
            .gap_8()
            .px_6()
            .pt(px(if wide { 80. } else { 20. }))
            .pb_6()
            .when(wide, |d| d.flex_row())
            .when(!wide, |d| d.flex_col())
            .child(left)
            .child(right)
            .into_any_element()
    }
}
