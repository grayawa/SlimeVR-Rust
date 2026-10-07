use super::*;
use serde_json::{Value, json};
use slimevr_gpui::{client::Event, proportions, protocol::TrackerKey, rpc_generated};

impl SlimeView {
    pub(super) fn batch(&mut self, requests: Vec<(String, Value)>, cx: &mut Context<Self>) {
        let reset = requests
            .iter()
            .any(|(name, _)| name == "SkeletonResetAllRequest");
        let bones: Vec<_> = requests
            .iter()
            .filter(|(name, _)| name == "ChangeSkeletonConfigRequest")
            .filter_map(|(_, value)| value["bone"].as_u64())
            .collect();
        self.ui_error = self.client.batch(requests).err();
        if self.ui_error.is_none() {
            for (key, value) in &self.form_values {
                let changed = reset && key == "scale-height"
                    || key.starts_with("bone-")
                        && key
                            .rsplit('-')
                            .next()
                            .and_then(|id| id.parse::<u64>().ok())
                            .is_some_and(|id| reset || bones.contains(&id))
                    || key.starts_with("group-")
                        && proportions::GROUPS.iter().any(|(name, ids)| {
                            key == &format!("group-{name}")
                                && (reset || ids.iter().any(|id| bones.contains(id)))
                        });
                if changed {
                    self.proportion_pending.insert(key.clone(), value.clone());
                }
            }
        }
        if self.ui_error.is_none() && !bones.is_empty() {
            self.preference("lastUsedProportions", json!("manual"), cx);
        }
        cx.notify();
    }
    pub(super) fn read(&self, name: &str) -> std::sync::Arc<Value> {
        self.snapshot
            .rpc
            .get(name)
            .map(|r| r.value.clone())
            .unwrap_or_else(|| std::sync::Arc::new(Value::Null))
    }
    pub(super) fn rpc_button(
        &self,
        id: &str,
        label: &str,
        name: &str,
        value: Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let name = name.to_owned();
        Button::new(id.to_owned())
            .label(self.text(label))
            .disabled(self.snapshot.connection != Connection::Connected)
            .on_click(cx.listener(move |this, _, _, cx| this.rpc(&name, value.clone(), cx)))
            .into_any_element()
    }
    pub(super) fn events(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        for event in events {
            if event.session != self.snapshot.session {
                continue;
            }
            let volume = if self.preferences.value["feedbackSound"] == true {
                self.preferences.value["feedbackSoundVolume"]
                    .as_f64()
                    .unwrap_or(0.5) as f32
            } else {
                0.0
            };
            match event.name.as_str() {
                "ResetResponse" => {
                    if let Some((session, tx, next)) = self.mounting_wait
                        && session == event.session
                        && tx == event.tx
                        && event.value["status"] == 1
                    {
                        self.mounting_wait = None;
                        if (self.navigation.page == Page::Mounting
                            || self.in_onboarding()
                                && self.onboarding.step == slimevr_gpui::onboarding::Step::Mounting)
                            && self.form_value("mounting-method", "choose") == "automatic"
                        {
                            self.form_values
                                .insert("mounting-step".into(), next.to_string());
                        }
                    }
                    for cue in self.sounds.reset(
                        event.session,
                        event.tx,
                        event.value["reset_type"].as_u64().unwrap_or(255) as u8,
                        event.value["status"] == 1,
                        event.value["progress"].as_i64().unwrap_or(0) as i32,
                    ) {
                        self.audio.play(cue, volume);
                    }
                }
                "WifiProvisioningStatusResponse"
                    if self.in_onboarding()
                        && self.onboarding.step == slimevr_gpui::onboarding::Step::Connect =>
                {
                    if self
                        .onboarding
                        .accepts_wifi(event.session, event.tx, event.sequence)
                        && (event.value["status"] != 0 || event.tx != 0)
                    {
                        let status = event.value["status"].as_u64().unwrap_or(0);
                        if status != self.onboarding.wifi_status && matches!(status, 9 | 10) {
                            self.onboarding.dialog =
                                Some(slimevr_gpui::onboarding::Dialog::WifiError(status));
                        }
                        self.onboarding.wifi_status = status;
                    }
                }
                "TrackingPauseStateResponse" => {
                    if let Some(cue) = self.sounds.pause(event.value["trackingPaused"] == true) {
                        self.audio.play(cue, volume);
                    }
                }
                "UnknownDeviceHandshakeNotification" => {
                    if let Some(mac) = event.value["mac_address"].as_str()
                        && !self.preferences.value["ignoredTrackers"]
                            .as_array()
                            .is_some_and(|a| a.iter().any(|m| m == mac))
                        && !self.unknown_devices.iter().any(|m| m == mac)
                    {
                        self.unknown_devices.push(mac.into());
                    }
                }
                "TapDetectionSetupNotification" => {
                    let id = &event.value["tracker_id"];
                    if let (Some(device), Some(sensor)) =
                        (id["device_id"]["id"].as_u64(), id["tracker_num"].as_u64())
                    {
                        let key = TrackerKey {
                            device: device as u8,
                            sensor: sensor as u8,
                        };
                        self.highlight = Some((key.device, key.sensor, std::time::Instant::now()));
                        if self.assignment_role.is_some_and(|body| {
                            body != BodyPart::NECK.0 || self.neck_warning_accepted
                        }) && self.select_assignment(Some(key), cx)
                        {
                            self.audio.play(self.sounds.tap(), volume);
                        }
                    }
                }
                "NewSerialDeviceResponse" => {
                    let _ = self.client.rpc("SerialDevicesRequest", json!({}));
                    if self.preferences.value["watchNewDevices"] == true {
                        self.notice = Some(self.text("native-new-serial"));
                    }
                }
                "SkeletonConfigResponse" => {
                    if self.snapshot.last_error.is_none() {
                        for (key, value) in self.proportion_pending.drain() {
                            if self.form_values.get(&key) == Some(&value) {
                                self.form_values.remove(&key);
                            }
                        }
                    }
                }
                "AutoBoneEpochResponse" => {
                    self.autobone_result = Some((*event.value).clone());
                }
                "AutoBoneProcessStatusResponse" => {
                    if event.value["process_type"] == 3 && event.value["completed"] == true {
                        self.autobone_valid = event.value["success"] == true;
                    }
                    if self.autobone_auto
                        && event.value["process_type"] == 1
                        && event.value["completed"] == true
                    {
                        self.autobone_auto = false;
                        if event.value["success"] == true {
                            let _ = self
                                .client
                                .rpc("AutoBoneProcessRequest", json!({"process_type":3}));
                        }
                    }
                }
                "MagToggleResponse" => {
                    if event.value["tracker_id"].is_null() {
                        self.global_mag = event.value["enable"] == true;
                    }
                }
                "UserHeightRecordingStatusResponse" => {
                    if event.value["status"] == 6 {
                        let _ = self.client.rpc("SkeletonConfigRequest", json!({}));
                    }
                }
                "FirmwareUpdateStatusResponse" => {
                    let id = event.value["device_id"]["value"]["port"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| {
                            event.value["device_id"]["value"]["id"]["id"].to_string()
                        });
                    self.firmware_status.insert(id, (*event.value).clone());
                }
                "SaveFileNotification" => {
                    let name = event.value["expected_filename"]
                        .as_str()
                        .unwrap_or("recording.bvh")
                        .to_owned();
                    let bytes = event.bytes.map(|b| b.to_vec()).unwrap_or_default();
                    self.save_bytes(name, bytes, cx);
                }
                "PubSub" => {
                    self.overlay.apply(event.session, &event.value);
                }
                "BackendNotice" => {
                    if event.value["type"] == "backend_info" {
                        self.driver_notice =
                            event.value["driver_notice"] == "existing_manual_driver";
                        if let Some(error) = event.value["driver_error"].as_str() {
                            self.ui_error = Some(error.into());
                        }
                        continue;
                    }
                    if event.value["type"] == "backend_file_saved" {
                        if let Some(path) = event.value["path"].as_str() {
                            self.saved_file = Some(path.into());
                        }
                        continue;
                    }
                    if let Some(message) = event.value["message"]
                        .as_str()
                        .or(event.value["reason"].as_str())
                    {
                        self.notice = Some(message.to_owned());
                    }
                }
                _ => (),
            }
        }
    }
    pub(super) fn save_bytes(&mut self, name: String, bytes: Vec<u8>, cx: &mut Context<Self>) {
        let future = cx.prompt_for_new_path(&self.paths.documents(), Some(&name));
        cx.spawn(async move |this, cx| match future.await {
            Ok(Ok(Some(path))) => {
                let saved = path.clone();
                let result =
                    smol::unblock(move || std::fs::write(path, bytes).map_err(|e| e.to_string()))
                        .await;
                let _ = this.update(cx, |this, cx| {
                    match result {
                        Ok(()) => this.saved_file = Some(saved),
                        Err(e) => this.ui_error = Some(e),
                    }
                    cx.notify();
                });
            }
            Ok(Err(e)) => {
                let _ = this.update(cx, |this, cx| {
                    this.ui_error = Some(e.to_string());
                    cx.notify();
                });
            }
            _ => (),
        })
        .detach();
    }
    fn import_proportions(&mut self, cx: &mut Context<Self>) {
        let future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.text("native-proportions-import").into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = future.await
                && let Some(path) = paths.first()
            {
                let path = path.clone();
                let result = smol::unblock(move || {
                    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
                    if bytes.len() > 1024 * 1024 {
                        return Err("Body measurement file is too large".into());
                    }
                    let value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
                    proportions::import(&value)
                })
                .await;
                let _ = this.update(cx, |this, cx| match result {
                    Ok(requests) => this.batch(requests, cx),
                    Err(e) => {
                        this.ui_error = Some(e);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }
    pub(super) fn bvh(&self, cx: &mut Context<Self>) -> AnyElement {
        let recording = self.read("RecordBVHStatus")["recording"]
            .as_bool()
            .unwrap_or(false);
        div()
            .h_flex()
            .gap_3()
            .child(
                Button::new("bvh-toggle")
                    .label(self.text(if recording {
                        "native-bvh-stop"
                    } else {
                        "native-bvh-start"
                    }))
                    .disabled(self.snapshot.connection != Connection::Connected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if recording {
                            this.rpc("RecordBVHRequest", json!({"stop":true}), cx);
                        } else {
                            if let Some(directory) = this.preferences.value["bvhDirectory"]
                                .as_str()
                                .filter(|s| !s.is_empty())
                            {
                                this.rpc(
                                    "RecordBVHRequest",
                                    json!({"stop":false,"path":directory}),
                                    cx,
                                );
                                return;
                            }
                            let future = cx.prompt_for_new_path(
                                &this.paths.documents(),
                                Some("SlimeVR-recording.bvh"),
                            );
                            cx.spawn(async move |this, cx| {
                                if let Ok(Ok(Some(path))) = future.await {
                                    let _ = this.update(cx, |this, cx| {
                                        this.rpc(
                                            "RecordBVHRequest",
                                            json!({"stop":false,"path":path.to_string_lossy()}),
                                            cx,
                                        )
                                    });
                                }
                            })
                            .detach();
                        }
                    })),
            )
            .when_some(self.saved_file.clone(), |row, path| {
                row.child(
                    Button::new("reveal-recording")
                        .label(self.text("native-open-file-folder"))
                        .on_click(move |_, _, cx| cx.reveal_path(&path)),
                )
            })
            .into_any_element()
    }
    pub(super) fn proportions(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let skeleton = self.read("SkeletonConfigResponse");
        let parts = skeleton["skeleton_parts"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let mode = self.form_value("proportions-mode", "ratios");
        let mut page = div().v_flex().gap_2().child(
            div()
                .h_flex()
                .gap_3()
                .children(["linear", "ratios"].into_iter().map(|mode| {
                    Button::new(format!("mode-{mode}"))
                        .label(self.text(if mode == "ratios" {
                            "onboarding-manual_proportions-grouped_proportions"
                        } else {
                            "onboarding-manual_proportions-all_proportions"
                        }))
                        .selected(self.form_value("proportions-mode", "ratios") == mode)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.form_values
                                .insert("proportions-mode".into(), mode.into());
                            cx.notify();
                        }))
                }))
                .child(
                    Button::new("proportions-precision")
                        .label(self.text(
                            if self.form_value("proportions-precise", "false") == "true" {
                                "onboarding-manual_proportions-precise_increment"
                            } else {
                                "onboarding-manual_proportions-normal_increment"
                            },
                        ))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let enabled = this.form_value("proportions-precise", "false") != "true";
                            this.form_values
                                .insert("proportions-precise".into(), enabled.to_string());
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("proportions-autobone")
                        .label(self.text("onboarding-manual_proportions-fine_tuning_button"))
                        .on_click(
                            cx.listener(|this, _, _, cx| this.open_proportions("automatic", cx)),
                        ),
                ),
        );
        page = page.child(
            div()
                .h_flex()
                .gap_3()
                .child(
                    Button::new("import-proportions")
                        .label(self.text("onboarding-manual_proportions-import"))
                        .on_click(cx.listener(|this, _, _, cx| this.import_proportions(cx))),
                )
                .child(
                    Button::new("export-proportions")
                        .label(self.text("onboarding-manual_proportions-export"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let value = proportions::export(&this.read("SkeletonConfigResponse"));
                            match serde_json::to_vec_pretty(&value) {
                                Ok(bytes) => {
                                    this.save_bytes("body-proportions.json".into(), bytes, cx)
                                }
                                Err(e) => this.ui_error = Some(e.to_string()),
                            }
                        })),
                )
                .child(
                    Button::new("reset-proportions")
                        .label(self.text("native-proportions-reset"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.confirmation = Some((
                                "SkeletonResetAllRequest".into(),
                                json!({}),
                                "SkeletonConfigRequest".into(),
                            ));
                            cx.notify();
                        })),
                ),
        );
        if mode == "scaled" {
            let height = skeleton["user_height"].as_f64().unwrap_or(1.7) * 100.0;
            page = page.child(
                div()
                    .h_flex()
                    .gap_3()
                    .items_center()
                    .child(self.text("native-scale-height"))
                    .child(self.local_input(
                        "scale-height",
                        &format!("{height:.1}"),
                        false,
                        window,
                        cx,
                    ))
                    .child(
                        Button::new("scale-apply")
                            .label(self.text("native-scale-apply"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let height = this
                                    .form_value("scale-height", "170")
                                    .parse::<f64>()
                                    .unwrap_or(f64::NAN)
                                    / 100.0;
                                match proportions::scale(
                                    &this.read("SkeletonConfigResponse"),
                                    height,
                                ) {
                                    Ok(r) => this.batch(r, cx),
                                    Err(e) => {
                                        this.ui_error = Some(e);
                                        cx.notify();
                                    }
                                }
                            })),
                    ),
            );
        }
        if mode == "ratios" {
            for (name, ids) in proportions::GROUPS {
                let total: f64 = parts
                    .iter()
                    .filter(|p| p["bone"].as_u64().is_some_and(|id| ids.contains(&id)))
                    .filter_map(|p| p["value"].as_f64())
                    .sum();
                let key = format!("group-{name}");
                page = page.child(
                    div()
                        .h_flex()
                        .gap_2()
                        .items_center()
                        .p_3()
                        .h(px(80.))
                        .rounded_lg()
                        .bg(cx.theme().muted)
                        .child(
                            div()
                                .flex_1()
                                .font_bold()
                                .child(self.text(&format!("skeleton_bone-{name}"))),
                        )
                        .children([-5., -1.].into_iter().map(|delta| {
                            self.proportion_group_increment(name, ids, total, delta, cx)
                        }))
                        .child(self.local_input(
                            &key,
                            &format!("{:.2}", total * 100.0),
                            false,
                            window,
                            cx,
                        ))
                        .children([1., 5.].into_iter().map(|delta| {
                            self.proportion_group_increment(name, ids, total, delta, cx)
                        }))
                        .child(
                            Button::new(format!("expand-group-{name}"))
                                .ghost()
                                .label(
                                    if self.form_value(
                                        &format!("proportion-group-open-{name}"),
                                        "false",
                                    ) == "true"
                                    {
                                        "⌃"
                                    } else {
                                        "⌄"
                                    },
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let key = format!("proportion-group-open-{name}");
                                    let open = this.form_value(&key, "false") != "true";
                                    this.form_values.insert(key, open.to_string());
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new(format!("apply-{key}"))
                                .label(self.text("native-apply"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let value = this
                                        .form_value(&format!("group-{name}"), "")
                                        .parse::<f64>()
                                        .unwrap_or(f64::NAN)
                                        / 100.0;
                                    match proportions::resize_group(
                                        &this.read("SkeletonConfigResponse"),
                                        ids,
                                        value,
                                    ) {
                                        Ok(r) => this.batch(r, cx),
                                        Err(e) => {
                                            this.ui_error = Some(e);
                                            cx.notify();
                                        }
                                    }
                                })),
                        ),
                );
            }
        }
        for part in parts {
            let bone = part["bone"].as_u64().unwrap_or(0);
            let value = part["value"].as_f64().unwrap_or(0.0);
            let name = rpc_generated::enum_choices("SkeletonBone")
                .iter()
                .find(|(_, id)| *id == bone)
                .map(|(n, _)| *n)
                .unwrap_or("NONE");
            let group = proportions::GROUPS
                .iter()
                .find(|(_, ids)| ids.contains(&bone))
                .map(|(_, ids)| *ids);
            if mode == "ratios"
                && let Some((name, _)) = proportions::GROUPS
                    .iter()
                    .find(|(_, ids)| ids.contains(&bone))
                && self.form_value(&format!("proportion-group-open-{name}"), "false") != "true"
            {
                continue;
            }
            let ratio_mode = mode == "ratios" && group.is_some();
            let total = group
                .map(|ids| {
                    skeleton["skeleton_parts"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|p| p["bone"].as_u64().is_some_and(|id| ids.contains(&id)))
                        .filter_map(|p| p["value"].as_f64())
                        .sum::<f64>()
                })
                .unwrap_or(1.0);
            let shown = if ratio_mode {
                value / total.max(0.001) * 100.0
            } else {
                value * 100.0
            };
            let key = format!("bone-{mode}-{bone}");
            page = page.child(
                div()
                    .h_flex()
                    .items_center()
                    .gap_3()
                    .p_3()
                    .bg(cx.theme().muted)
                    .rounded_lg()
                    .child(
                        div()
                            .flex_1()
                            .child(self.text(&format!("skeleton_bone-{name}"))),
                    )
                    .children([-1., -0.5].into_iter().map(|n| {
                        let precise = self.form_value("proportions-precise", "false") == "true";
                        let delta = if precise {
                            n
                        } else {
                            if n == -1. { -5. } else { -1. }
                        };
                        self.proportion_increment(
                            bone,
                            group.filter(|_| ratio_mode),
                            shown,
                            delta,
                            cx,
                        )
                    }))
                    .child(self.local_input(&key, &format!("{shown:.2}"), false, window, cx))
                    .children([0.5, 1.].into_iter().map(|n| {
                        let precise = self.form_value("proportions-precise", "false") == "true";
                        let delta = if precise {
                            n
                        } else {
                            if n == 1. { 5. } else { 1. }
                        };
                        self.proportion_increment(
                            bone,
                            group.filter(|_| ratio_mode),
                            shown,
                            delta,
                            cx,
                        )
                    }))
                    .child(if ratio_mode { "%" } else { "cm" })
                    .child(
                        Button::new(format!("save-{key}"))
                            .label(self.text("native-apply"))
                            .disabled(self.snapshot.connection != Connection::Connected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let v = this
                                    .form_value(
                                        &format!(
                                            "bone-{}-{bone}",
                                            this.form_value("proportions-mode", "ratios")
                                        ),
                                        &format!("{shown}"),
                                    )
                                    .parse::<f64>()
                                    .unwrap_or(f64::NAN)
                                    / 100.0;
                                let result = if ratio_mode {
                                    proportions::ratio(
                                        &this.read("SkeletonConfigResponse"),
                                        group.unwrap(),
                                        bone,
                                        v,
                                    )
                                } else {
                                    proportions::change(bone, v).map(|r| {
                                        vec![r, ("SkeletonConfigRequest".into(), json!({}))]
                                    })
                                };
                                match result {
                                    Ok(r) => this.batch(r, cx),
                                    Err(e) => {
                                        this.ui_error = Some(e);
                                        cx.notify();
                                    }
                                }
                            })),
                    ),
            );
        }
        page.into_any_element()
    }
    pub(super) fn calibration(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self.read("AutoBoneProcessStatusResponse");
        let epoch = self.autobone_result.clone().unwrap_or(Value::Null);
        let height = self.read("UserHeightRecordingStatusResponse");
        let busy = self.autobone_auto || (status["completed"] == false && !status.is_null());
        let can_apply = self.autobone_valid && !busy;
        let mut page = div()
            .v_flex()
            .gap_3()
            .mt_4()
            .child(div().font_bold().text_lg().child("AutoBone"))
            .child(self.text("onboarding-automatic_proportions-recording-steps"))
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .flex_wrap()
                    .child(
                        Button::new("autobone-record")
                            .label(self.text("native-record"))
                            .disabled(busy || self.snapshot.connection != Connection::Connected)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.autobone_auto = true;
                                this.autobone_valid = false;
                                this.autobone_result = None;
                                this.rpc("AutoBoneProcessRequest", json!({"process_type":1}), cx);
                            })),
                    )
                    .child(self.rpc_button(
                        "autobone-stop",
                        "native-stop",
                        "AutoBoneStopRecordingRequest",
                        json!({}),
                        cx,
                    ))
                    .child(
                        Button::new("autobone-cancel")
                            .label(self.text("native-cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.autobone_auto = false;
                                this.rpc("AutoBoneCancelRecordingRequest", json!({}), cx);
                            })),
                    )
                    .child(self.rpc_button(
                        "autobone-process",
                        "native-process",
                        "AutoBoneProcessRequest",
                        json!({"process_type":3}),
                        cx,
                    ))
                    .child(self.rpc_button(
                        "autobone-save",
                        "native-autobone-save",
                        "AutoBoneProcessRequest",
                        json!({"process_type":2}),
                        cx,
                    ))
                    .child(
                        Button::new("autobone-apply")
                            .label(self.text("native-autobone-apply"))
                            .primary()
                            .disabled(!can_apply)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.batch(
                                    vec![
                                        ("AutoBoneApplyRequest".into(), json!({})),
                                        ("SkeletonConfigRequest".into(), json!({})),
                                    ],
                                    cx,
                                );
                            })),
                    ),
            );
        if !status.is_null() {
            page = page.child(format!(
                "{} / {} · ETA {}s · {}",
                status["current"],
                status["total"],
                status["eta"],
                self.text(if status["completed"] == true {
                    if status["success"] == true {
                        "native-ok"
                    } else {
                        "native-warning"
                    }
                } else {
                    "native-operation-pending"
                })
            ));
        }
        if !epoch.is_null() {
            page = page.child(format!(
                "{} / {} · {:.6}",
                epoch["current_epoch"],
                epoch["total_epochs"],
                epoch["epoch_error"].as_f64().unwrap_or(0.0)
            ));
            if let Some(parts) = epoch["adjusted_skeleton_parts"].as_array() {
                for p in parts {
                    let name = rpc_generated::enum_choices("SkeletonBone")
                        .iter()
                        .find(|(_, v)| Some(*v) == p["bone"].as_u64())
                        .map(|(n, _)| *n)
                        .unwrap_or("NONE");
                    page = page.child(format!(
                        "{}: {:.1} cm",
                        self.text(&format!("skeleton_bone-{name}")),
                        p["value"].as_f64().unwrap_or(0.0) * 100.0
                    ));
                }
            }
        }
        page = page
            .child(
                div()
                    .mt_3()
                    .font_bold()
                    .child(self.text("native-height-calibration")),
            )
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(self.rpc_button(
                        "height-start",
                        "native-height-calibration",
                        "StartUserHeightCalibration",
                        json!({}),
                        cx,
                    ))
                    .child(self.rpc_button(
                        "height-cancel",
                        "native-height-cancel",
                        "CancelUserHeightCalibration",
                        json!({}),
                        cx,
                    )),
            );
        if !height.is_null() {
            let name = rpc_generated::enum_choices("UserHeightCalibrationStatus")
                .iter()
                .find(|(_, v)| Some(*v) == height["status"].as_u64())
                .map(|(n, _)| *n)
                .unwrap_or("NONE");
            page = page.child(format!(
                "{} · {:.1} cm",
                self.text(&format!("onboarding-user_height-calibration-{name}")),
                height["hmdHeight"].as_f64().unwrap_or(0.0) * 100.0
            ));
        }
        page.into_any_element()
    }
    pub(super) fn relaxed_poses(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut page = div().v_flex().gap_3();
        for (pose, name) in [(0, "standing"), (1, "sitting"), (2, "flat")] {
            page = page.child(
                div()
                    .h_flex()
                    .gap_3()
                    .items_center()
                    .child(self.text(&format!("native-relaxed-{name}")))
                    .child(self.rpc_button(
                        &format!("pose-save-{pose}"),
                        "native-pose-save",
                        "DetectStayAlignedRelaxedPoseRequest",
                        json!({"pose":pose}),
                        cx,
                    ))
                    .child(self.rpc_button(
                        &format!("pose-reset-{pose}"),
                        "native-pose-reset",
                        "ResetStayAlignedRelaxedPoseRequest",
                        json!({"pose":pose}),
                        cx,
                    )),
            );
        }
        page.child(self.rpc_button(
            "stay-aligned-enable",
            "native-enabled",
            "EnableStayAlignedRequest",
            json!({"enable":true}),
            cx,
        ))
        .into_any_element()
    }
}
impl SlimeView {
    pub(super) fn vr_mode(&self, cx: &mut Context<Self>) -> AnyElement {
        let value = self
            .overlay
            .value
            .clone()
            .unwrap_or_else(|| (*self.read("OverlayDisplayModeResponse")).clone());
        let visible = value["is_visible"].as_bool().unwrap_or(false);
        let mirrored = value["is_mirrored"].as_bool().unwrap_or(false);
        div()
            .v_flex()
            .gap_3()
            .child(
                Button::new("vr-mode-back")
                    .label(self.text("native-back"))
                    .on_click(cx.listener(|this, _, _, cx| this.go(this.return_page, cx))),
            )
            .child(self.resets(cx))
            .child(self.bvh(cx))
            .child(self.preview_controls(cx))
            .child(self.skeleton_panel(360., cx))
            .child(self.rpc_button(
                "overlay-visible",
                if visible {
                    "native-overlay-hide"
                } else {
                    "native-overlay-show"
                },
                "OverlayDisplayModeChangeRequest",
                json!({"is_visible":!visible}),
                cx,
            ))
            .child(self.rpc_button(
                "overlay-mirror",
                "native-overlay-mirror",
                "OverlayDisplayModeChangeRequest",
                json!({"is_mirrored":!mirrored}),
                cx,
            ))
            .into_any_element()
    }
}

impl SlimeView {
    pub(super) fn preview_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let toggles = self.read("SettingsResponse")["model_settings"]["toggles"].clone();
        let mut row = div().h_flex().gap_2().flex_wrap();
        for field in ["floor_clip", "skating_correction", "toe_snap", "foot_plant"] {
            let enabled = self
                .temporary_tweaks
                .get(field)
                .copied()
                .unwrap_or(toggles[field] == true);
            row = row.child(
                Button::new(format!("tmp-{field}"))
                    .small()
                    .label(self.text(&format!("native-tmp-{field}")))
                    .selected(enabled)
                    .disabled(self.snapshot.connection != Connection::Connected)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let mut value = json!({});
                        value[field] = json!(!enabled);
                        if this.client.rpc("LegTweaksTmpChange", value).is_ok() {
                            this.temporary_tweaks.insert(field.into(), !enabled);
                        }
                        cx.notify();
                    })),
            );
        }
        row.child(Button::new("tmp-clear").small().label(self.text("native-tmp-clear")).on_click(cx.listener(|this,_,_,cx|{this.rpc("LegTweaksTmpClear",json!({"floor_clip":true,"skating_correction":true,"toe_snap":true,"foot_plant":true}),cx);this.temporary_tweaks.clear();}))).into_any_element()
    }
}
impl SlimeView {
    pub(super) fn avatar_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h_flex()
            .gap_3()
            .child(
                Button::new("avatar-import")
                    .label(self.text("native-vrm-import"))
                    .on_click(cx.listener(|_this, _, _, cx| {
                        let future = cx.prompt_for_paths(PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: None,
                        });
                        cx.spawn(async move |this, cx| {
                            if let Ok(Ok(Some(paths))) = future.await
                                && let Some(path) = paths.first()
                            {
                                let path = path.clone();
                                let result =
                                    smol::unblock(move || slimevr_gpui::avatar::read(&path)).await;
                                let _ = this.update(cx, |this, cx| match result {
                                    Ok(json) => this.batch(
                                        vec![
                                            (
                                                "ChangeSettingsRequest".into(),
                                                json!({"vrm":{"vrm_json":json}}),
                                            ),
                                            ("SettingsRequest".into(), json!({})),
                                        ],
                                        cx,
                                    ),
                                    Err(error) => {
                                        this.ui_error = Some(error);
                                        cx.notify();
                                    }
                                });
                            }
                        })
                        .detach();
                    })),
            )
            .child(
                Button::new("avatar-clear")
                    .label(self.text("native-vrm-clear"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.batch(
                            vec![
                                (
                                    "ChangeSettingsRequest".into(),
                                    json!({"vrm":{"vrm_json":""}}),
                                ),
                                ("SettingsRequest".into(), json!({})),
                            ],
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }
}
