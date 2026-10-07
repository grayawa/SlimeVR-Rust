use super::*;
use serde_json::{Value, json};
impl SlimeView {
    pub(super) fn serial(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let devices = self.read("SerialDevicesResponse")["devices"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let selected = self.form_value("serial-port", "");
        let opened = self.snapshot.serial_device.is_some();
        let device_type = self
            .snapshot
            .serial_device
            .as_ref()
            .and_then(|d| d["type"].as_u64())
            .unwrap_or(0);
        let port_label = if selected.is_empty() {
            self.text("settings-serial-auto_dropdown_item")
        } else {
            selected.clone()
        };
        let auto_label = self.text("settings-serial-auto_dropdown_item");
        let target = cx.entity().downgrade();
        let ports =
            Button::new("serial-ports")
                .label(port_label)
                .dropdown_menu(move |mut menu, _, _| {
                    for port in std::iter::once(String::new()).chain(
                        devices
                            .iter()
                            .filter_map(|d| d["port"].as_str())
                            .map(str::to_owned),
                    ) {
                        let target = target.clone();
                        let label = if port.is_empty() {
                            auto_label.clone()
                        } else {
                            port.clone()
                        };
                        menu = menu.item(
                            PopupMenuItem::new(label)
                                .checked(selected == port)
                                .on_click(move |_, _, cx| {
                                    let _ = target.update(cx, |this, cx| {
                                        this.form_values.insert("serial-port".into(), port.clone());
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                });
        let paused = self.form_value("serial-scroll-paused", "false") == "true";
        if !paused {
            self.serial_scroll.scroll_to_bottom();
        }
        let height = (f32::from(window.viewport_size().height) - 400.).max(180.);
        let mut toolbar = div()
            .h_flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.text("settings-serial-serial_select"))
            .child(ports)
            .child(
                Button::new("serial-toggle")
                    .label(self.text(if opened {
                        "native-close"
                    } else {
                        "native-open"
                    }))
                    .disabled(self.snapshot.connection != Connection::Connected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if opened {
                            this.rpc("CloseSerialRequest", json!({}), cx);
                        } else {
                            let port = this.form_value("serial-port", "");
                            this.rpc(
                                "OpenSerialRequest",
                                json!({"auto":port.is_empty(),"port":port}),
                                cx,
                            );
                        }
                    })),
            )
            .child(self.rpc_button(
                "serial-refresh",
                "native-refresh",
                "SerialDevicesRequest",
                json!({}),
                cx,
            ));
        if opened {
            for (id, label, rpc) in [
                (
                    "reboot",
                    "settings-serial-reboot",
                    "SerialTrackerRebootRequest",
                ),
                ("info", "native-serial-info", "SerialTrackerGetInfoRequest"),
            ] {
                toolbar = toolbar.child(self.rpc_button(
                    &format!("serial-{id}"),
                    label,
                    rpc,
                    json!({}),
                    cx,
                ));
            }
            if device_type == 0 {
                toolbar = toolbar
                    .child(self.rpc_button(
                        "serial-scan",
                        "settings-serial-get_wifi_scan",
                        "SerialTrackerGetWifiScanRequest",
                        json!({}),
                        cx,
                    ))
                    .child(
                        Button::new("serial-factory-reset")
                            .label(self.text("settings-serial-factory_reset"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirmation = Some((
                                    "SerialTrackerFactoryResetRequest".into(),
                                    json!({}),
                                    "".into(),
                                ));
                                cx.notify();
                            })),
                    );
            } else {
                for (command, label) in [
                    ("pair", "settings-serial-enter_pairing"),
                    ("exit", "settings-serial-exit_pairing"),
                    ("calibrate", "settings-serial-calibrate"),
                    ("6-side", "settings-serial-six_side_calibrate"),
                    ("dfu", "settings-serial-dfu"),
                    ("meow", "settings-serial-meow"),
                ] {
                    if (command == "exit" && device_type != 1)
                        || (["calibrate", "6-side"].contains(&command) && device_type != 2)
                    {
                        continue;
                    }
                    toolbar = toolbar.child(self.rpc_button(
                        &format!("serial-{command}"),
                        label,
                        "SerialTrackerCustomCommandRequest",
                        json!({"command":command}),
                        cx,
                    ));
                }
            }
        }
        toolbar = toolbar
            .child(
                Button::new("serial-save-log")
                    .label(self.text("settings-serial-save_logs"))
                    .disabled(self.snapshot.serial_log.is_empty())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.save_bytes(
                            "slimevr-serial.log".into(),
                            this.snapshot.serial_log.as_bytes().to_vec(),
                            cx,
                        )
                    })),
            )
            .child(
                Button::new("copy-serial-log")
                    .label(self.text("native-copy-diagnostics"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            this.snapshot.serial_log.clone(),
                        ))
                    })),
            )
            .child(
                Button::new("serial-pause-scroll")
                    .label(self.text(if paused {
                        "native-serial-resume-scroll"
                    } else {
                        "native-serial-pause-scroll"
                    }))
                    .selected(paused)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.form_values
                            .insert("serial-scroll-paused".into(), (!paused).to_string());
                        cx.notify();
                    })),
            );
        let console = self.snapshot.serial_log.clone();
        let mut page = div()
            .v_flex()
            .gap_4()
            .child(self.paragraph(
                "settings-serial-description",
                (f32::from(window.viewport_size().width) - 400.).max(250.),
                window,
            ))
            .child(
                div()
                    .v_flex()
                    .p_3()
                    .rounded_lg()
                    .gap_3()
                    .bg(cx.theme().background)
                    .child(
                        div()
                            .id("serial-monitor")
                            .h(px(height))
                            .overflow_y_scroll()
                            .track_scroll(&self.serial_scroll)
                            .font_family("monospace")
                            .text_sm()
                            .child(div().v_flex().children(
                                console.lines().map(|line| div().child(line.to_owned())),
                            )),
                    )
                    .child(toolbar)
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .items_center()
                            .child(self.text("settings-serial-send_command-placeholder"))
                            .child(
                                div()
                                    .p_1()
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .rounded_lg()
                                    .child(self.local_input(
                                        "serial-command",
                                        "",
                                        false,
                                        window,
                                        cx,
                                    )),
                            )
                            .child(
                                Button::new("serial-send")
                                    .label(self.text("settings-serial-send_command"))
                                    .disabled(!opened)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let command = this.form_value("serial-command", "");
                                        if command.trim().is_empty() {
                                            return;
                                        }
                                        this.confirmation = Some((
                                            "SerialTrackerCustomCommandRequest".into(),
                                            json!({"command":command}),
                                            "".into(),
                                        ));
                                        cx.notify();
                                    })),
                            ),
                    ),
            );
        let ignored = self.preferences.value["ignoredTrackers"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if !ignored.is_empty() {
            let mut recovery = div()
                .v_flex()
                .gap_2()
                .child(self.text("native-forgotten-devices"));
            for mac in ignored.iter().filter_map(Value::as_str) {
                let mac = mac.to_owned();
                recovery = recovery.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .items_center()
                        .child(mac.clone())
                        .child(
                            Button::new(format!("restore-{mac}"))
                                .label(self.text("native-accept-device"))
                                .disabled(self.snapshot.connection != Connection::Connected)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.rpc(
                                        "AddUnknownDeviceRequest",
                                        json!({"mac_address":mac}),
                                        cx,
                                    );
                                    if this.ui_error.is_none() {
                                        let mut remaining =
                                            this.preferences.value["ignoredTrackers"]
                                                .as_array()
                                                .cloned()
                                                .unwrap_or_default();
                                        remaining
                                            .retain(|value| value.as_str() != Some(mac.as_str()));
                                        this.preference("ignoredTrackers", json!(remaining), cx);
                                    }
                                })),
                        ),
                );
            }
            page = page.child(recovery);
        }
        page.into_any_element()
    }
}
