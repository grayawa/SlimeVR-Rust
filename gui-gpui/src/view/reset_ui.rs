use super::*;

impl SlimeView {
    pub(super) fn reset_button(
        &self,
        id: impl Into<ElementId>,
        kind: ResetKind,
        parts: &[u8],
        toolbar: bool,
        cx: &Context<Self>,
    ) -> Button {
        let frame = self
            .reset_feedback
            .frame(kind.wire().0, parts, std::time::Instant::now());
        let label = self.text(if kind == ResetKind::Mounting && !parts.is_empty() {
            "reset-mounting-feet"
        } else {
            kind.label()
        });
        let skiing = kind == ResetKind::Mounting && parts.is_empty();
        let path = match kind {
            ResetKind::Full => "slime/FullReset.svg",
            ResetKind::Yaw => "slime/YawReset.svg",
            ResetKind::Mounting if skiing => "slime/Ski.svg",
            ResetKind::Mounting => "slime/Foot.svg",
        };
        let icon_size = if skiing { 24. } else { 18. };
        let (angle, x, y) = if toolbar && skiing {
            frame.skiing()
        } else if toolbar {
            (-360. * frame.completion, 0., 0.)
        } else {
            (0., 0., 0.)
        };
        let angle = angle.to_radians();
        let translation = point(
            px((x * angle.cos() - y * angle.sin()) * icon_size),
            px((x * angle.sin() + y * angle.cos()) * icon_size),
        );
        let icon = svg()
            .path(path)
            .size(px(icon_size))
            .text_color(cx.theme().foreground)
            .with_transformation(
                Transformation::rotate(radians(angle)).with_translation(translation),
            );
        let counting = frame.counting && (toolbar || kind != ResetKind::Yaw);
        let foreground = div()
            .h_flex()
            .gap_2()
            .items_center()
            .justify_center()
            .opacity(if toolbar && counting { 0. } else { 1. })
            .child(icon)
            .child(
                div()
                    .relative()
                    // The label reserves the same width through every countdown frame.
                    .child(
                        div()
                            .opacity(if counting { 0. } else { 1. })
                            .child(label.clone()),
                    )
                    .when(!toolbar && counting, |d| {
                        d.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(cx.theme().primary_foreground)
                                .child(frame.timer()),
                        )
                    }),
            );
        let content = div()
            .relative()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .px_3()
            .when(!toolbar, |d| {
                d.border_2()
                    .rounded_md()
                    .border_color(cx.theme().success.opacity(frame.outline))
            })
            .when(toolbar, |d| {
                d.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .h_full()
                        .w(relative(frame.progress))
                        .bg(cx.theme().primary.opacity(0.5)),
                )
            })
            .child(foreground)
            .when(toolbar && counting, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_bold()
                        .text_color(cx.theme().foreground)
                        .text_size(px(30. * frame.timer_scale))
                        .child(frame.timer()),
                )
            });
        Button::new(id)
            .accessibility_label(label)
            .p_0()
            .overflow_hidden()
            .when(toolbar, |b| {
                b.bg(cx.theme().popover).rounded_lg().border_0()
            })
            .when(!toolbar, |b| {
                b.primary().border_0().when(frame.counting, |b| {
                    let mut color = cx.theme().primary;
                    color.l *= 0.75;
                    b.bg(color)
                })
            })
            .child(content)
    }
}
