use super::*;
use gpui_kit::component::{ActiveTheme, Theme};
use serde_json::{Value, json};
use slimevr_gpui::{locales, rpc_generated, theme::apply_theme};
impl SlimeView {
    pub(super) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Keep the displayed confirmation active during repeated close requests.
        if self.exit_confirm {
            return;
        }
        if self.preferences.value["useTray"] == true
            && self.tray.as_ref().is_some_and(|tray| tray.available())
        {
            slimevr_gpui::tray::visible(window, false);
        } else {
            self.request_exit(cx);
        }
    }
    pub(super) fn finish_exit(&mut self, cx: &mut Context<Self>) {
        self.onboarding_stop_wifi();
        self.allow_exit = true;
        cx.quit();
    }
    pub(super) fn request_exit(&mut self, cx: &mut Context<Self>) {
        if slimevr_gpui::exit_warning::should_warn(
            self.preferences.value["connectedTrackersWarning"] == true,
            &self.snapshot,
        ) {
            self.exit_confirm = true;
            // Close callbacks already borrow the current window. Restore
            // visibility and focus after that callback returns.
            let entity = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = entity.update(cx, |this, cx| {
                    if !this.exit_confirm {
                        return;
                    }
                    let focus = this.focus.clone();
                    for handle in cx.windows() {
                        let _ = handle.update(cx, |_, window, cx| {
                            slimevr_gpui::tray::visible(window, true);
                            window.activate_window();
                            window.focus(&focus, cx);
                        });
                    }
                });
            });
            cx.notify();
        } else {
            self.finish_exit(cx);
        }
    }

    pub(super) fn exit_dialog(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let description = self
            .text("trackers_still_on-modal-description")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        let width = (f32::from(window.viewport_size().width) - 32.).min(370.);
        let body = div()
            .id("exit-dialog-body")
            .v_flex()
            .w(px(width))
            .p_6()
            .gap_3()
            .rounded_lg()
            .bg(cx.theme().popover)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .v_flex()
                    .gap_2()
                    .text_center()
                    .child(
                        div()
                            .text_2xl()
                            .font_bold()
                            .child(self.text("trackers_still_on-modal-title")),
                    )
                    .child(
                        div().v_flex().children(
                            self.settings_text_lines(description, width - 24., window)
                                .into_iter()
                                .map(|line| div().child(line)),
                        ),
                    ),
            )
            .child(
                Button::new("confirm-exit")
                    .primary()
                    .w_full()
                    .h(px(40.))
                    .label(self.text("trackers_still_on-modal-confirm"))
                    .on_click(cx.listener(|this, _, _, cx| this.finish_exit(cx))),
            )
            .child(
                Button::new("cancel-exit")
                    .ghost()
                    .bg(cx.theme().border.opacity(0.35))
                    .w_full()
                    .h(px(40.))
                    .label(self.text("trackers_still_on-modal-cancel"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.exit_confirm = false;
                        cx.notify();
                    })),
            );
        div()
            .id("exit-dialog")
            .absolute()
            .inset_0()
            .v_flex()
            .items_center()
            .justify_center()
            .bg(cx.theme().background.opacity(0.6))
            .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| {
                this.exit_confirm = false;
                cx.stop_propagation();
                cx.notify();
            }))
            .child(body)
            .into_any_element()
    }
    pub(super) fn preference(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let escaped = key.trim_start_matches('/');
        let parts: Vec<_> = escaped.split('/').collect();
        if parts.len() == 1 {
            self.preferences.value[parts[0]] = value;
        } else {
            let mut v = &mut self.preferences.value;
            for key in &parts[..parts.len() - 1] {
                if v[*key].is_null() {
                    v[*key] = json!({});
                }
                v = &mut v[*key];
            }
            v[parts[parts.len() - 1]] = value;
        }
        self.ui_error = self.preferences.save().err();
        if key == "devSettings/highContrast" {
            self.apply_theme(cx);
            if self.preferences.value["devSettings"]["highContrast"] == true {
                Theme::update(cx, |t| {
                    t.colors.foreground = rgb(0xffffff).into();
                    t.colors.background = rgb(0x000000).into();
                    t.colors.muted = rgb(0x252525).into();
                });
            }
        }
        cx.notify();
    }
    pub(super) fn pref_toggle(&self, key: &str, label: &str, cx: &mut Context<Self>) -> AnyElement {
        let path = format!("/{}", key.trim_start_matches('/'));
        let enabled = self
            .preferences
            .value
            .pointer(&path)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let key = key.to_owned();
        div()
            .h_flex()
            .gap_3()
            .justify_between()
            .items_center()
            .p_3()
            .bg(cx.theme().muted)
            .rounded_lg()
            .child(div().flex_1().child(self.text(label)))
            .child(
                Switch::new(format!("pref-{key}"))
                    .checked(enabled)
                    .small()
                    .accessibility_label(self.text(label))
                    .on_click(cx.listener(move |this, enabled, _, cx| {
                        this.preference(&key, json!(*enabled), cx)
                    })),
            )
            .into_any_element()
    }
    pub(super) fn apply_theme(&self, cx: &mut Context<Self>) {
        apply_theme(
            self.preferences.value["theme"].as_str().unwrap_or("slime"),
            cx,
        );
        if self.preferences.value["devSettings"]["highContrast"] == true {
            Theme::update(cx, |t| {
                t.colors.foreground = rgb(0xffffff).into();
                t.colors.background = rgb(0x000000).into();
                t.colors.muted = rgb(0x252525).into();
            });
        }
    }
    pub(super) fn interface_page(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if section == Section::Home {
            return self.home_settings_pane(cx);
        }
        if matches!(
            section,
            Section::Notifications | Section::Behavior | Section::Appearance
        ) {
            let pane = slimevr_gpui::settings_layout::pane(section).unwrap();
            let content = self.layout_nodes(&pane["children"], window, cx);
            return self.settings_pane("", pane["icon"].as_str().unwrap_or("Wrench"), content, cx);
        }
        let mut page = div().v_flex().gap_4();
        match section {
            Section::Notifications => {
                for (key, label) in [
                    (
                        "watchNewDevices",
                        "settings-general-interface-serial_detection-label",
                    ),
                    (
                        "feedbackSound",
                        "settings-general-interface-feedback_sound-label",
                    ),
                    (
                        "connectedTrackersWarning",
                        "settings-general-interface-connected_trackers_warning-label",
                    ),
                ] {
                    page = page.child(self.pref_toggle(key, label, cx));
                }
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(self.text("settings-general-interface-feedback_sound-volume"))
                        .child(self.local_input(
                            "sound-volume",
                            &format!("{}", self.preferences.value["feedbackSoundVolume"]),
                            false,
                            window,
                            cx,
                        ))
                        .child(
                            Button::new("save-sound-volume")
                                .label(self.text("native-apply"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    match this.form_value("sound-volume", "0.5").parse::<f64>() {
                                        Ok(v) if (0.0..=1.0).contains(&v) => {
                                            this.preference("feedbackSoundVolume", json!(v), cx)
                                        }
                                        _ => {
                                            this.ui_error = Some(this.text("native-invalid-ratio"));
                                            cx.notify();
                                        }
                                    }
                                })),
                        ),
                );
            }
            Section::Behavior => {
                page = page.child(self.text("native-privacy-local"));
                for (key, label) in [
                    ("debug", "settings-general-interface-dev_mode-label"),
                    ("useTray", "settings-general-interface-use_tray-label"),
                    (
                        "discordPresence",
                        "settings-general-interface-discord_presence-label",
                    ),
                ] {
                    page = page.child(self.pref_toggle(key, label, cx));
                }
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(self.text("settings-interface-behavior-bvh_directory-label"))
                        .child(
                            Button::new("bvh-folder")
                                .label(
                                    self.preferences.value["bvhDirectory"]
                                        .as_str()
                                        .unwrap_or("…")
                                        .to_owned(),
                                )
                                .on_click(cx.listener(|_this, _, _, cx| {
                                    let future = cx.prompt_for_paths(PathPromptOptions {
                                        files: false,
                                        directories: true,
                                        multiple: false,
                                        prompt: None,
                                    });
                                    cx.spawn(async move |this, cx| {
                                        if let Ok(Ok(Some(paths))) = future.await
                                            && let Some(path) = paths.first()
                                        {
                                            let _ = this.update(cx, |this, cx| {
                                                this.preference(
                                                    "bvhDirectory",
                                                    json!(path.to_string_lossy()),
                                                    cx,
                                                )
                                            });
                                        }
                                    })
                                    .detach();
                                })),
                        )
                        .child(
                            Button::new("clear-bvh-folder")
                                .label(self.text("native-clear"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.preference("bvhDirectory", Value::Null, cx)
                                })),
                        ),
                );
            }
            Section::Appearance => {
                page = page.child(self.text("settings-general-interface-theme"));
                let mut themes = div().h_flex().gap_2().flex_wrap();
                for name in [
                    "slime",
                    "slime-green",
                    "slime-yellow",
                    "slime-orange",
                    "slime-red",
                    "dark",
                    "light",
                    "trans",
                    "asexual",
                    "snep",
                ] {
                    let color = match name {
                        "slime-green" => 0x69ba82,
                        "slime-yellow" => 0xd7b65c,
                        "slime-orange" => 0xd49860,
                        "slime-red" => 0xc96f7c,
                        "dark" => 0x252525,
                        "light" => 0xf2f2f2,
                        "trans" => 0x83cbdc,
                        "asexual" => 0x9773b9,
                        "snep" => 0x8babb6,
                        _ => 0x65459a,
                    };
                    themes = themes.child(
                        Button::new(format!("theme-{name}"))
                            .ghost()
                            .size(px(46.))
                            .rounded_full()
                            .tooltip(self.text(&format!("native-theme-{name}")))
                            .selected(self.preferences.value["theme"] == name)
                            .child(div().size(px(30.)).rounded_full().bg(rgb(color)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preference("theme", json!(name), cx);
                                this.apply_theme(cx);
                            })),
                    );
                }
                page = page.child(themes);
                let locale = self.preferences.value["lang"]
                    .as_str()
                    .unwrap_or("zh-Hans")
                    .to_owned();
                let view = cx.entity().downgrade();
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(self.text("settings-general-interface-lang"))
                        .child(Button::new("language").label(locale).dropdown_menu(
                            move |mut menu, _, _| {
                                for locale in locales::LOCALES {
                                    let view = view.clone();
                                    menu =
                                        menu.item(PopupMenuItem::new(locale.to_string()).on_click(
                                            move |_, _, cx| {
                                                let _ = view.update(cx, |this, cx| {
                                                    match Localizer::new(locale) {
                                                        Ok(mut l10n) => {
                                                            if let Ok(source) =
                                                                std::fs::read_to_string(
                                                                    this.paths
                                                                        .root
                                                                        .join("override.ftl"),
                                                                )
                                                            {
                                                                let _ = l10n.add_override(&source);
                                                            }
                                                            this.l10n = l10n;
                                                            if let Some(tray) = &this.tray {
                                                                tray.labels([
                                                                    this.text("tray_menu-show"),
                                                                    this.text("tray_menu-hide"),
                                                                    this.text("tray_menu-quit"),
                                                                ]);
                                                            }
                                                            this.preference(
                                                                "lang",
                                                                json!(locale),
                                                                cx,
                                                            );
                                                            gpui_kit::component::set_locale(
                                                                if *locale == "zh-Hans" {
                                                                    "zh-CN"
                                                                } else {
                                                                    locale
                                                                },
                                                            );
                                                        }
                                                        Err(e) => this.ui_error = Some(e),
                                                    }
                                                    cx.notify();
                                                });
                                            },
                                        ));
                                }
                                menu
                            },
                        )),
                );
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(self.text("settings-interface-appearance-font_size"))
                        .child(
                            div()
                                .w(px(300.))
                                .child(gpui_kit::component::slider::Slider::new(&self.font_slider)),
                        )
                        .child(self.preferences.value["textSize"].to_string()),
                );
                let view = cx.entity().downgrade();
                page = page.child(
                    div()
                        .h_flex()
                        .gap_3()
                        .child(self.text("settings-interface-appearance-font"))
                        .child(
                            Button::new("font-select")
                                .w(px(240.))
                                .label(
                                    self.preferences.value["fonts"][0]
                                        .as_str()
                                        .unwrap_or("Poppins")
                                        .to_owned(),
                                )
                                .dropdown_menu(move |mut menu, _, _| {
                                    for font in [
                                        "System",
                                        "Poppins",
                                        "Noto Sans",
                                        "Lexend",
                                        "Ubuntu",
                                        "OpenDyslexic",
                                    ] {
                                        let view = view.clone();
                                        menu = menu.item(PopupMenuItem::new(font).on_click(
                                            move |_, _, cx| {
                                                let _ = view.update(cx, |this, cx| {
                                                    this.preference("fonts", json!([font]), cx)
                                                });
                                            },
                                        ));
                                    }
                                    menu
                                }),
                        ),
                );
                page = div()
                    .v_flex()
                    .gap_6()
                    .p_6()
                    .rounded_lg()
                    .bg(cx.theme().popover)
                    .child(
                        div()
                            .h_flex()
                            .gap_4()
                            .child(
                                svg()
                                    .path("slime/Gear.svg")
                                    .size(px(32.))
                                    .text_color(cx.theme().primary),
                            )
                            .child(
                                div()
                                    .text_2xl()
                                    .font_bold()
                                    .child(self.text("settings-interface-appearance")),
                            ),
                    )
                    .child(page);
            }
            Section::Home => {
                page = page
                    .child(self.pref_toggle(
                        "skeletonPreview",
                        "settings-home-skeleton_preview-label",
                        cx,
                    ))
                    .child(self.pref_toggle("mirrorView", "native-camera-mirror", cx));
                page = page.child(
                    div().h_flex().gap_3().children(
                        [
                            ("default", "settings-home-list-layout-grid"),
                            ("table", "settings-home-list-layout-table"),
                        ]
                        .into_iter()
                        .map(|(layout, label)| {
                            Button::new(format!("home-{layout}"))
                                .label(self.text(label))
                                .selected(self.preferences.value["homeLayout"] == layout)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.preference("homeLayout", json!(layout), cx)
                                }))
                        }),
                    ),
                );
            }
            Section::Checklist => return self.checklist_settings(cx),
            _ => (),
        }
        if self.preferences.value["debug"] == true && matches!(section, Section::Behavior) {
            for (key, label) in [
                ("highContrast", "high_contrast"),
                ("preciseRotation", "precise_rotation"),
                ("fastDataFeed", "fast_data_feed"),
                ("filterSlimesAndHMD", "filter_slimes_and_hmd"),
                ("sortByName", "sort_by_name"),
                ("rawSlimeRotation", "raw_slime_rotation"),
                ("moreInfo", "more_info"),
            ] {
                page = page.child(self.pref_toggle(
                    &format!("devSettings/{key}"),
                    &format!("settings-general-interface-dev_mode-{label}"),
                    cx,
                ));
            }
        }
        page.into_any_element()
    }
    pub(super) fn advanced_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let original = slimevr_gpui::settings_layout::pane(Section::Advanced).unwrap();
        let original_content = self.layout_nodes(&original["children"], window, cx);
        let original_pane = self.settings_pane("", "Bug", original_content, cx);
        let mut page = div().v_flex().gap_4().child(
            div()
                .h_flex()
                .gap_3()
                .child(
                    Button::new("logs-open")
                        .label(self.text("native-logs-open"))
                        .on_click(
                            cx.listener(|this, _, _, cx| cx.open_with_system(&this.paths.logs)),
                        ),
                )
                .child(
                    Button::new("config-open")
                        .label(self.text("native-config-open"))
                        .on_click(
                            cx.listener(|this, _, _, cx| cx.open_with_system(&this.paths.root)),
                        ),
                )
                .child(
                    Button::new("copy-diagnostics")
                        .label(self.text("native-copy-diagnostics"))
                        .on_click(cx.listener(|this, _, _, cx| {
                            let summary = format!(
                                "SlimeVR GPUI {}\n{:?}\n{}\n{}",
                                env!("CARGO_PKG_VERSION"),
                                this.snapshot.connection,
                                this.snapshot
                                    .diagnostics
                                    .iter()
                                    .cloned()
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                                this.paths.logs.display()
                            );
                            cx.write_to_clipboard(ClipboardItem::new_string(summary));
                        })),
                ),
        );
        page=page.child(self.rpc_button("mag-all","native-magnetometer","ChangeMagToggleRequest",json!({"enable":!self.read("MagToggleResponse")["enable"].as_bool().unwrap_or(false)}),cx))
            .child(self.rpc_button("clear-drift","native-drift-clear","ClearDriftCompensationRequest",json!({}),cx))
            .child(Button::new("reset-settings").label(self.text("native-settings-reset")).on_click(cx.listener(|this,_,_,cx|{this.confirmation=Some(("SettingsResetRequest".into(),json!({}),"SettingsRequest".into()));cx.notify();})))
            .child(div().font_bold().child(self.text("native-keybind")));
        let keybinds = self.read("KeybindResponse")["keybind"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for keybind in keybinds {
            let id = keybind["keybind_id"].as_u64().unwrap_or(0);
            let name = rpc_generated::enum_choices("KeybindId")
                .iter()
                .find(|(_, v)| *v == id)
                .map(|(n, _)| *n)
                .unwrap_or("FULL_RESET");
            let key = format!("keybind-{id}");
            let delay = format!("keybind-delay-{id}");
            page=page.child(div().h_flex().gap_3().items_center().child(div().flex_1().child(self.text(keybind["keybind_name_id"].as_str().unwrap_or(name))))
                .child(self.local_input(&key,keybind["keybind_value"].as_str().unwrap_or(""),false,window,cx))
                .child(self.local_input(&delay,&keybind["keybind_delay"].to_string(),false,window,cx)).child("s")
                .child(Button::new(format!("capture-{id}")).label(self.text("native-keybind-capture")).on_click(cx.listener(move|this,_,window,cx|{this.key_capture=Some(id);this.focus.focus(window,cx);cx.notify();})))
                .child(Button::new(format!("save-{key}")).label(self.text("native-keybind-save")).on_click(cx.listener(move|this,_,_,cx|{
                    let value=this.form_value(&format!("keybind-{id}"),"");let delay=this.form_value(&format!("keybind-delay-{id}"),"0").parse::<f64>().unwrap_or(f64::NAN);
                    if !delay.is_finite()||delay<0.0{this.ui_error=Some(this.text("native-invalid-number"));cx.notify();return;}
                    this.rpc("ChangeKeybindRequest",json!({"keybind":{"keybind_id":id,"keybind_value":value,"keybind_delay":delay}}),cx);
                }))));
        }
        page = page
            .child(self.settings_extra_fields(Section::Advanced, window, cx))
            .child(div().font_bold().child(self.text("native-diagnostics")));
        for line in &self.snapshot.diagnostics {
            page = page.child(div().text_sm().child(line.clone()));
        }
        div()
            .v_flex()
            .gap_2()
            .child(original_pane)
            .child(self.settings_pane(
                "native-advanced-settings",
                "Wrench",
                page.into_any_element(),
                cx,
            ))
            .child(self.settings_save_bar(cx))
            .into_any_element()
    }
}
