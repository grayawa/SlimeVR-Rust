use super::*;
use serde_json::json;
use slimevr_gpui::{proportions, rpc_generated};
#[derive(Clone, Copy)]
pub(super) enum HeightAction {
    Manual(f64),
    Automatic,
}

impl SlimeView {
    pub(super) fn open_proportions(&mut self, view: &str, cx: &mut Context<Self>) {
        self.form_values
            .insert("proportions-view".into(), view.into());
        cx.notify();
    }
    fn apply_height(&mut self, eye_height: f64, cx: &mut Context<Self>) {
        match proportions::scale(&self.read("SkeletonConfigResponse"), eye_height) {
            Ok(r) => {
                self.batch(r, cx);
                if self.ui_error.is_none() {
                    self.preference("lastUsedProportions", json!("scaled"), cx);
                }
            }
            Err(e) => self.ui_error = Some(e),
        };
        cx.notify();
    }
    fn change_height(&mut self, eye_height: f64, cx: &mut Context<Self>) {
        if self.preferences.value["lastUsedProportions"]
            .as_str()
            .is_some_and(|v| v != "scaled")
        {
            self.pending_height = Some(HeightAction::Manual(eye_height));
            cx.notify();
        } else {
            self.apply_height(eye_height, cx);
        }
    }
    pub(super) fn height_warning(
        &self,
        action: HeightAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("height-reset-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.9))
            .v_flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .v_flex()
                    .w(px((f32::from(window.viewport_size().width) - 40.).min(540.)))
                    .gap_4()
                    .p_6()
                    .rounded_lg()
                    .bg(cx.theme().popover)
                    .child(self.paragraph("onboarding-user_height-reset-warning", 500., window))
                    .child(
                        div()
                            .h_flex()
                            .justify_end()
                            .gap_3()
                            .child(
                                Button::new("height-reset-cancel")
                                    .ghost()
                                    .label(self.text("native-cancel"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.pending_height = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("height-reset-confirm")
                                    .primary()
                                    .label(self.text("native-apply"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.pending_height = None;
                                        match action {
                                            HeightAction::Manual(height) => {
                                                this.apply_height(height, cx)
                                            }
                                            HeightAction::Automatic => this.rpc(
                                                "StartUserHeightCalibration",
                                                json!({}),
                                                cx,
                                            ),
                                        };
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn proportions_aligned(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width)
            - if self.in_onboarding() { 32. } else { 150. })
        .max(300.);
        let height = (f32::from(window.viewport_size().height) - 90.).max(450.);
        let view = self.form_value("proportions-view", "preview");
        if view != "preview" {
            let back = Button::new("proportions-back")
                .ghost()
                .label(self.text("onboarding-manual_proportions-back-scaled"))
                .on_click(cx.listener(|this, _, _, cx| this.open_proportions("preview", cx)));
            if view == "automatic" {
                return div()
                    .v_flex()
                    .gap_4()
                    .child(div().h_flex().child(back))
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("onboarding-automatic_proportions-title")),
                    )
                    .child(self.paragraph(
                        "onboarding-automatic_proportions-description",
                        width.min(768.),
                        window,
                    ))
                    .child(self.calibration(cx))
                    .into_any_element();
            }
            let manual = self.proportions(window, cx);
            return div()
                .v_flex()
                .gap_3()
                .child(div().h_flex().child(back))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .when(width < 900., |d| d.flex_col())
                        .when(width >= 900., |d| d.flex_row())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .p_2()
                                .rounded_lg()
                                .bg(cx.theme().popover)
                                .child(manual),
                        )
                        .when(width >= 900., |d| {
                            d.child(
                                div()
                                    .w(px(width * 0.32))
                                    .flex_shrink_0()
                                    .child(self.skeleton_scene(height - 120., false, cx)),
                            )
                        }),
                )
                .into_any_element();
        }
        let controls_width = width.min(672.);
        let eye = self.read("SettingsResponse")["model_settings"]["skeleton_height"]["hmd_height"]
            .as_f64()
            .unwrap_or(1.65 * 0.936);
        let imperial = self.form_value("height-unit", "meter") == "foot";
        let total_inches = (eye / 0.936 / 0.0254).round() as i64;
        let display = if imperial {
            format!("{}′ {}″", total_inches / 12, total_inches % 12)
        } else {
            format!("{:.2} {}", eye / 0.936, self.text("unit-meter"))
        };
        let mut increments = div().h_flex().gap_2().items_center();
        for (index, delta) in [-10f64, -1., 0., 1., 10.].into_iter().enumerate() {
            if delta == 0. {
                increments = increments.child(
                    div()
                        .h_flex()
                        .flex_1()
                        .min_w_0()
                        .h(px(75.))
                        .items_center()
                        .justify_center()
                        .gap_4()
                        .rounded_lg()
                        .bg(cx.theme().border)
                        .child(div().text_2xl().font_bold().child(display.clone()))
                        .child(
                            div().v_flex().gap_1().children(
                                [("meter", "unit-meter"), ("foot", "unit-foot")]
                                    .into_iter()
                                    .map(|(unit, label)| {
                                        Button::new(format!("height-unit-{unit}"))
                                            .small()
                                            .selected(imperial == (unit == "foot"))
                                            .label(self.text(label))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.form_values
                                                    .insert("height-unit".into(), unit.into());
                                                cx.notify();
                                            }))
                                    }),
                            ),
                        ),
                );
            } else {
                let delta = if imperial {
                    if delta.abs() == 10. {
                        delta.signum() * 12.
                    } else {
                        delta
                    }
                } else {
                    delta
                };
                let next =
                    ((eye / 0.936 + delta * if imperial { 0.0254 } else { 0.01 }) * 0.936 * 10000.)
                        .round()
                        / 10000.;
                increments = increments.child(
                    Button::new(format!("height-increment-{index}"))
                        .ghost()
                        .h(px(75.))
                        .w(px(if width < 620. { 42. } else { 75. }))
                        .flex_shrink_0()
                        .bg(cx.theme().border)
                        .disabled(
                            !(1.2..=1.936).contains(&next)
                                || self.snapshot.connection != Connection::Connected
                                || self.snapshot.pending.is_some(),
                        )
                        .child(
                            div()
                                .v_flex()
                                .items_center()
                                .gap_1()
                                .child(div().text_2xl().font_bold().child(format!("{delta:+.0}")))
                                .child(self.text(if imperial { "unit-inch" } else { "unit-cm" })),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.change_height(next, cx))),
                );
            }
        }
        let can_height = self.snapshot.feed.as_ref().is_some_and(|f| f.can_height);
        let status = self.read("UserHeightRecordingStatusResponse");
        let status_code = status["status"].as_u64().unwrap_or(0);
        let busy = (1..=5).contains(&status_code);
        let mut controls = div()
            .v_flex()
            .gap_3()
            .p_4()
            .w_full()
            .rounded_lg()
            .bg(cx.theme().popover)
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("onboarding-user_height-title")),
            )
            .child(increments)
            .child(
                Button::new("height-auto")
                    .primary()
                    .w_full()
                    .label(self.text(if busy {
                        "native-height-cancel"
                    } else {
                        "onboarding-user_height-calculate"
                    }))
                    .disabled(
                        !busy && (!can_height || self.snapshot.connection != Connection::Connected),
                    )
                    .when(!can_height, |b| {
                        b.tooltip(self.text("onboarding-user_height-need_head_tracker"))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !busy
                            && this.preferences.value["lastUsedProportions"]
                                .as_str()
                                .is_some_and(|v| v != "scaled")
                        {
                            this.pending_height = Some(HeightAction::Automatic);
                            cx.notify();
                        } else {
                            this.rpc(
                                if busy {
                                    "CancelUserHeightCalibration"
                                } else {
                                    "StartUserHeightCalibration"
                                },
                                json!({}),
                                cx,
                            );
                        }
                    })),
            )
            .when(!self.in_onboarding(), |d| {
                d.child(
                    div()
                        .h_flex()
                        .justify_between()
                        .gap_3()
                        .flex_wrap()
                        .child(
                            Button::new("proportions-manual")
                                .label(self.text("onboarding-user_height-manual-proportions"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.open_proportions("manual", cx)
                                })),
                        )
                        .child(
                            Button::new("proportions-full-reset")
                                .label(self.text("reset-full"))
                                .disabled(
                                    self.snapshot.connection != Connection::Connected
                                        || self.snapshot.reset.as_ref().is_some_and(|r| !r.done),
                                )
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.send(Command::Reset(ResetKind::Full), cx)
                                })),
                        ),
                )
            })
            .when(self.in_onboarding(), |d| {
                d.child(
                    div().h_flex().child(
                        Button::new("setup-height-next")
                            .primary()
                            .label(self.text("onboarding-user_height-next_step"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.onboarding_to(slimevr_gpui::onboarding::Step::Usage, cx)
                            })),
                    ),
                )
            });
        if status_code != 0 {
            let name = rpc_generated::enum_choices("UserHeightCalibrationStatus")
                .iter()
                .find(|(_, n)| *n == status_code)
                .map(|(n, _)| *n)
                .unwrap_or("NONE");
            controls = controls.child(self.paragraph(
                &format!("onboarding-user_height-calibration-{name}"),
                controls_width,
                window,
            ));
        }
        div()
            .relative()
            .h(px(height))
            .w_full()
            .child(self.skeleton_scene(height, false, cx))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px((width - controls_width) / 2.))
                    .w(px(controls_width))
                    .child(self.tip(
                        "onboarding-user_height-manual-tip",
                        controls_width,
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .absolute()
                    .bottom_0()
                    .left(px((width - controls_width) / 2.))
                    .w(px(controls_width))
                    .child(controls),
            )
            .into_any_element()
    }
}

impl SlimeView {
    pub(super) fn proportion_increment(
        &self,
        bone: u64,
        group: Option<&'static [u64]>,
        shown: f64,
        delta: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        Button::new(format!("bone-step-{bone}-{delta}"))
            .small()
            .label(format!("{delta:+}"))
            .disabled(
                self.snapshot.connection != Connection::Connected
                    || self.snapshot.pending.is_some(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let skeleton = this.read("SkeletonConfigResponse");
                let result = if let Some(group) = group {
                    proportions::ratio(&skeleton, group, bone, (shown + delta) / 100.)
                } else {
                    proportions::change(bone, (shown + delta) / 100.)
                        .map(|r| vec![r, ("SkeletonConfigRequest".into(), json!({}))])
                };
                match result {
                    Ok(r) => {
                        this.batch(r, cx);
                        if this.ui_error.is_none() {
                            this.preference("lastUsedProportions", json!("manual"), cx);
                        }
                    }
                    Err(e) => this.ui_error = Some(e),
                };
                cx.notify();
            }))
            .into_any_element()
    }
}

impl SlimeView {
    pub(super) fn proportion_group_increment(
        &self,
        name: &'static str,
        ids: &'static [u64],
        total: f64,
        delta: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let precise = self.form_value("proportions-precise", "false") == "true";
        let delta = if precise {
            if delta.abs() == 5. {
                delta.signum()
            } else {
                delta * 0.5
            }
        } else {
            delta
        };
        Button::new(format!("group-step-{name}-{delta}"))
            .small()
            .label(format!("{delta:+}"))
            .disabled(
                self.snapshot.connection != Connection::Connected
                    || self.snapshot.pending.is_some(),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                match proportions::resize_group(
                    &this.read("SkeletonConfigResponse"),
                    ids,
                    total + delta / 100.,
                ) {
                    Ok(r) => this.batch(r, cx),
                    Err(e) => this.ui_error = Some(e),
                };
                cx.notify();
            }))
            .into_any_element()
    }
}
