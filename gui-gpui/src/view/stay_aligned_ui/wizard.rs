use super::*;

impl SlimeView {
    fn stay_aligned_previous(&self, cx: &mut Context<Self>) -> Button {
        Button::new("stay-previous")
            .label(self.text("onboarding-stay_aligned-previous_step"))
            .disabled(self.stay_aligned.wizard.busy())
            .on_click(cx.listener(|this, _, _, cx| {
                let old = this.stay_aligned.wizard.step;
                this.stay_aligned.wizard.previous();
                this.stay_aligned.retry = None;
                this.stay_aligned_step_changed(old, cx);
                cx.notify();
            }))
    }
    fn stay_aligned_step_body(
        &self,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = self.stay_aligned.wizard.step;
        let busy =
            self.stay_aligned.wizard.busy() || self.snapshot.connection != Connection::Connected;
        let mut body = div().v_flex().gap_3().py_2();
        if self.stay_aligned.wizard.failed {
            body = body.child(
                div()
                    .v_flex()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(cx.theme().danger.opacity(0.1))
                    .child(self.paragraph("native-stay-aligned-save-error", width - 24., window))
                    .when_some(self.stay_aligned.retry, |d, action| {
                        d.child(
                            Button::new("stay-retry")
                                .label(self.text("native-stay-aligned-retry"))
                                .disabled(self.snapshot.connection != Connection::Connected)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.stay_aligned_action(action, cx)
                                })),
                        )
                    }),
            );
        }
        match step {
            Step::PutTrackersOn => {
                let count = self
                    .snapshot
                    .feed
                    .as_ref()
                    .map(|f| {
                        f.trackers
                            .iter()
                            .filter(|t| !t.computed && t.body != 0 && matches!(t.status, 2 | 3))
                            .count()
                    })
                    .unwrap_or(0);
                body = body
                    .child(self.paragraph(
                        "onboarding-stay_aligned-put_trackers_on-description",
                        width,
                        window,
                    ))
                    .child(self.tip("tips-find_tracker", width, window, cx))
                    .when(count < 5, |d| {
                        d.child(
                            div()
                                .p_3()
                                .rounded_lg()
                                .bg(cx.theme().warning.opacity(0.15))
                                .text_color(cx.theme().warning)
                                .child(self.paragraph(
                                    "onboarding-stay_aligned-put_trackers_on-trackers_warning",
                                    width - 24.,
                                    window,
                                )),
                        )
                    })
                    .child(self.assigned_trackers_figure(width, 500., 500., cx))
                    .child(
                        div().h_flex().justify_end().child(
                            Button::new("stay-put-on-next")
                                .primary()
                                .label(self.text("onboarding-stay_aligned-put_trackers_on-next"))
                                .disabled(busy || count < 5 || self.stay_aligned.wizard.failed)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    let old = this.stay_aligned.wizard.step;
                                    this.stay_aligned.wizard.advance();
                                    this.stay_aligned_step_changed(old, cx);
                                    cx.notify();
                                })),
                        ),
                    );
            }
            Step::Preparation => {
                for i in 0..3 {
                    body = body.child(self.paragraph(
                        &format!("onboarding-automatic_mounting-preparation-v2-step-{i}"),
                        width,
                        window,
                    ));
                }
                body = body.child(self.tip(
                    "onboarding-stay_aligned-preparation-tip",
                    width,
                    window,
                    cx,
                ));
                let mut poses = div().h_flex().gap_2();
                for (name, correct) in [
                    ("FullResetPose", true),
                    ("FullResetPoseSide", true),
                    ("FullResetPoseWrong", false),
                ] {
                    let w = (width - 16.) / 3.;
                    poses = poses.child(
                        div()
                            .relative()
                            .rounded_lg()
                            .bg(cx.theme().popover)
                            .w(px(w))
                            .child(
                                crate::ui_assets::image(&format!("slime/reset/{name}.webp"), cx)
                                    .w(px(w))
                                    .h(px(w * 1920. / 1080.))
                                    .object_fit(ObjectFit::Contain),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top_2()
                                    .right_2()
                                    .text_2xl()
                                    .text_color(if correct {
                                        cx.theme().success
                                    } else {
                                        cx.theme().danger
                                    })
                                    .child(if correct { "✓" } else { "×" }),
                            ),
                    );
                }
                body = body.child(poses).child(
                    div()
                        .h_flex()
                        .justify_between()
                        .gap_2()
                        .child(self.stay_aligned_previous(cx))
                        .child(
                            self.reset_button("stay-full-reset", ResetKind::Full, &[], false, cx)
                                .disabled(busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.stay_aligned_action(Action::FullReset, cx)
                                })),
                        ),
                );
            }
            Step::VerifyMounting => {
                for i in 0..2 {
                    body = body.child(self.paragraph(
                        &format!("onboarding-automatic_mounting-mounting_reset-step-{i}"),
                        width,
                        window,
                    ));
                }
                body = body
                    .child(
                        crate::ui_assets::image("slime/mounting-reset-pose.webp", cx)
                            .w(px(width.min(600.)))
                            .h(px(width.min(600.) * 3725. / 3463.))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .flex_wrap()
                            .justify_between()
                            .child(self.stay_aligned_previous(cx))
                            .child(
                                Button::new("stay-manual-mounting")
                                    .label(
                                        self.text("onboarding-automatic_mounting-manual_mounting"),
                                    )
                                    .disabled(busy)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.stay_aligned.manual = true;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                self.reset_button(
                                    "stay-mounting-reset",
                                    ResetKind::Mounting,
                                    &[],
                                    false,
                                    cx,
                                )
                                .disabled(busy)
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.stay_aligned_action(Action::MountingReset, cx)
                                    },
                                )),
                            ),
                    );
            }
            Step::Standing | Step::Sitting | Step::Flat => {
                let pose = step.pose().unwrap();
                body = body
                    .child(self.stay_aligned_pose_content(pose, width, window, false, cx))
                    .child(
                        div()
                            .h_flex()
                            .justify_between()
                            .gap_2()
                            .flex_wrap()
                            .child(self.stay_aligned_previous(cx))
                            .child(
                                div()
                                    .h_flex()
                                    .gap_2()
                                    .child(
                                        Button::new("stay-skip-pose")
                                            .label(self.text(
                                                "onboarding-stay_aligned-relaxed_poses-skip_step",
                                            ))
                                            .disabled(busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.stay_aligned_action(Action::Clear(pose), cx)
                                            })),
                                    )
                                    .child(
                                        Button::new("stay-save-pose")
                                            .primary()
                                            .label(self.text(
                                                "settings-stay_aligned-relaxed_poses-save_pose",
                                            ))
                                            .disabled(busy)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.stay_aligned_action(Action::Detect(pose), cx)
                                            })),
                                    ),
                            ),
                    );
            }
            Step::Done => {
                body = body
                    .py_8()
                    .child(
                        div()
                            .text_xl()
                            .font_bold()
                            .child(self.text("onboarding-stay_aligned-done-description")),
                    )
                    .child(self.paragraph(
                        "onboarding-stay_aligned-done-description-2",
                        width,
                        window,
                    ))
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .justify_center()
                            .child(
                                Button::new("stay-restart")
                                    .label(self.text("onboarding-stay_aligned-restart"))
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.stay_aligned_start(cx)),
                                    ),
                            )
                            .child(
                                Button::new("stay-done")
                                    .primary()
                                    .label(self.text("onboarding-stay_aligned-done"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.go(Page::Settings(Section::StayAligned), cx)
                                    })),
                            ),
                    );
            }
        }
        body.into_any_element()
    }
    pub(in crate::view) fn stay_aligned_setup(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width) - 128.).max(300.);
        let height = (f32::from(window.viewport_size().height) - 52.).max(300.);
        let mobile = width < 700.;
        let preview_width = if mobile {
            width
        } else {
            (width * 0.38).min(512.)
        };
        let left_width = if mobile {
            width
        } else {
            width - preview_width - 8.
        };
        let content_width = (left_width - 72.).max(180.);
        let mut left = div()
            .id("stay-wizard-steps")
            .v_flex()
            .gap_3()
            .w_full()
            .h_full()
            .overflow_y_scroll()
            .track_scroll(&self.stay_aligned.scroll)
            .p_4()
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("onboarding-stay_aligned-title")),
                    )
                    .child(
                        Button::new("stay-exit")
                            .ghost()
                            .label("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.go(Page::Settings(Section::StayAligned), cx)
                            })),
                    ),
            )
            .child(self.paragraph(
                "onboarding-stay_aligned-description",
                left_width - 32.,
                window,
            ));
        let mut steps = div().v_flex().ml_4().mt_2();
        for (index, step) in Step::ALL.into_iter().enumerate() {
            let active = step == self.stay_aligned.wizard.step;
            let done = index < (self.stay_aligned.wizard.step as usize);
            let mut row = div()
                .relative()
                .v_flex()
                .gap_2()
                .pl_6()
                .pb_10()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(1.))
                        .bg(if done {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .left(px(-16.))
                        .top_0()
                        .size(px(32.))
                        .rounded_full()
                        .bg(if active || done {
                            cx.theme().primary
                        } else {
                            cx.theme().popover
                        })
                        .text_color(if active || done {
                            cx.theme().primary_foreground
                        } else {
                            cx.theme().foreground
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if done {
                            "✓".to_owned()
                        } else {
                            (index + 1).to_string()
                        }),
                )
                .child(
                    div()
                        .min_h(px(28.))
                        .font_bold()
                        .text_color(if active {
                            cx.theme().foreground
                        } else {
                            cx.theme().muted_foreground
                        })
                        .child(self.text(step.title())),
                );
            if active {
                row = row.child(self.stay_aligned_step_body(content_width, window, cx));
            }
            steps = steps.child(row);
        }
        left = left.child(steps);
        let dual = matches!(self.stay_aligned.wizard.step, Step::Sitting | Step::Flat);
        let preview_height = if mobile { height * 0.3 } else { height };
        let mut views = div()
            .flex()
            .size_full()
            .rounded_lg()
            .overflow_hidden()
            .bg(cx.theme().muted)
            .when(mobile, |d| d.flex_row())
            .when(!mobile, |d| d.flex_col());
        if dual {
            views = views.child(div().flex_1().min_w_0().child(self.skeleton_scene_for(
                if mobile {
                    preview_height
                } else {
                    preview_height / 2.
                },
                false,
                Camera {
                    yaw: 0.,
                    pitch: std::f32::consts::FRAC_PI_2,
                    ..Default::default()
                },
                "stay-camera-top",
                cx,
            )));
        }
        views = views.child(div().flex_1().min_w_0().child(self.skeleton_scene_for(
            if mobile || !dual {
                preview_height
            } else {
                preview_height / 2.
            },
            false,
            self.stay_aligned_camera(),
            "stay-camera-main",
            cx,
        )));
        div()
            .flex()
            .size_full()
            .min_h_0()
            .gap_2()
            .when(mobile, |d| d.flex_col())
            .when(!mobile, |d| d.flex_row())
            .child(
                div()
                    .w(px(left_width))
                    .h(px(if mobile { height * 0.7 - 8. } else { height }))
                    .child(left),
            )
            .child(
                div()
                    .w(px(preview_width))
                    .h(px(preview_height))
                    .child(views),
            )
            .into_any_element()
    }
    pub(in crate::view) fn stay_aligned_manual(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("stay-manual-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.9))
            .p_6()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.stay_aligned.manual = false;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("stay-manual-dialog")
                    .relative()
                    .size_full()
                    .overflow_y_scroll()
                    .p_4()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(self.mounting_aligned(window, cx))
                    .child(
                        Button::new("stay-manual-close")
                            .ghost()
                            .label("×")
                            .absolute()
                            .top_2()
                            .right_2()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stay_aligned.manual = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }
}
