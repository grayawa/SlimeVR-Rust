use super::*;
use serde_json::json;
use slimevr_gpui::autobone::{Phase, Step};

impl SlimeView {
    pub(super) fn autobone_scroll_to_top(&self) {
        if let Some(scroll) = self.scrolls.get(&Page::Proportions) {
            scroll.set_offset(point(px(0.), px(0.)));
        }
    }
    pub(super) fn dismiss_autobone_error(&mut self, cx: &mut Context<Self>) {
        self.autobone = Default::default();
        self.ui_error = None;
        let _ = self.client.rpc("SkeletonConfigRequest", json!({}));
        self.autobone_scroll_to_top();
        cx.notify();
    }
    pub(super) fn leave_autobone(&mut self) {
        if self.autobone.phase == Phase::Recording {
            let _ = self.client.rpc("AutoBoneCancelRecordingRequest", json!({}));
        }
        self.autobone = Default::default();
    }
    fn autobone_record(&mut self, cx: &mut Context<Self>) {
        match self
            .client
            .rpc("AutoBoneProcessRequest", json!({"process_type":1}))
        {
            Ok(_) => {
                self.autobone.record();
                self.ui_error = None;
            }
            Err(error) => self.ui_error = Some(error),
        }
        self.autobone_scroll_to_top();
        cx.notify();
    }
    fn autobone_previous(&self, cx: &mut Context<Self>) -> Button {
        Button::new("autobone-previous")
            .ghost()
            .label(self.text("onboarding-automatic_proportions-prev_step"))
            .disabled(self.autobone.busy())
            .on_click(cx.listener(|this, _, _, cx| {
                this.autobone.previous();
                this.autobone_scroll_to_top();
                cx.notify();
            }))
    }
    fn autobone_next(&self, label: &str, cx: &mut Context<Self>) -> Button {
        Button::new("autobone-next")
            .primary()
            .label(self.text(label))
            .on_click(cx.listener(|this, _, _, cx| {
                this.autobone.advance();
                this.autobone_scroll_to_top();
                cx.notify();
            }))
    }
    fn autobone_title(&self, id: &str) -> AnyElement {
        div()
            .text_2xl()
            .font_bold()
            .child(self.text(id))
            .into_any_element()
    }
    fn autobone_list(&self, id: &str, width: f32, window: &Window, numbered: bool) -> AnyElement {
        div()
            .v_flex()
            .gap_2()
            .children(self.text(id).lines().enumerate().map(|(i, line)| {
                div()
                    .h_flex()
                    .items_start()
                    .gap_2()
                    .child(div().flex_shrink_0().child(if numbered {
                        format!("{}.", i + 1)
                    } else {
                        "•".into()
                    }))
                    .child(
                        div().v_flex().min_w_0().children(
                            self.settings_text_lines(line.to_owned(), width - 24., window)
                                .into_iter()
                                .map(|line| div().child(line)),
                        ),
                    )
            }))
            .into_any_element()
    }
    fn autobone_video(&self, cx: &mut Context<Self>) -> AnyElement {
        let _ = crate::ui_assets::image("slime/autobone-motion.webp", cx);
        let playing = self.autobone.demo_started.is_some();
        let mut demonstration = if playing {
            crate::ui_assets::image_frame_with_fallback(
                "slime/autobone-motion.webp",
                self.autobone
                    .demo_started
                    .map(|start| start.elapsed().as_millis() as usize / 125)
                    .unwrap_or(0),
                "slime/autobone-poster.webp",
                cx,
            )
            .id("autobone-motion")
        } else {
            crate::ui_assets::image("slime/autobone-poster.webp", cx).id("autobone-poster")
        }
        .w(px(192.))
        .h(px(360.))
        .object_fit(ObjectFit::Contain);
        demonstration.style().aspect_ratio = Some(192. / 360.);
        div()
            .id("autobone-video")
            .relative()
            .w(px(192.))
            .h(px(360.))
            .flex_shrink_0()
            .cursor_pointer()
            .child(demonstration)
            .when(!playing, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .h_flex()
                        .items_center()
                        .justify_center()
                        .child(div().text_3xl().child("▶")),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.autobone.demo_started = if playing {
                    None
                } else {
                    Some(std::time::Instant::now())
                };
                cx.notify();
            }))
            .into_any_element()
    }
    fn autobone_body(&self, width: f32, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let image_height = height.min(450.).min(width * 392. / 163.);
        let image_width = image_height * 163. / 392.;
        let image_x = (width - image_width) / 2.;
        let image_y = (height - image_height) / 2.;
        let mut body = div()
            .relative()
            .w(px(width))
            .h(px(height))
            .flex_shrink_0()
            .child(
                crate::ui_assets::image("slime/assignment-pose.webp", cx)
                    .id("autobone-character")
                    .absolute()
                    .left(px(image_x))
                    .top(px(image_y))
                    .w(px(image_width))
                    .h(px(image_height))
                    .object_fit(ObjectFit::Fill),
            );
        for target in
            slimevr_gpui::assignment::targets("all", self.preferences.value["mirrorView"] != false)
        {
            let trackers: Vec<_> = self
                .snapshot
                .feed
                .as_ref()
                .into_iter()
                .flat_map(|feed| &feed.trackers)
                .filter(|tracker| !tracker.computed && tracker.body == target.body)
                .collect();
            if trackers.is_empty() {
                continue;
            }
            let level = trackers
                .iter()
                .map(|tracker| self.tracker_highlight(tracker))
                .fold(0., f32::max);
            let dot = div()
                .id(format!("autobone-body-{}", target.body))
                .absolute()
                .left(px(image_x + target.x / 163. * image_width - 7.5))
                .top(px(image_y + target.y / 392. * image_height - 7.5))
                .size(px(15.))
                .rounded_full()
                .bg(cx.theme().foreground)
                .border(px(2. + level * 2.))
                .border_color(cx.theme().link);
            body = body.child(self.motion_glow(dot, level, 12., cx));
        }
        body.into_any_element()
    }
    pub(super) fn automatic_proportions(
        &self,
        width: f32,
        height: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page_width = width.min(768.);
        let compact = page_width < 650.;
        let step = self.autobone.step;
        let content_width = (page_width - if compact { 32. } else { 112. }).max(240.);
        let card_height = (height - 104.).max(420.);
        let text_width = if compact {
            content_width
        } else {
            (content_width - 208.).max(240.)
        };
        let connected = self.snapshot.connection == Connection::Connected;
        let navigation = |previous, next| {
            div()
                .h_flex()
                .gap_3()
                .flex_wrap()
                .child(previous)
                .child(next)
        };
        let content = match step {
            Step::PutTrackersOn => {
                let body_width = if compact {
                    content_width
                } else {
                    (content_width - 316.).max(190.)
                };
                let text_width = if compact { content_width } else { 300. };
                let info =
                    div()
                        .v_flex()
                        .w(px(text_width))
                        .gap_4()
                        .child(self.autobone_title(
                            "onboarding-automatic_proportions-put_trackers_on-title",
                        ))
                        .child(self.paragraph(
                            "onboarding-automatic_proportions-put_trackers_on-description",
                            text_width,
                            window,
                        ))
                        .child(self.tip("tips-find_tracker", text_width, window, cx));
                let controls = navigation(
                    Button::new("autobone-back-manual")
                        .ghost()
                        .label(self.text("onboarding-automatic_proportions-back"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.open_proportions("manual", cx)),
                        ),
                    self.autobone_next("onboarding-automatic_proportions-put_trackers_on-next", cx),
                );
                let body = self.autobone_body(
                    body_width,
                    if compact { 380. } else { card_height - 64. },
                    cx,
                );
                if compact {
                    div()
                        .v_flex()
                        .gap_4()
                        .child(info)
                        .child(body)
                        .child(controls)
                        .into_any_element()
                } else {
                    div()
                        .h_flex()
                        .gap_4()
                        .items_start()
                        .child(
                            div()
                                .v_flex()
                                .w(px(text_width))
                                .h(px(card_height - 64.))
                                .justify_between()
                                .child(info)
                                .child(controls),
                        )
                        .child(body)
                        .into_any_element()
                }
            }
            Step::Requirements => div()
                .v_flex()
                .gap_4()
                .min_h(px(card_height - if compact { 32. } else { 64. }))
                .justify_between()
                .child(
                    div()
                        .v_flex()
                        .gap_4()
                        .child(
                            self.autobone_title(
                                "onboarding-automatic_proportions-requirements-title",
                            ),
                        )
                        .child(self.autobone_list(
                            "onboarding-automatic_proportions-requirements-descriptionv2",
                            content_width,
                            window,
                            false,
                        )),
                )
                .child(navigation(
                    self.autobone_previous(cx),
                    self.autobone_next("onboarding-automatic_proportions-requirements-next", cx),
                ))
                .into_any_element(),
            Step::Preparation => {
                let mut preparation = div()
                    .v_flex()
                    .gap_4()
                    .child(self.autobone_title("onboarding-automatic_mounting-preparation-title"));
                for index in 0..3 {
                    preparation = preparation.child(self.paragraph(
                        &format!("onboarding-automatic_mounting-preparation-v2-step-{index}"),
                        content_width,
                        window,
                    ));
                }
                let mut poses = div().h_flex().gap_2();
                let pose_height = if compact { 160. } else { 230. };
                let pose_width = (content_width - 16.) / 3.;
                for (path, correct) in [
                    ("FullResetPose", true),
                    ("FullResetPoseSide", true),
                    ("FullResetPoseWrong", false),
                ] {
                    let mut pose_image =
                        crate::ui_assets::image(&format!("slime/reset/{path}.webp"), cx)
                            .w(px(pose_width))
                            .h(px(pose_height))
                            .object_fit(ObjectFit::Contain);
                    pose_image.style().aspect_ratio = Some(pose_width / pose_height);
                    poses = poses.child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h(px(pose_height))
                            .overflow_hidden()
                            .rounded_lg()
                            .bg(cx.theme().background)
                            .child(pose_image)
                            .child(
                                div()
                                    .absolute()
                                    .top_2()
                                    .right_2()
                                    .text_2xl()
                                    .font_bold()
                                    .text_color(if correct {
                                        rgb(0x50e897)
                                    } else {
                                        rgb(0xff4b63)
                                    })
                                    .child(if correct { "✓" } else { "×" }),
                            ),
                    );
                }
                let waiting = self.autobone.phase == Phase::Resetting;
                preparation
                    .child(poses)
                    .child(navigation(
                        self.autobone_previous(cx),
                        self.reset_button("autobone-full-reset", ResetKind::Full, &[], false, cx)
                            .disabled(!connected || waiting || self.snapshot.pending.is_some())
                            .on_click(cx.listener(|this, _, _, cx| {
                                match this.client.send(Command::Reset(ResetKind::Full)) {
                                    Ok(tx) => {
                                        this.autobone.wait_for_reset(this.snapshot.session, tx);
                                        this.ui_error = None;
                                    }
                                    Err(error) => this.ui_error = Some(error),
                                }
                                cx.notify();
                            })),
                    ))
                    .into_any_element()
            }
            Step::StartRecording => div()
                .v_flex()
                .gap_4()
                .min_h(px(card_height - if compact { 32. } else { 64. }))
                .justify_between()
                .child(
                    div()
                        .flex()
                        .gap_4()
                        .when(compact, |d| d.flex_col())
                        .when(!compact, |d| d.flex_row())
                        .child(
                            div()
                                .v_flex()
                                .gap_4()
                                .flex_1()
                                .min_w_0()
                                .child(self.autobone_title(
                                    "onboarding-automatic_proportions-start_recording-title",
                                ))
                                .child(self.paragraph(
                                    "onboarding-automatic_proportions-start_recording-description",
                                    text_width,
                                    window,
                                ))
                                .child(self.autobone_list(
                                    "onboarding-automatic_proportions-recording-steps",
                                    text_width,
                                    window,
                                    true,
                                ))
                                .child(self.tip("tips-do_not_move_heels", text_width, window, cx)),
                        )
                        .child(self.autobone_video(cx)),
                )
                .child(navigation(
                    self.autobone_previous(cx),
                    Button::new("autobone-start")
                        .primary()
                        .label(self.text("onboarding-automatic_proportions-start_recording-next"))
                        .disabled(!connected || self.snapshot.pending.is_some())
                        .on_click(cx.listener(|this, _, _, cx| this.autobone_record(cx))),
                ))
                .into_any_element(),
            Step::Recording => {
                let processing = self.autobone.phase == Phase::Processing;
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("time", self.autobone.eta.max(0.).round());
                let timer = if processing {
                    self.text("onboarding-automatic_proportions-recording-processing")
                } else {
                    self.l10n
                        .format("onboarding-automatic_proportions-recording-timer", &args)
                };
                div()
                    .flex()
                    .gap_4()
                    .when(compact, |d| d.flex_col())
                    .when(!compact, |d| d.flex_row())
                    .child(
                        div()
                            .v_flex()
                            .items_center()
                            .gap_4()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .text_color(rgb(0xff4b63))
                                    .child(div().size(px(8.)).rounded_full().bg(rgb(0xff4b63)))
                                    .child(
                                        self.text(
                                            "onboarding-automatic_proportions-recording-title",
                                        ),
                                    ),
                            )
                            .child(self.autobone_title(if processing {
                                "onboarding-automatic_proportions-recording-processing"
                            } else {
                                "onboarding-automatic_proportions-recording-description-p0"
                            }))
                            .child(self.paragraph(
                                "onboarding-automatic_proportions-recording-description-p1",
                                text_width,
                                window,
                            ))
                            .child(self.autobone_list(
                                "onboarding-automatic_proportions-recording-steps",
                                text_width,
                                window,
                                true,
                            ))
                            .child(self.tip("tips-do_not_move_heels", text_width, window, cx))
                            .child(
                                div()
                                    .v_flex()
                                    .items_center()
                                    .gap_2()
                                    .w(px(150.))
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(4.))
                                            .rounded_full()
                                            .bg(cx.theme().border)
                                            .child(
                                                div()
                                                    .h_full()
                                                    .rounded_full()
                                                    .bg(cx.theme().primary)
                                                    .w(relative(self.autobone.progress as f32)),
                                            ),
                                    )
                                    .child(timer),
                            ),
                    )
                    .child(self.autobone_video(cx))
                    .into_any_element()
            }
            Step::VerifyResults => {
                let mut result = div()
                    .v_flex()
                    .gap_2()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().background);
                if let Some(parts) = self
                    .autobone
                    .result
                    .as_ref()
                    .and_then(|v| v["adjusted_skeleton_parts"].as_array())
                {
                    for part in parts {
                        let name = slimevr_gpui::rpc_generated::enum_choices("SkeletonBone")
                            .iter()
                            .find(|(_, bone)| Some(*bone) == part["bone"].as_u64())
                            .map(|(name, _)| *name)
                            .unwrap_or("NONE");
                        result = result.child(
                            div()
                                .h_flex()
                                .gap_3()
                                .justify_between()
                                .child(self.text(&format!("skeleton_bone-{name}")))
                                .child(div().font_bold().child(format!(
                                    "{:.2} {}",
                                    part["value"].as_f64().unwrap_or(0.) * 100.,
                                    self.text("unit-cm")
                                ))),
                        );
                    }
                }
                div()
                    .v_flex()
                    .gap_4()
                    .min_h(px(card_height - if compact { 32. } else { 64. }))
                    .justify_between()
                    .child(
                        div()
                            .v_flex()
                            .gap_1()
                            .child(self.autobone_title(
                                "onboarding-automatic_proportions-verify_results-title",
                            ))
                            .child(self.paragraph(
                                "onboarding-automatic_proportions-verify_results-description",
                                content_width.min(384.),
                                window,
                            )),
                    )
                    .child(
                        div().h_flex().justify_center().child(
                            div()
                                .v_flex()
                                .gap_2()
                                .w(px(content_width.min(320.)))
                                .child(div().font_bold().child(self.text(
                                    "onboarding-automatic_proportions-verify_results-results",
                                )))
                                .child(result),
                        ),
                    )
                    .child(navigation(
                        Button::new("autobone-redo")
                            .ghost()
                            .label(
                                self.text("onboarding-automatic_proportions-verify_results-redo"),
                            )
                            .disabled(!connected || self.autobone.busy())
                            .on_click(cx.listener(|this, _, _, cx| this.autobone_record(cx))),
                        Button::new("autobone-apply")
                            .primary()
                            .label(
                                self.text(
                                    "onboarding-automatic_proportions-verify_results-confirm",
                                ),
                            )
                            .disabled(!connected || self.autobone.busy() || self.autobone.failed)
                            .on_click(cx.listener(|this, _, _, cx| {
                                match this.client.batch(vec![
                                    ("AutoBoneApplyRequest".into(), json!({})),
                                    ("SkeletonConfigRequest".into(), json!({})),
                                ]) {
                                    Ok(tx) => {
                                        this.autobone.wait_for_apply(this.snapshot.session, tx);
                                        this.ui_error = None;
                                    }
                                    Err(error) => this.ui_error = Some(error),
                                }
                                cx.notify();
                            })),
                    ))
                    .into_any_element()
            }
            Step::Done => div()
                .v_flex()
                .items_center()
                .gap_4()
                .child(self.autobone_title("onboarding-automatic_proportions-done-title"))
                .child(self.paragraph(
                    "onboarding-automatic_proportions-done-description",
                    content_width,
                    window,
                ))
                .child(div().w_full().child(self.skeleton_scene(
                    (height - 260.).clamp(240., 450.),
                    false,
                    cx,
                )))
                .into_any_element(),
        };
        div()
            .w_full()
            .h_flex()
            .justify_center()
            .child(
                div()
                    .v_flex()
                    .gap_3()
                    .w(px(page_width))
                    .min_h(px(height + 40.))
                    .child(self.autobone_title("onboarding-automatic_proportions-title"))
                    .child(self.paragraph(
                        "onboarding-automatic_proportions-description",
                        page_width.min(512.),
                        window,
                    ))
                    .child(
                        div()
                            .id("autobone-step")
                            .min_h(px(card_height))
                            .h_flex()
                            .items_start()
                            .gap_4()
                            .p(px(if compact { 16. } else { 32. }))
                            .rounded_lg()
                            .bg(cx.theme().popover)
                            .when(step.numbered() && !compact, |d| {
                                d.child(
                                    div()
                                        .size(px(32.))
                                        .flex_shrink_0()
                                        .rounded_full()
                                        .h_flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(cx.theme().primary)
                                        .font_bold()
                                        .child((step.index() + 1).to_string()),
                                )
                            })
                            .child(
                                div()
                                    .v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .gap_3()
                                    .min_h(px(card_height - if compact { 32. } else { 64. }))
                                    .when(
                                        matches!(
                                            step,
                                            Step::Preparation | Step::Recording | Step::Done
                                        ),
                                        |d| d.justify_center(),
                                    )
                                    .when(step.numbered() && compact, |d| {
                                        d.child(
                                            div()
                                                .size(px(32.))
                                                .rounded_full()
                                                .h_flex()
                                                .items_center()
                                                .justify_center()
                                                .bg(cx.theme().primary)
                                                .font_bold()
                                                .child((step.index() + 1).to_string()),
                                        )
                                    })
                                    .child(content),
                            ),
                    )
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .children((0..7).map(|index| {
                                div()
                                    .h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when(index > 0, |d| {
                                        d.child(
                                            div()
                                                .w(px(20.))
                                                .h(px(4.))
                                                .rounded_full()
                                                .bg(cx.theme().border),
                                        )
                                    })
                                    .child(
                                        div()
                                            .size(px(16.))
                                            .rounded_full()
                                            .h_flex()
                                            .items_center()
                                            .justify_center()
                                            .bg(if index <= step.index() {
                                                cx.theme().primary
                                            } else {
                                                cx.theme().border
                                            })
                                            .when(index < step.index(), |d| {
                                                d.child(div().text_xs().child("✓"))
                                            })
                                            .when(index == step.index(), |d| {
                                                d.child(
                                                    div()
                                                        .size(px(8.))
                                                        .rounded_full()
                                                        .bg(cx.theme().foreground),
                                                )
                                            }),
                                    )
                            })),
                    )
                    .when(step == Step::Done, |d| {
                        d.child(
                            div().h_flex().child(
                                Button::new("autobone-return-manual")
                                    .ghost()
                                    .label(self.text("onboarding-automatic_proportions-back"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.open_proportions("manual", cx)
                                    })),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }
    pub(super) fn autobone_error(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        div().id("autobone-error-overlay").absolute().inset_0().bg(cx.theme().background.opacity(0.9)).v_flex().items_center().justify_center()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| { this.dismiss_autobone_error(cx); }))
            .child(div().id("autobone-error-dialog").v_flex().gap_4().p_6().rounded_lg().bg(cx.theme().popover)
                .w(px((f32::from(window.viewport_size().width) - 40.).min(540.)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(self.paragraph("onboarding-automatic_proportions-error_modal-v2", (f32::from(window.viewport_size().width) - 88.).min(492.), window))
                .child(div().h_flex().gap_3().flex_wrap()
                    .child(Button::new("autobone-error-docs").ghost().label(self.text("native-help"))
                        .on_click(|_, _, cx| cx.open_url("https://docs.slimevr.dev/server/body-config.html#common-issues--debugging")))
                    .child(Button::new("autobone-error-discord").ghost().label("Discord")
                        .on_click(|_, _, cx| cx.open_url("https://discord.gg/slimevr"))))
                .child(Button::new("autobone-error-confirm").primary().label(self.text("onboarding-automatic_proportions-error_modal-confirm"))
                    .on_click(cx.listener(|this, _, _, cx| { this.dismiss_autobone_error(cx); }))))
            .into_any_element()
    }
}
