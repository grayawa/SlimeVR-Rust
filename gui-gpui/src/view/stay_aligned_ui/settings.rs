use super::*;
use slimevr_ui::components::SwitchRow;

impl SlimeView {
    fn stay_aligned_switch(
        &self,
        field: &str,
        label: &str,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = format!("/stay_aligned/{field}");
        SwitchRow::new(
            self.text(label),
            Switch::new(format!("stay-{field}"))
                .checked(self.draft.value.pointer(&path) == Some(&json!(true)))
                .small()
                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                    this.draft.set(&path, json!(*on));
                    cx.notify();
                })),
        )
        .disabled(
            disabled
                || self.snapshot.connection != Connection::Connected
                || self.draft.saving.is_some(),
        )
        .build(cx)
        .into_any_element()
    }
    pub(in crate::view) fn stay_aligned_settings(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.snapshot.rpc.contains_key("SettingsResponse") {
            return div().child(self.text("native-waiting")).into_any_element();
        }
        let width = (f32::from(window.viewport_size().width) - 410.).max(240.);
        let ready = self.draft.value["stay_aligned"]["setupComplete"] == true;
        let mut poses = div().v_flex().gap_3();
        for pose in Pose::ALL {
            poses = poses.child(
                div()
                    .h_flex()
                    .gap_2()
                    .items_center()
                    .child(div().flex_1().min_w_0().child(self.stay_aligned_switch(
                        &format!("{}Enabled", pose.name()),
                        &format!("settings-stay_aligned-relaxed_poses-{}", pose.name()),
                        !ready || !pose.has_saved(&self.draft.value["stay_aligned"]),
                        cx,
                    )))
                    .child(
                        Button::new(format!("stay-pose-{}", pose.name()))
                            .primary()
                            .w(px(128.))
                            .flex_shrink_0()
                            .label(self.text("settings-stay_aligned-relaxed_poses-save_pose"))
                            .disabled(
                                !ready
                                    || self.draft.saving.is_some()
                                    || self.snapshot.connection != Connection::Connected,
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.stay_aligned.modal = Some(pose);
                                this.ui_error = None;
                                cx.notify();
                            })),
                    ),
            );
        }
        let general = div()
            .flex()
            .gap_3()
            .when(width < 580., |d| d.flex_col())
            .when(width >= 580., |d| d.flex_row())
            .child(div().flex_1().min_w_0().child(self.stay_aligned_switch(
                "enabled",
                "settings-stay_aligned-enabled-label",
                !ready,
                cx,
            )))
            .child(div().flex_1().min_w_0().child(self.stay_aligned_switch(
                "hideYawCorrection",
                "settings-stay_aligned-hide_yaw_correction-label",
                !ready,
                cx,
            )));
        let pane = div()
            .v_flex()
            .gap_2()
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("settings-stay_aligned")),
            )
            .child(self.paragraph("settings-stay_aligned-description", width, window))
            .child(self.paragraph("settings-stay_aligned-setup-description", width, window))
            .child(
                div().h_flex().child(
                    Button::new("stay-setup")
                        .primary()
                        .label(self.text("settings-stay_aligned-setup-label"))
                        .disabled(
                            self.snapshot.connection != Connection::Connected
                                || self.draft.saving.is_some(),
                        )
                        .on_click(
                            cx.listener(|this, _, _, cx| this.go(Page::StayAlignedSetup, cx)),
                        ),
                ),
            )
            .child(
                div()
                    .mt_4()
                    .text_lg()
                    .font_bold()
                    .child(self.text("settings-stay_aligned-general-label")),
            )
            .child(general)
            .child(
                div()
                    .mt_4()
                    .text_lg()
                    .font_bold()
                    .child(self.text("settings-stay_aligned-relaxed_poses-label")),
            )
            .child(self.paragraph(
                "settings-stay_aligned-relaxed_poses-description",
                width,
                window,
            ))
            .child(poses)
            .child(
                div()
                    .mt_4()
                    .text_lg()
                    .font_bold()
                    .child(self.text("settings-stay_aligned-debug-label")),
            )
            .child(self.paragraph("settings-stay_aligned-debug-description", width, window))
            .child(
                div().h_flex().child(
                    Button::new("stay-copy-settings")
                        .primary()
                        .label(self.text("settings-stay_aligned-debug-copy-label"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                this.stay_aligned_report(),
                            ))
                        })),
                ),
            );
        div()
            .v_flex()
            .gap_2()
            .child(self.settings_pane("", "Wrench", pane.into_any_element(), cx))
            .child(self.settings_save_bar(cx))
            .into_any_element()
    }
    fn stay_aligned_report(&self) -> String {
        let settings = &self.draft.value;
        let config = &settings["stay_aligned"];
        let boolean = |key: &str| config[key] == true;
        let number = |value: &Value| format!("{:.1}", value.as_f64().unwrap_or(0.));
        let mut report = format!(
            "Stay Aligned\n\nGENERAL\n=======\nEnabled: {}\nExtra yaw correction: {}\nSetup complete: {}\n\nRELAXED POSES\n=============\n",
            boolean("enabled"),
            boolean("extraYawCorrection"),
            boolean("setupComplete")
        );
        for pose in Pose::ALL {
            let name = pose.name();
            let label = match pose {
                Pose::Standing => "Standing",
                Pose::Sitting => "Sitting",
                Pose::Flat => "Flat",
            };
            let value = if boolean(&format!("{name}Enabled")) {
                format!(
                    "Enabled (upper_leg={}, lower_leg={}, foot={})",
                    number(&config[format!("{name}UpperLegAngle")]),
                    number(&config[format!("{name}LowerLegAngle")]),
                    number(&config[format!("{name}FootAngle")])
                )
            } else {
                "Disabled".into()
            };
            report.push_str(&format!("{label}: {value}\n"));
        }
        report.push_str("\nTRACKERS\n========\n");
        if let Some(feed) = &self.snapshot.feed {
            for tracker in &feed.trackers {
                if tracker.is_imu
                    && !tracker.computed
                    && let Some(info) = &tracker.stay_aligned
                {
                    report.push_str(&format!("{}: correction={:.1} locked={} locked_error={:.1} center_error={:.1} neighbor_error={:.1}\n",BodyPart(tracker.body).variant_name().unwrap_or("NONE"),info.correction,info.locked,info.locked_error,info.center_error,info.neighbor_error));
                }
            }
        }
        report.push_str(&format!("\nOTHER\n=====\nFiltering: type={} amount={}\nEnforce constraints: {}\nSkating correction: {}\n",settings["filtering"]["type"],number(&settings["filtering"]["amount"]),settings["model_settings"]["toggles"]["enforce_constraints"]==true,settings["model_settings"]["toggles"]["skating_correction"]==true));
        report
    }
}
