use super::*;
use slimevr_gpui::{assignment, protocol::TrackerKey};

impl SlimeView {
    pub(super) fn assignment_mode_strings(&self, mode: &str) -> (String, String, String) {
        let count = match mode {
            "lower-body" => 5,
            "core" => 6,
            "enhanced-core" => 8,
            "full-body" => 10,
            _ => 20,
        };
        let mut args = fluent_bundle::FluentArgs::new();
        args.set("mode", mode);
        args.set("trackersCount", count);
        (
            self.l10n
                .format("onboarding-assign_trackers-option-amount", &args),
            self.l10n
                .format("onboarding-assign_trackers-option-label", &args),
            self.l10n
                .format("onboarding-assign_trackers-option-description", &args),
        )
    }

    /// No role is retained after closing, navigation, or a disconnect. A tap
    /// outside the selector can only highlight a tracker, never assign it.
    pub(super) fn select_assignment(
        &mut self,
        selected: Option<TrackerKey>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(body) = self.assignment_role else {
            return false;
        };
        if self.snapshot.connection != Connection::Connected {
            return false;
        }
        let Some(feed) = &self.snapshot.feed else {
            return false;
        };
        if selected.is_some_and(|key| {
            !feed
                .trackers
                .iter()
                .any(|t| t.key == key && !t.computed && t.editable)
        }) {
            return false;
        }
        let requests = assignment::selection_requests(&feed.trackers, body, selected);
        if !requests.is_empty() {
            self.batch(requests, cx);
        } else {
            self.ui_error = None;
        }
        if self.ui_error.is_some() {
            return false;
        }
        self.assignment_role = None;
        cx.notify();
        true
    }

    pub(super) fn assignment_selection(
        &self,
        body: u8,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(260., 486.);
        let height = (f32::from(window.viewport_size().height) - 40.).max(250.);
        let neck_warning = body == BodyPart::NECK.0 && !self.neck_warning_accepted;
        let mut panel = div()
            .id("tracker-selection-panel")
            .v_flex()
            .w(px(width))
            .h(px(height))
            .gap_3()
            .py_4()
            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                this.assignment_role = None;
                cx.stop_propagation();
                cx.notify();
            }));
        if neck_warning {
            panel = panel
                .justify_center()
                .child(
                    div()
                        .p_4()
                        .rounded_lg()
                        .bg(rgb(0xffe135))
                        .text_color(rgb(0x081e30))
                        .child(self.text("tracker_selection_menu-neck_warning")),
                )
                .child(
                    div()
                        .h_flex()
                        .justify_center()
                        .gap_3()
                        .child(
                            Button::new("assignment-neck-cancel")
                                .label(self.text("tracker_selection_menu-neck_warning-cancel"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.assignment_role = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("assignment-neck-accept")
                                .primary()
                                .label(self.text("tracker_selection_menu-neck_warning-done"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.neck_warning_accepted = true;
                                    cx.notify();
                                })),
                        ),
                );
        } else {
            let title = self.text(&format!(
                "tracker_selection_menu-{}",
                BodyPart(body).variant_name().unwrap_or("NONE")
            ));
            panel = panel
                .child(div().text_2xl().font_bold().text_center().child(title))
                .child(
                    div().w_full().flex().justify_center().child(
                        div()
                            .w(px(width.min(384.)))
                            .p_2()
                            .h_flex()
                            .gap_4()
                            .rounded_lg()
                            .bg(cx.theme().secondary)
                            .text_color(cx.theme().link)
                            .child(
                                svg()
                                    .path("slime/Bulb.svg")
                                    .size(px(20.))
                                    .flex_shrink_0()
                                    .text_color(cx.theme().link),
                            )
                            .child(div().flex_1().min_w_0().child(self.text("tips-tap_setup"))),
                    ),
                );
            let mut list = div()
                .id("tracker-selection-list")
                .v_flex()
                .flex_1()
                .min_h_0()
                .gap_4()
                .p_2()
                .overflow_y_scroll();
            for assigned in [false, true] {
                let trackers: Vec<_> = self
                    .snapshot
                    .feed
                    .as_ref()
                    .map(|f| {
                        f.trackers
                            .iter()
                            .filter(|t| !t.computed && t.editable && (t.body != 0) == assigned)
                            .collect()
                    })
                    .unwrap_or_default();
                if trackers.is_empty() && !assigned {
                    continue;
                }
                list = list.child(self.text(if assigned {
                    "tracker_selection_menu-assigned"
                } else {
                    "tracker_selection_menu-unassigned"
                }));
                let mut cards = div().h_flex().flex_wrap().gap_3().w_full();
                for tracker in trackers {
                    let key = tracker.key;
                    cards = cards.child(
                        self.tracker_card_button(tracker, cx)
                            .w(px(if f32::from(window.viewport_size().width) >= 420. {
                                (width - 28.) / 2.
                            } else {
                                width - 16.
                            }))
                            .min_w_0()
                            .border_2()
                            .border_color(if tracker.body == body {
                                cx.theme().primary
                            } else {
                                cx.theme().popover
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_assignment(Some(key), cx);
                            })),
                    );
                }
                list = list.child(cards);
            }
            panel = panel.child(list).child(
                div().h_flex().justify_end().child(
                    Button::new("assignment-dont-assign")
                        .primary()
                        .label(self.text("tracker_selection_menu-dont_assign"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_assignment(None, cx);
                        })),
                ),
            );
        }
        div()
            .id("tracker-selection-overlay")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .occlude()
            .bg(cx.theme().background.opacity(0.9))
            .flex()
            .justify_center()
            .items_start()
            .pt_10()
            .child(panel)
            .into_any_element()
    }
}

pub(super) fn mode_option(amount: String, label: String, description: String) -> AnyElement {
    div()
        .h_flex()
        .w_full()
        .gap_3()
        .min_h(px(42.))
        .child(div().text_2xl().font_bold().child(amount))
        .child(
            div()
                .v_flex()
                .items_start()
                .flex_1()
                .min_w_0()
                .child(label)
                .child(description),
        )
        .into_any_element()
}
