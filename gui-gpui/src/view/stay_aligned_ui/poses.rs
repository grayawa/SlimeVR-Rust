use super::*;

impl SlimeView {
    pub(super) fn stay_aligned_pose_content(
        &self,
        pose: Pose,
        width: f32,
        window: &Window,
        modal: bool,
        cx: &mut App,
    ) -> AnyElement {
        let image_width =
            (width - if modal { 64. } else { 0. })
                .max(100.)
                .min(match (pose, modal) {
                    (Pose::Flat, true) => 560.,
                    (Pose::Flat, false) => 600.,
                    (_, true) => 260.,
                    _ => 300.,
                });
        div()
            .v_flex()
            .gap_2()
            .child(self.paragraph(
                &format!(
                    "onboarding-stay_aligned-relaxed_poses-{}-step-0",
                    pose.name()
                ),
                width,
                window,
            ))
            .child(self.paragraph(
                &format!(
                    "onboarding-stay_aligned-relaxed_poses-{}-step-1-v2",
                    pose.name()
                ),
                width,
                window,
            ))
            .child(
                div().h_flex().justify_center().py_2().child(
                    crate::ui_assets::image(pose.image(), cx)
                        .w(px(image_width))
                        .h(px(image_width
                            * if pose == Pose::Flat {
                                1.
                            } else {
                                1920. / 1080.
                            }))
                        .object_fit(ObjectFit::Contain),
                ),
            )
            .into_any_element()
    }
    pub(in crate::view) fn stay_aligned_pose_modal(
        &self,
        pose: Pose,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width) - 40.).min(576.);
        let busy = self.stay_aligned.modal_pending.is_some();
        let body = div()
            .id("stay-pose-dialog")
            .v_flex()
            .gap_4()
            .w(px(width))
            .max_h(px(f32::from(window.viewport_size().height) - 48.))
            .overflow_y_scroll()
            .p_4()
            .rounded_lg()
            .bg(cx.theme().muted)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xl().font_bold().child(self.text(&format!(
                        "onboarding-stay_aligned-relaxed_poses-{}-title",
                        pose.name()
                    ))))
                    .child(
                        Button::new("stay-pose-x")
                            .ghost()
                            .label("×")
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stay_aligned.modal = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(self.stay_aligned_pose_content(pose, width - 32., window, true, cx))
            .when_some(
                self.ui_error.clone().or(self.snapshot.last_error.clone()),
                |d, e| d.child(div().text_color(cx.theme().danger).child(e)),
            )
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        Button::new("stay-pose-close")
                            .label(self.text("settings-stay_aligned-relaxed_poses-close"))
                            .disabled(busy)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.stay_aligned.modal = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .h_flex()
                            .gap_2()
                            .child(
                                Button::new("stay-pose-clear")
                                    .ghost()
                                    .label(
                                        self.text("settings-stay_aligned-relaxed_poses-reset_pose"),
                                    )
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.stay_aligned_action(Action::Clear(pose), cx)
                                    })),
                            )
                            .child(
                                Button::new("stay-pose-save")
                                    .primary()
                                    .label(
                                        self.text("settings-stay_aligned-relaxed_poses-save_pose"),
                                    )
                                    .disabled(busy)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.stay_aligned_action(Action::Detect(pose), cx)
                                    })),
                            ),
                    ),
            );
        div()
            .id("stay-pose-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.85))
            .v_flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.stay_aligned.modal_pending.is_none() {
                        this.stay_aligned.modal = None;
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .child(body)
            .into_any_element()
    }
}
