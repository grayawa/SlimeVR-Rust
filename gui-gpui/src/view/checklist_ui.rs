use super::*;
use serde_json::{Value, json};
use slimevr_gpui::checklist::{self, Status, Step};

impl SlimeView {
    pub(super) fn checklist(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .v_flex()
            .gap_3()
            .child(
                Button::new("checklist-back")
                    .label(self.text("native-back"))
                    .on_click(cx.listener(|this, _, _, cx| this.go(this.return_page, cx))),
            )
            .child(self.checklist_panel(false, cx))
            .into_any_element()
    }
    pub(super) fn checklist_panel(&self, home: bool, cx: &mut Context<Self>) -> AnyElement {
        let state = checklist::derive(
            &self.read("TrackingChecklistResponse"),
            &self.checklist_session_ignored,
        );
        let collapsed = home && self.checklist_closed;
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("count", state.warnings as f64);
        let summary = self.l10n.format(
            &format!("tracking_checklist-status-{}", state.completion),
            &args,
        );
        let color = match state.completion {
            "complete" => rgb(0x50e897),
            "partial" => rgb(0xffe135),
            _ => cx.theme().link.into(),
        };
        let mut panel = div()
            .id(if home {
                "home-checklist-panel"
            } else {
                "checklist-panel"
            })
            .v_flex()
            .w_full()
            .when(home && !collapsed, |d| d.h_full())
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                div()
                    .h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_lg()
                            .font_bold()
                            .child(self.text("tracking_checklist")),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_1()
                            .child(
                                Button::new("checklist-settings")
                                    .ghost()
                                    .small()
                                    .label("⚙")
                                    .tooltip(self.text("tracking_checklist-settings"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.go(Page::Settings(Section::Checklist), cx)
                                    })),
                            )
                            .when(home, |d| {
                                d.child(
                                    Button::new("checklist-collapse")
                                        .ghost()
                                        .small()
                                        .label(if collapsed { "⌄" } else { "×" })
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.checklist_closed = !this.checklist_closed;
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            );
        if !collapsed {
            let mut list = div()
                .id("checklist-steps")
                .v_flex()
                .gap_3()
                .overflow_y_scroll()
                .flex_1()
                .min_h_0();
            for step in &state.steps {
                let id = step.id;
                let expanded = step.first_required || self.checklist_open.contains(&id);
                let expandable = matches!(step.status, Status::Skipped | Status::Invalid)
                    && !step.first_required;
                let symbol = if matches!(step.status, Status::Complete | Status::Skipped) {
                    "✓"
                } else {
                    "●"
                };
                let mut item = div()
                    .v_flex()
                    .gap_2()
                    .pl_3()
                    .border_l_2()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new(format!("checklist-step-{id}"))
                            .ghost()
                            .w_full()
                            .child(
                                div()
                                    .h_flex()
                                    .gap_2()
                                    .w_full()
                                    .items_center()
                                    .child(
                                        div()
                                            .w(px(24.))
                                            .text_color(if step.status == Status::Complete {
                                                rgb(0x50e897)
                                            } else {
                                                cx.theme().muted_foreground.into()
                                            })
                                            .child(symbol),
                                    )
                                    .child(div().flex_1().font_bold().child(self.text(&format!(
                                        "tracking_checklist-{}",
                                        checklist::name(id)
                                    ))))
                                    .when(expandable, |d| {
                                        d.child(if expanded { "⌃" } else { "⌄" })
                                    }),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if expandable {
                                    if !this.checklist_open.remove(&id) {
                                        this.checklist_open.insert(id);
                                    }
                                    cx.notify();
                                }
                            })),
                    );
                if expanded {
                    item = item.child(self.checklist_detail(step, cx));
                }
                list = list.child(item);
            }
            if self.snapshot.connection != Connection::Connected {
                list = list.child(self.text("native-waiting"));
            }
            panel = panel.child(list).child(
                div()
                    .h(px(5.))
                    .w_full()
                    .rounded_full()
                    .bg(cx.theme().border)
                    .child(
                        div()
                            .h_full()
                            .w(relative(state.progress))
                            .rounded_full()
                            .bg(color),
                    ),
            );
        }
        panel = panel.child(
            Button::new("checklist-status")
                .ghost()
                .w_full()
                .child(
                    div()
                        .h_flex()
                        .gap_2()
                        .w_full()
                        .child(div().text_color(color).child("●"))
                        .child(summary),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    if home {
                        this.checklist_closed = !this.checklist_closed;
                        cx.notify();
                    }
                })),
        );
        panel.into_any_element()
    }
    fn checklist_detail(&self, step: &Step, cx: &mut Context<Self>) -> AnyElement {
        let name = checklist::name(step.id);
        let extra = &step.data["extra_data"]["value"];
        let blocked = self.snapshot.connection != Connection::Connected;
        let mut description = format!("tracking_checklist-{name}-desc");
        if name == "STEAMVR_DISCONNECTED" {
            description = format!(
                "tracking_checklist-STEAMVR_DISCONNECTED{}-desc",
                if extra["driver_blocked_by_safe_mode"] == true {
                    "-driver_blocked"
                } else if extra["driver_enabled"] == false {
                    "-driver_disabled"
                } else if extra["driver_installed"] == false {
                    "-driver_not_installed"
                } else {
                    ""
                }
            );
        }
        let mut args = fluent_bundle::FluentArgs::new();
        let adapters = extra["adapters"].as_array().cloned().unwrap_or_default();
        args.set("count", adapters.len() as f64);
        args.set(
            "adapters",
            adapters
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        );
        let description = self.l10n.format(&description, &args);
        let mut detail = div().v_flex().gap_3().pl_5().pr_1();
        if !description.starts_with("tracking_checklist-") {
            detail = detail.child(description);
        }
        let mut actions = div().h_flex().gap_2().flex_wrap();
        match name {
            "FULL_RESET" => {
                for key in [
                    "onboarding-automatic_mounting-preparation-v2-step-0",
                    "onboarding-automatic_mounting-preparation-v2-step-1",
                    "onboarding-automatic_mounting-preparation-v2-step-2",
                ] {
                    detail = detail.child(self.text(key));
                }
                let mut poses = div().h_flex().gap_2();
                for (index, pose) in ["FullResetPose", "FullResetPoseSide", "FullResetPoseWrong"]
                    .iter()
                    .enumerate()
                {
                    poses = poses.child(
                        div()
                            .v_flex()
                            .flex_1()
                            .min_w_0()
                            .rounded_lg()
                            .bg(cx.theme().background)
                            .child(
                                div()
                                    .text_color(if index == 2 {
                                        rgb(0xff5656)
                                    } else {
                                        rgb(0x50e897)
                                    })
                                    .child(if index == 2 { "×" } else { "✓" }),
                            )
                            .child(
                                crate::ui_assets::image(&format!("slime/reset/{pose}.webp"), cx)
                                    .w_full()
                                    .h(px(160.)),
                            ),
                    );
                }
                detail = detail.child(poses);
                actions = actions.child(
                    Button::new("checklist-full-reset")
                        .primary()
                        .label(self.text("reset-full"))
                        .disabled(blocked || self.snapshot.pending.is_some())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.send(Command::Reset(ResetKind::Full), cx)
                        })),
                );
            }
            "MOUNTING_CALIBRATION" | "FEET_MOUNTING_CALIBRATION" => {
                let feet = name == "FEET_MOUNTING_CALIBRATION";
                for key in if feet {
                    vec![
                        "onboarding-automatic_mounting-mounting_reset-feet-step-0",
                        "onboarding-automatic_mounting-mounting_reset-feet-step-1",
                    ]
                } else {
                    vec![
                        "onboarding-automatic_mounting-mounting_reset-step-0",
                        "onboarding-automatic_mounting-mounting_reset-step-1",
                    ]
                } {
                    detail = detail.child(self.text(key));
                }
                let mut poses = div().h_flex().gap_2();
                for path in if feet {
                    vec![
                        "slime/mounting/MountingFeets.webp",
                        "slime/mounting/MountingFeetsSide.webp",
                    ]
                } else {
                    vec!["slime/mounting-reset-pose.webp"]
                } {
                    poses = poses.child(
                        crate::ui_assets::image(path, cx)
                            .h(px(165.))
                            .flex_1()
                            .min_w_0(),
                    );
                }
                detail = detail.child(poses);
                actions=actions.child(Button::new("checklist-mounting-reset").primary().label(self.text(if feet {"onboarding-automatic_mounting-mounting_reset-feet"} else {"reset-mounting"}))
                    .disabled(blocked || self.snapshot.pending.is_some())
                    .on_click(cx.listener(move |this, _, _, cx| if feet {this.rpc("ResetRequest",json!({"reset_type":2,"body_parts":[BodyPart::LEFT_FOOT.0,BodyPart::RIGHT_FOOT.0]}),cx)} else {this.send(Command::Reset(ResetKind::Mounting),cx)})));
            }
            "STEAMVR_DISCONNECTED" => {
                if extra["driver_blocked_by_safe_mode"] == true || extra["driver_enabled"] == false
                {
                    actions = actions.child(self.rpc_button(
                        "checklist-enable-driver",
                        "tracking_checklist-STEAMVR_DISCONNECTED-enable",
                        "EnableSteamVRDriverRequest",
                        json!({}),
                        cx,
                    ));
                } else if extra["driver_installed"] != false {
                    actions = actions.child(
                        Button::new("checklist-launch-steamvr")
                            .primary()
                            .label(self.text("tracking_checklist-STEAMVR_DISCONNECTED-open"))
                            .on_click(|_, _, cx| cx.open_url("steam://run/250820")),
                    );
                }
            }
            "NETWORK_PROFILE_PUBLIC" => {
                actions = actions.child(
                    Button::new("checklist-network")
                        .primary()
                        .label(self.text("tracking_checklist-NETWORK_PROFILE_PUBLIC-open"))
                        .on_click(|_, _, cx| cx.open_url("ms-settings:network")),
                );
            }
            "VRCHAT_SETTINGS" | "STAY_ALIGNED_CONFIGURED" => {
                let target = if name == "VRCHAT_SETTINGS" {
                    Page::VrchatWarnings
                } else {
                    Page::Settings(Section::StayAligned)
                };
                actions = actions.child(
                    Button::new(format!("checklist-open-{}", step.id))
                        .primary()
                        .label(self.text(&format!("tracking_checklist-{name}-open")))
                        .on_click(cx.listener(move |this, _, _, cx| this.go(target, cx))),
                );
            }
            "STEAMVR_HANDS_ENABLED" => {
                let mut sharing = self.read("SettingsResponse")["steam_vr_trackers"].clone();
                sharing["left_hand"] = json!(false);
                sharing["right_hand"] = json!(false);
                actions = actions.child(self.rpc_button(
                    "disable-steamvr-hands",
                    "tracking_checklist-STEAMVR_HANDS_ENABLED-go",
                    "ChangeSettingsRequest",
                    json!({"steam_vr_trackers":sharing}),
                    cx,
                ));
            }
            _ => (),
        }
        if step.data["ignorable"] == true {
            let id = step.id;
            actions = actions.child(
                Button::new(format!("checklist-session-ignore-{id}"))
                    .label(self.text("tracking_checklist-ignore"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.checklist_session_ignored.insert(id);
                        let _ = this.client.rpc("TrackingChecklistRequest", json!({}));
                        cx.notify();
                    })),
            );
        }
        detail.child(actions).into_any_element()
    }
    pub(super) fn checklist_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self.read("TrackingChecklistResponse");
        let ignored = value["ignored_steps"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mut toggles = div().flex().flex_wrap().gap_3();
        for step in value["steps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["enabled"] == true)
        {
            let id = step["id"].as_u64().unwrap_or(0);
            let checked = !ignored.contains(&json!(id));
            toggles =
                toggles.child(
                    div()
                        .h_flex()
                        .items_center()
                        .gap_3()
                        .w(relative(0.48))
                        .min_h(px(52.))
                        .p_3()
                        .rounded_lg()
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            Switch::new(format!("checklist-setting-{id}"))
                                .checked(checked)
                                .small()
                                .disabled(
                                    step["ignorable"] != true
                                        || self.snapshot.connection != Connection::Connected,
                                )
                                .on_click(cx.listener(move |this, enabled: &bool, _, cx| {
                                    this.rpc(
                                        "IgnoreTrackingChecklistStepRequest",
                                        json!({"step_id":id,"ignore":!*enabled}),
                                        cx,
                                    )
                                })),
                        )
                        .child(div().flex_1().child(
                            self.text(&format!("tracking_checklist-{}", checklist::name(id))),
                        )),
                );
        }
        self.settings_pane(
            "tracking_checklist",
            "Check",
            div()
                .v_flex()
                .gap_3()
                .child(
                    div()
                        .font_bold()
                        .child(self.text("settings-tracking_checklist-active_steps")),
                )
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.text("settings-tracking_checklist-active_steps-desc")),
                )
                .child(toggles)
                .into_any_element(),
            cx,
        )
    }
}
