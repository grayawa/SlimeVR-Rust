use super::*;
use serde_json::{Value, json};
use slimevr_gpui::{firmware, json_form};
impl SlimeView {
    pub(super) fn http_job(
        &mut self,
        key: &str,
        work: impl FnOnce() -> Result<Value, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        if !self.http_pending.insert(key.into()) {
            return;
        }
        let key = key.to_owned();
        cx.spawn(async move |this, cx| {
            let result = smol::unblock(work).await;
            let _ = this.update(cx, |this, cx| {
                this.http_pending.remove(&key);
                match result {
                    Ok(mut value) => {
                        if key == "firmware-digest"
                            && let Some(digest) = value.as_str()
                        {
                            this.form_values
                                .insert("firmware-digest".into(), digest.into());
                        }
                        if key == "board-defaults" {
                            value["data"] = json_form::populate(&value["schema"], &value["data"]);
                        }
                        this.http.insert(key, value);
                    }
                    Err(error) => this.ui_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }
    fn tool_request(
        &mut self,
        key: &str,
        method: &str,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        let base = self.form_value("builder-base", "http://localhost:3000");
        let refs: Vec<_> = params
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let url = match firmware::tool_url(&base, path, &refs) {
            Ok(u) => u,
            Err(e) => {
                self.ui_error = Some(e);
                cx.notify();
                return;
            }
        };
        let method = method.to_owned();
        self.http_job(
            key,
            move || firmware::http(&method, &url, body.as_ref()),
            cx,
        );
    }
    fn update_firmware(&mut self, device: u8, cx: &mut Context<Self>) {
        let target = self
            .snapshot
            .feed
            .as_ref()
            .and_then(|f| f.trackers.iter().find(|t| t.key.device == device))
            .cloned();
        let Some(target) = target else {
            self.ui_error = Some(self.text("native-unavailable"));
            cx.notify();
            return;
        };
        if target.battery.is_some_and(|b| !(50..=200).contains(&b)) {
            self.ui_error = Some(self.text("native-firmware-battery"));
            cx.notify();
            return;
        }
        let official = self.form_value("firmware-mode", "official") == "official";
        let part = if official {
            let release = self
                .http
                .get("official-firmware")
                .cloned()
                .unwrap_or(Value::Null);
            if release["allowed"] != true {
                self.ui_error = Some(self.text("firmware_update-unavailable"));
                cx.notify();
                return;
            }
            let asset = &release["files"][target.board.to_string()];
            let version = release["version"]
                .as_str()
                .and_then(|v| semver::Version::parse(v).ok());
            let current = semver::Version::parse(target.firmware.trim_start_matches('v')).ok();
            if !version.zip(current).is_some_and(|(v, c)| v > c) {
                self.ui_error = Some(self.text("native-unavailable"));
                cx.notify();
                return;
            }
            json!({"url":asset["browser_download_url"],"digest":asset["digest"],"offset":0})
        } else {
            let url = self.form_value("firmware-url", "");
            let digest = self.form_value("firmware-digest", "");
            let offset = match self.form_value("firmware-offset", "0").parse::<u32>() {
                Ok(v) => v,
                Err(_) => {
                    self.ui_error = Some(self.text("native-invalid-number"));
                    cx.notify();
                    return;
                }
            };
            json!({"url":url,"digest":digest,"offset":offset})
        };
        if part["digest"].as_str().is_none_or(|s| s.is_empty()) {
            let Some(url) = part["url"].as_str().map(str::to_owned) else {
                self.ui_error = Some(self.text("native-unavailable"));
                cx.notify();
                return;
            };
            cx.spawn(async move|this,cx|{let digest=smol::unblock(move||firmware::digest_url(&url)).await;let _=this.update(cx,|this,cx|{match digest {Ok(digest)=>{let mut part=part;part["digest"]=json!(digest);this.confirmation=Some(("FirmwareUpdateRequest".into(),json!({"method":{"type":"OTAFirmwareUpdate","value":{"device_id":{"id":device},"firmware_part":part}}}),"".into()));},Err(error)=>this.ui_error=Some(error)}cx.notify();});}).detach();
        } else {
            self.confirmation = Some((
                "FirmwareUpdateRequest".into(),
                json!({"method":{"type":"OTAFirmwareUpdate","value":{"device_id":{"id":device},"firmware_part":part}}}),
                "".into(),
            ));
        }
        cx.notify();
    }
    fn builder_data(&self) -> Result<Value, String> {
        let value = self
            .http
            .get("board-defaults")
            .ok_or("Load board defaults first")?;
        let mut data = value["data"].clone();
        for field in json_form::fields(&value["schema"], &data) {
            if let Some(text) = self.form_values.get(&format!("builder{}", field.path)) {
                let parsed = json_form::parse(&field, text)?;
                if let Some(value) = data.pointer_mut(&field.path) {
                    *value = parsed;
                }
            }
        }
        json_form::validate(&value["schema"], &data)?;
        Ok(data)
    }
    fn flash_build(&mut self, serial: bool, cx: &mut Context<Self>) {
        let build = self
            .http
            .get("build-status")
            .or(self.http.get("build"))
            .cloned()
            .unwrap_or(Value::Null);
        if build["status"] != "DONE" {
            self.ui_error = Some(self.text("native-operation-pending"));
            cx.notify();
            return;
        }
        let storage = self.form_value("builder-storage", "http://localhost:9099");
        let files = build["files"].as_array().cloned().unwrap_or_default();
        let mut parts = Vec::new();
        for file in &files {
            let path = file["filePath"].as_str().unwrap_or("");
            let url = if path.starts_with("http://") || path.starts_with("https://") {
                path.to_owned()
            } else {
                format!("{}/{path}", storage.trim_end_matches('/'))
            };
            parts.push(json!({"url":url,"offset":file["offset"],"digest":file["digest"]}));
        }
        let method = if serial {
            let board = self.form_value("builder-board", "");
            let defaults = self
                .http
                .get("board-defaults")
                .cloned()
                .unwrap_or(Value::Null);
            json!({"type":"SerialFirmwareUpdate","value":{"device_id":{"port":self.form_value("serial-port","")},"needManualReboot":defaults["data"]["defaults"][board]["flashingRules"]["needManualReboot"].as_bool().unwrap_or(false),"ssid":self.form_value("wifi-ssid",""),"password":self.form_value("wifi-password",""),"firmware_part":parts}})
        } else {
            let device = self
                .form_value("firmware-device", "0")
                .parse::<u8>()
                .unwrap_or(0);
            let battery = self
                .snapshot
                .feed
                .as_ref()
                .and_then(|f| f.trackers.iter().find(|t| t.key.device == device))
                .and_then(|t| t.battery);
            if battery.is_some_and(|b| !(50..=200).contains(&b)) {
                self.ui_error = Some(self.text("native-firmware-battery"));
                cx.notify();
                return;
            }
            let index = files
                .iter()
                .position(|f| f["isFirmware"] == true)
                .ok_or("No application firmware in build");
            let Ok(index) = index else {
                self.ui_error = Some("No application firmware in build".into());
                cx.notify();
                return;
            };
            let mut part = parts[index].clone();
            part["offset"] = json!(0);
            json!({"type":"OTAFirmwareUpdate","value":{"device_id":{"id":device},"firmware_part":part}})
        };
        self.confirmation = Some((
            "FirmwareUpdateRequest".into(),
            json!({"method":method}),
            "".into(),
        ));
        cx.notify();
    }
    pub(super) fn firmware_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut page = div()
            .v_flex()
            .gap_4()
            .child(
                div()
                    .font_bold()
                    .text_lg()
                    .child(self.text("native-firmware")),
            )
            .child(
                Button::new("firmware-official")
                    .label(self.text("native-firmware-refresh"))
                    .disabled(self.http_pending.contains("official-firmware"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let uuid = this.preferences.value["uuid"]
                            .as_str()
                            .unwrap_or("")
                            .to_owned();
                        this.http_job("official-firmware", move || firmware::official(&uuid), cx);
                    })),
            )
            .child(
                div()
                    .h_flex()
                    .gap_3()
                    .children(["official", "custom"].into_iter().map(|mode| {
                        Button::new(format!("fw-mode-{mode}"))
                            .label(self.text(&format!("native-firmware-{mode}")))
                            .selected(self.form_value("firmware-mode", "official") == mode)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.form_values.insert("firmware-mode".into(), mode.into());
                                cx.notify();
                            }))
                    })),
            );
        if let Some(release) = self.http.get("official-firmware") {
            page = page
                .child(format!(
                    "{} · {}",
                    release["name"].as_str().unwrap_or(""),
                    release["version"].as_str().unwrap_or("")
                ))
                .child(release["changelog"].as_str().unwrap_or("").to_owned());
        }
        if self.form_value("firmware-mode", "official") == "custom" {
            for (key, label, default) in [
                ("firmware-url", "native-firmware-url", ""),
                ("firmware-digest", "native-firmware-digest", ""),
                ("firmware-offset", "native-firmware-offset", "0"),
            ] {
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(div().flex_1().child(self.text(label)))
                        .child(self.local_input(key, default, false, window, cx)),
                );
            }
            page = page.child(
                Button::new("firmware-calculate-digest")
                    .label(self.text("native-firmware-checksum"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        let url = this.form_value("firmware-url", "");
                        this.http_job(
                            "firmware-digest",
                            move || firmware::digest_url(&url).map(|v| json!(v)),
                            cx,
                        );
                    })),
            );
        }
        let mut devices = std::collections::BTreeSet::new();
        let trackers = self
            .snapshot
            .feed
            .as_ref()
            .map(|f| f.trackers.clone())
            .unwrap_or_default();
        for t in trackers {
            if t.key.device == 0 || !devices.insert(t.key.device) {
                continue;
            }
            let id = t.key.device;
            page = page.child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(div().flex_1().child(format!("{} · {}", t.name, t.firmware)))
                    .child(
                        Button::new(format!("select-fw-{id}"))
                            .label(self.text("native-firmware-device"))
                            .selected(self.form_value("firmware-device", "0") == id.to_string())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.form_values
                                    .insert("firmware-device".into(), id.to_string());
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(format!("update-{id}"))
                            .label(self.text("native-firmware-start"))
                            .disabled(self.snapshot.connection != Connection::Connected)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.update_firmware(id, cx)),
                            ),
                    ),
            );
        }
        page = page.child(self.rpc_button(
            "firmware-stop",
            "native-firmware-stop",
            "FirmwareUpdateStopQueuesRequest",
            json!({}),
            cx,
        ));
        for (id, status) in &self.firmware_status {
            let name = slimevr_gpui::rpc_generated::enum_choices("FirmwareUpdateStatus")
                .iter()
                .find(|(_, v)| Some(*v) == status["status"].as_u64())
                .map(|(n, _)| *n)
                .unwrap_or("NONE");
            page = page.child(format!(
                "{id}: {} · {}%",
                self.text(&format!("firmware_update-status-{name}")),
                status["progress"]
            ));
        }
        page = page.child(
            div()
                .mt_4()
                .font_bold()
                .text_lg()
                .child(self.text("settings-sidebar-firmware-tool")),
        );
        for (key, label, default) in [
            (
                "builder-base",
                "native-firmware-service",
                "http://localhost:3000",
            ),
            (
                "builder-storage",
                "native-firmware-storage",
                "http://localhost:9099",
            ),
        ] {
            page = page.child(
                div()
                    .h_flex()
                    .gap_3()
                    .child(div().flex_1().child(self.text(label)))
                    .child(self.local_input(key, default, false, window, cx)),
            );
        }
        page = page.child(
            Button::new("builder-load")
                .label(self.text("native-firmware-check"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.tool_request("builder-health", "GET", "health", vec![], None, cx);
                    this.tool_request(
                        "builder-sources",
                        "GET",
                        "firmware/sources",
                        vec![],
                        None,
                        cx,
                    );
                })),
        );
        if let Some(sources) = self
            .http
            .get("builder-sources")
            .and_then(|v| v.as_array())
            .cloned()
        {
            for source in sources {
                let name = source["source"].as_str().unwrap_or("").to_owned();
                let version = source["version"].as_str().unwrap_or("").to_owned();
                let boards = source["availableBoards"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                page = page.child(div().h_flex().gap_3().child(format!("{name} · {version}")));
                let mut row = div().h_flex().flex_wrap().gap_2();
                for board in boards {
                    let Some(board) = board.as_str().map(str::to_owned) else {
                        continue;
                    };
                    let name = name.clone();
                    let version = version.clone();
                    row = row.child(
                        Button::new(format!("board-{name}-{version}-{board}"))
                            .label(board.clone())
                            .disabled(self.http_pending.contains("board-defaults"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.form_values
                                    .insert("builder-source".into(), name.clone());
                                this.form_values
                                    .insert("builder-version".into(), version.clone());
                                this.form_values
                                    .insert("builder-board".into(), board.clone());
                                this.form_values.retain(|k, _| !k.starts_with("builder/"));
                                this.tool_request(
                                    "board-defaults",
                                    "GET",
                                    "firmware/board-defaults",
                                    vec![
                                        ("source".into(), name.clone()),
                                        ("version".into(), version.clone()),
                                        ("board".into(), board.clone()),
                                    ],
                                    None,
                                    cx,
                                );
                            })),
                    );
                }
                page = page.child(row);
            }
        }
        if let Some(defaults) = self.http.get("board-defaults").cloned() {
            for field in json_form::fields(&defaults["schema"], &defaults["data"]) {
                let key = format!("builder{}", field.path);
                let mut row = div()
                    .h_flex()
                    .gap_3()
                    .items_center()
                    .child(div().flex_1().child(field.label.clone()));
                if field.kind == "variant" {
                    let current = field.value.as_u64().unwrap_or(0) as usize;
                    let label = field
                        .choices
                        .get(current)
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_owned();
                    let view = cx.entity().downgrade();
                    let path = field.path.clone();
                    let choices = field.choices.clone();
                    let variants = field.schema["x-native-variants"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    row = row.child(Button::new(key.clone()).label(label).dropdown_menu(
                        move |mut menu, _, _| {
                            for (index, label) in choices.iter().enumerate() {
                                let view = view.clone();
                                let path = path.clone();
                                let value = variants[index].clone();
                                menu = menu.item(
                                    PopupMenuItem::new(label.as_str().unwrap_or("").to_owned())
                                        .on_click(move |_, _, cx| {
                                            let _ = view.update(cx, |this, cx| {
                                                if let Some(defaults) =
                                                    this.http.get_mut("board-defaults")
                                                    && let Some(target) =
                                                        defaults["data"].pointer_mut(&path)
                                                {
                                                    *target = value.clone();
                                                }
                                                this.form_values.retain(|k, _| {
                                                    !k.starts_with(&format!("builder{path}"))
                                                });
                                                cx.notify();
                                            });
                                        }),
                                );
                            }
                            menu
                        },
                    ));
                } else if field.kind == "array" {
                    let path = field.path.clone();
                    let item_schema = field.schema["items"].clone();
                    row = row
                        .child(
                            Button::new(format!("add-{key}"))
                                .label("+")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(defaults) = this.http.get_mut("board-defaults")
                                        && let Some(array) = defaults["data"]
                                            .pointer_mut(&path)
                                            .and_then(Value::as_array_mut)
                                    {
                                        array.push(json_form::default_value(&item_schema));
                                    }
                                    cx.notify();
                                })),
                        )
                        .child(Button::new(format!("remove-{key}")).label("−").on_click(
                            cx.listener(move |this, _, _, cx| {
                                if let Some(defaults) = this.http.get_mut("board-defaults")
                                    && let Some(array) = defaults["data"]
                                        .pointer_mut(&field.path)
                                        .and_then(Value::as_array_mut)
                                {
                                    array.pop();
                                    this.form_values.retain(|k, _| {
                                        !k.starts_with(&format!("builder{}", field.path))
                                    });
                                }
                                cx.notify();
                            }),
                        ));
                } else if field.kind == "boolean" {
                    let enabled = self
                        .form_value(&key, if field.value == true { "true" } else { "false" })
                        == "true";
                    row = row.child(
                        Button::new(key.clone())
                            .label(self.text(if enabled {
                                "native-enabled"
                            } else {
                                "native-disabled"
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.form_values.insert(key.clone(), (!enabled).to_string());
                                cx.notify();
                            })),
                    );
                } else if !field.choices.is_empty() {
                    let current = self.form_value(&key, field.value.as_str().unwrap_or(""));
                    let view = cx.entity().downgrade();
                    let choices = field.choices.clone();
                    row = row.child(Button::new(key.clone()).label(current).dropdown_menu(
                        move |mut menu, _, _| {
                            for choice in &choices {
                                let value = choice
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| choice.to_string());
                                let view = view.clone();
                                let key = key.clone();
                                menu = menu.item(PopupMenuItem::new(value.clone()).on_click(
                                    move |_, _, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.form_values.insert(key.clone(), value.clone());
                                            cx.notify();
                                        });
                                    },
                                ));
                            }
                            menu
                        },
                    ));
                } else {
                    let text = field.value.as_str().map(str::to_owned).unwrap_or_else(|| {
                        if field.value.is_null() {
                            "".into()
                        } else {
                            field.value.to_string()
                        }
                    });
                    row = row.child(self.local_input(&key, &text, false, window, cx));
                }
                page = page.child(row);
            }
            page=page.child(Button::new("builder-build").label(self.text("native-firmware-build")).primary().disabled(self.http_pending.contains("build")).on_click(cx.listener(|this,_,_,cx|{
                match this.builder_data(){Ok(data)=>{let board=this.form_value("builder-board","");let body=json!({"source":this.form_value("builder-source",""),"version":this.form_value("builder-version",""),"board":board,"values":data["defaults"][board]});this.http.remove("build-status");this.tool_request("build","POST","firmware/build",vec![],Some(body),cx);},Err(e)=>{this.ui_error=Some(e);cx.notify();}}
            }))) ;
        }
        if let Some(build) = self
            .http
            .get("build-status")
            .or(self.http.get("build"))
            .cloned()
        {
            let status = build["status"].as_str().unwrap_or("");
            let id = build["id"].as_str().unwrap_or("");
            page = page.child(self.text(&format!("firmware_tool-build-{status}")));
            if !matches!(status, "DONE" | "ERROR")
                && !id.is_empty()
                && !self.http_pending.contains("build-status")
                && std::time::Instant::now() >= self.next_build_poll
            {
                self.next_build_poll = std::time::Instant::now() + Duration::from_secs(1);
                self.tool_request(
                    "build-status",
                    "GET",
                    &format!("firmware/{id}"),
                    vec![],
                    None,
                    cx,
                );
            }
            if status == "DONE" {
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(
                            Button::new("build-flash-ota").label("OTA").on_click(
                                cx.listener(|this, _, _, cx| this.flash_build(false, cx)),
                            ),
                        )
                        .child(
                            Button::new("build-flash-serial")
                                .label(self.text("native-serial-port"))
                                .on_click(cx.listener(|this, _, _, cx| this.flash_build(true, cx))),
                        ),
                );
                page = page.child(self.serial(window, cx));
            }
        }
        page.into_any_element()
    }
}
