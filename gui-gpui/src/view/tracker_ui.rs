use super::mounting_ui::MountTarget;
use super::*;
use serde_json::json;
use slimevr_gpui::{mounting, protocol::TrackerKey};
impl SlimeView {
    pub(super) fn assign_tracker_role(
        &mut self,
        key: TrackerKey,
        body: u8,
        cx: &mut Context<Self>,
    ) {
        if !self.snapshot.feed.as_ref().is_some_and(|f| {
            f.trackers
                .iter()
                .any(|t| t.key == key && t.editable && !t.computed)
        }) {
            return;
        }
        self.rpc("AssignTrackerRequest",json!({"tracker_id":{"device_id":{"id":key.device},"tracker_num":key.sensor},"body_position":body}),cx);
        if self.ui_error.is_none() {
            self.tracker_role_target = None;
        }
        cx.notify();
    }
    pub(super) fn tracker_role_selection(
        &self,
        key: TrackerKey,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (f32::from(window.viewport_size().width) - 32.).clamp(300., 650.);
        let height = (f32::from(window.viewport_size().height) - 150.).clamp(360., 650.);
        let mut points =
            slimevr_gpui::assignment::targets("all", self.preferences.value["mirrorView"] != false);
        slimevr_gpui::assignment::position_labels(&mut points, height);
        let diagram = self.body_diagram(width, height, points, cx);
        div()
            .id("tracker-role-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.95))
            .v_flex()
            .items_center()
            .justify_center()
            .gap_3()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.tracker_role_target = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .id("tracker-role-dialog")
                    .v_flex()
                    .items_center()
                    .gap_3()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("tracker-settings-assignment_section-description")),
                    )
                    .child(diagram)
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .child(
                                Button::new("tracker-role-unassign")
                                    .primary()
                                    .label(self.text("tracker_selection_menu-dont_assign"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.assign_tracker_role(key, 0, cx)
                                    })),
                            )
                            .child(
                                Button::new("tracker-role-cancel")
                                    .ghost()
                                    .label(self.text("native-close"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.tracker_role_target = None;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn tracker_details(
        &mut self,
        key: TrackerKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tracker = self
            .snapshot
            .feed
            .as_ref()
            .and_then(|f| f.trackers.iter().find(|t| t.key == key))
            .cloned();
        let Some(t) = tracker else {
            return div()
                .child(self.text("native-unavailable"))
                .into_any_element();
        };
        let width = (f32::from(window.viewport_size().width) - 150.).max(300.);
        let narrow = width < 750.;
        let tracker_icon = if t.body == 3 {
            "Chest"
        } else if t.body == 6 {
            "Hip"
        } else if (7..=12).contains(&t.body) {
            "UpperLeg"
        } else {
            "SlimeVR"
        };
        let status = TrackerStatus(t.status)
            .variant_name()
            .unwrap_or("DISCONNECTED");
        let board = solarxr_protocol::datatypes::hardware_info::BoardType(t.board)
            .variant_name()
            .map(|name| self.text(&format!("board_type-{name}")))
            .unwrap_or_else(|| t.board_name.clone());
        let mut info = div()
            .v_flex()
            .gap_2()
            .w(px(if narrow { width } else { 320. }))
            .flex_shrink_0()
            .child(
                div()
                    .v_flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .min_h(px(192.))
                    .py_3()
                    .rounded_lg()
                    .bg(cx.theme().muted)
                    .child(
                        svg()
                            .path(format!("slime/{tracker_icon}.svg"))
                            .size(px(24.))
                            .text_color(cx.theme().primary),
                    )
                    .child(div().font_bold().child(t.name.clone()))
                    .child(
                        div()
                            .h_flex()
                            .items_center()
                            .gap_2()
                            .child(div().size(px(8.)).rounded_full().bg(if t.status == 2 {
                                rgb(0x50e897)
                            } else {
                                rgb(0xdf6d8c)
                            }))
                            .child(self.text(&format!("tracker-status-{}", status.to_lowercase()))),
                    )
                    .child(self.tracker_battery(&t, true, cx))
                    .child(self.tracker_wifi(&t, true, cx)),
            );
        let firmware =
            div()
                .v_flex()
                .gap_3()
                .p_3()
                .rounded_lg()
                .bg(cx.theme().muted)
                .child(
                    div()
                        .text_lg()
                        .child(self.text("tracker-settings-update-title")),
                )
                .child(
                    div()
                        .h_flex()
                        .justify_between()
                        .gap_3()
                        .child(self.text("tracker-settings-build-date"))
                        .child(if t.firmware_date.is_empty() {
                            "—".into()
                        } else {
                            t.firmware_date.clone()
                        }),
                )
                .child(
                    div()
                        .h_flex()
                        .justify_between()
                        .gap_3()
                        .child(self.text("tracker-settings-current-version"))
                        .child(if t.firmware.is_empty() {
                            "—".into()
                        } else {
                            t.firmware.clone()
                        }),
                )
                .child(
                    Button::new("tracker-firmware-tools")
                        .label(self.text("settings-sidebar-firmware-tool"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go(Page::Settings(Section::Firmware), cx)
                        })),
                );
        if !t.computed {
            info = info.child(firmware);
        }
        let mut facts = div()
            .v_flex()
            .gap_3()
            .p_3()
            .rounded_lg()
            .bg(cx.theme().muted);
        for (label, value) in [
            ("tracker-infos-manufacturer", t.manufacturer.clone()),
            ("tracker-infos-display_name", t.display_name.clone()),
            (
                "tracker-infos-url",
                t.address.clone().unwrap_or_else(|| "—".into()),
            ),
            (
                "tracker-infos-custom_name",
                t.custom_name.clone().unwrap_or_else(|| "—".into()),
            ),
            ("tracker-infos-hardware_identifier", t.hardware.clone()),
            ("tracker-infos-imu", t.imu.clone()),
            ("tracker-infos-board_type", board),
            (
                "tracker-infos-network_version",
                t.network_version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
        ] {
            facts = facts.child(
                div()
                    .h_flex()
                    .justify_between()
                    .gap_3()
                    .child(self.text(label))
                    .child(div().text_right().child(value)),
            );
        }
        for (label, value) in [
            ("TPS", t.tps.map(|n| n.to_string())),
            ("RSSI", t.rssi.map(|n| format!("{n} dBm"))),
            ("V", t.voltage.map(|n| format!("{n:.2}"))),
            ("°C", t.temperature.map(|n| format!("{n:.1}"))),
        ] {
            facts = facts.child(
                div()
                    .h_flex()
                    .justify_between()
                    .child(label)
                    .child(value.unwrap_or_else(|| "—".into())),
            );
        }
        info = info
            .child(facts)
            .when(t.is_imu, |d| d.child(self.imu_native(&t, cx)));
        let right_width = if narrow { width } else { width - 328. };
        let mut settings = div()
            .v_flex()
            .flex_1()
            .min_w_0()
            .gap_3()
            .p_5()
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                div().h_flex().child(
                    Button::new("tracker-back")
                        .ghost()
                        .label(self.text("tracker-settings-back"))
                        .on_click(cx.listener(|this, _, _, cx| this.go(this.return_page, cx))),
                ),
            )
            .child(
                div()
                    .text_2xl()
                    .font_bold()
                    .child(self.text("tracker-settings-title")),
            )
            .child(
                div()
                    .mt_3()
                    .text_lg()
                    .child(self.text("tracker-settings-assignment_section")),
            )
            .child(self.paragraph(
                "tracker-settings-assignment_section-description",
                right_width - 40.,
                window,
            ))
            .child(
                div()
                    .h_flex()
                    .justify_between()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .rounded_lg()
                    .bg(cx.theme().background)
                    .child(self.body_name(t.body))
                    .child(
                        Button::new("tracker-edit-role")
                            .label(self.text("tracker-settings-assignment_section-edit"))
                            .disabled(!t.editable)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.tracker_role_target = Some(key);
                                window.focus(&this.focus, cx);
                                cx.notify();
                            })),
                    ),
            );
        if t.is_imu {
            let direction = t
                .mounting
                .and_then(mounting::direction)
                .map(|i| mounting::DIRECTIONS[i].0)
                .unwrap_or("custom");
            settings = settings
                .child(
                    div()
                        .mt_3()
                        .text_lg()
                        .child(self.text("tracker-settings-mounting_section")),
                )
                .child(self.paragraph(
                    "tracker-settings-mounting_section-description",
                    right_width - 40.,
                    window,
                ))
                .child(
                    div()
                        .h_flex()
                        .justify_between()
                        .items_center()
                        .gap_3()
                        .p_3()
                        .rounded_lg()
                        .bg(cx.theme().background)
                        .child(self.text(&format!("tracker-rotation-{direction}")))
                        .child(
                            Button::new("tracker-edit-mounting")
                                .label(self.text("tracker-settings-mounting_section-edit"))
                                .disabled(!t.editable)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.mounting_target = Some(MountTarget::Tracker(key));
                                    window.focus(&this.focus, cx);
                                    cx.notify();
                                })),
                        ),
                );
            if self.preferences.value["debug"] == true {
                settings = settings.child(self.rpc_button(
                    "clear-mounting",
                    "native-mounting-clear",
                    "ClearMountingResetRequest",
                    json!({}),
                    cx,
                ));
            }
        }
        if t.editable && t.magnetometer != 0 {
            settings=settings.child(div().mt_3().text_lg().child(self.text("tracker-settings-use_mag")))
                .child(self.paragraph("tracker-settings-use_mag-description",right_width-40.,window))
                .child(Switch::new("tracker-mag-toggle").checked(t.magnetometer==2).accessibility_label(self.text("tracker-settings-use_mag-label"))
                    .on_click(cx.listener(move|this,enable,_,cx|this.rpc("ChangeMagToggleRequest",json!({"tracker_id":{"device_id":{"id":key.device},"tracker_num":key.sensor},"enable":*enable}),cx))));
        }
        let name_key = format!("name-{}-{}", key.device, key.sensor);
        settings=settings.child(div().mt_3().text_lg().child(self.text("tracker-settings-name_section")))
            .child(self.paragraph("tracker-settings-name_section-description",right_width-40.,window))
            .child(div().h_flex().gap_3().child(self.local_input(&name_key,&t.name,false,window,cx))
                .child(Button::new("rename-tracker").label(self.text("native-rename")).disabled(!t.editable).on_click(cx.listener(move|this,_,_,cx|{
                    let name=this.form_value(&format!("name-{}-{}",key.device,key.sensor),"");if name.trim().is_empty(){return;}
                    let body=this.snapshot.feed.as_ref().and_then(|f|f.trackers.iter().find(|t|t.key==key)).map(|t|t.body).unwrap_or(0);
                    this.rpc("AssignTrackerRequest",json!({"tracker_id":{"device_id":{"id":key.device},"tracker_num":key.sensor},"body_position":body,"display_name":name}),cx);
                }))));
        if key.device != 0 && !t.computed {
            settings = settings
                .child(
                    div()
                        .mt_3()
                        .text_lg()
                        .child(self.text("tracker-settings-forget")),
                )
                .child(self.paragraph(
                    "tracker-settings-forget-description",
                    right_width - 40.,
                    window,
                ))
                .child(
                    div().h_flex().child(
                        Button::new("forget-device")
                            .danger()
                            .label(self.text("tracker-settings-forget-label"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirmation = Some((
                                    "ForgetDeviceRequest".into(),
                                    json!({"mac_address":t.hardware}),
                                    "".into(),
                                ));
                                cx.notify();
                            })),
                    ),
                );
        }
        if self.preferences.value["debug"] == true {
            let mut debug = div()
                .v_flex()
                .gap_2()
                .p_3()
                .rounded_lg()
                .bg(cx.theme().background);
            let precision = if self.preferences.value["devSettings"]["preciseRotation"] == true {
                6
            } else {
                3
            };
            for (label, value) in [
                ("Rotation", t.rotation.map(|n| format!("{n:.precision$?}"))),
                (
                    "Raw rotation",
                    t.raw_rotation.map(|n| format!("{n:.precision$?}")),
                ),
                ("Position", t.position.map(|n| format!("{n:.precision$?}"))),
                (
                    "Acceleration",
                    t.acceleration.map(|n| format!("{n:.precision$?}")),
                ),
                (
                    "Magnetic field",
                    t.magnetic.map(|n| format!("{n:.precision$?}")),
                ),
                ("Packet loss", t.packet_loss.map(|n| format!("{n:.1}%"))),
                (
                    "Yaw correction",
                    t.yaw_correction.map(|n| format!("{n:.1}°")),
                ),
            ] {
                debug = debug.child(format!("{label}: {}", value.unwrap_or_else(|| "—".into())));
            }
            settings = settings.child(debug);
        }
        div()
            .flex()
            .gap_2()
            .when(narrow, |d| d.flex_col())
            .when(!narrow, |d| d.flex_row())
            .child(info)
            .child(settings)
            .into_any_element()
    }
}
