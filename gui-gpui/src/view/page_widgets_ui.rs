use super::*;
impl SlimeView {
    pub(super) fn paragraph(&self, id: &str, width: f32, window: &Window) -> AnyElement {
        div()
            .v_flex()
            .gap_1()
            .children(
                self.settings_text_lines(self.text(id), width + 24., window)
                    .into_iter()
                    .map(|s| div().child(s)),
            )
            .into_any_element()
    }
    pub(super) fn tip(
        &self,
        id: &str,
        width: f32,
        window: &Window,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .h_flex()
            .items_start()
            .gap_4()
            .p_4()
            .rounded_lg()
            .bg(cx.theme().secondary)
            .text_color(cx.theme().primary)
            .child(
                svg()
                    .path("slime/Bulb.svg")
                    .size(px(20.))
                    .flex_shrink_0()
                    .text_color(cx.theme().primary),
            )
            .child(self.paragraph(id, width - 50., window))
            .into_any_element()
    }
}
impl SlimeView {
    pub(super) fn confirmation_dialog(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some((name, value, read)) = self.confirmation.clone() else {
            return div().into_any_element();
        };
        let body = div()
            .id("confirmation-body")
            .v_flex()
            .w(px((f32::from(window.viewport_size().width) - 40.).min(540.)))
            .rounded_lg()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .gap_3()
            .p_4()
            .bg(cx.theme().popover)
            .child(self.paragraph(
                match name.as_str() {
                    "SerialTrackerFactoryResetRequest" => "settings-serial-factory_reset-warning",
                    "SerialTrackerCustomCommandRequest" => "settings-serial-send_command-warning",
                    _ => "native-confirm-destructive",
                },
                500.,
                window,
            ))
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(
                        Button::new("confirm-destructive")
                            .label(self.text("native-confirm"))
                            .primary()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirmation = None;
                                if read.is_empty() {
                                    this.rpc(&name, value.clone(), cx);
                                } else {
                                    this.batch(
                                        vec![
                                            (name.clone(), value.clone()),
                                            (read.clone(), serde_json::json!({})),
                                        ],
                                        cx,
                                    );
                                }
                                if name == "SerialTrackerCustomCommandRequest"
                                    && this.ui_error.is_none()
                                {
                                    this.form_values.remove("serial-command");
                                }
                                if name == "ForgetDeviceRequest" && this.ui_error.is_none() {
                                    let mut ignored = this.preferences.value["ignoredTrackers"]
                                        .as_array()
                                        .cloned()
                                        .unwrap_or_default();
                                    if !ignored.contains(&value["mac_address"]) {
                                        ignored.push(value["mac_address"].clone());
                                    }
                                    this.preference(
                                        "ignoredTrackers",
                                        serde_json::json!(ignored),
                                        cx,
                                    );
                                    this.go(Page::Home, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("cancel-destructive")
                            .label(self.text("native-cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirmation = None;
                                cx.notify();
                            })),
                    ),
            );
        div()
            .id("confirmation-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.9))
            .v_flex()
            .items_center()
            .justify_center()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.confirmation = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .child(body)
            .into_any_element()
    }
}
