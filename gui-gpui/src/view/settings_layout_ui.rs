use super::*;
use gpui_kit::component::{WindowExt, dialog::DialogButtonProps};
use serde_json::{Value, json};
use slimevr_gpui::{
    rpc_generated,
    settings::{self, Field},
    settings_layout,
    ui::components::{ChoiceCard, NumberSelector, SettingsPane, SwitchRow},
};

impl SlimeView {
    // Wrap descriptions at the pane width using shaped glyph advances.
    pub(super) fn settings_text_lines(
        &self,
        text: String,
        width: f32,
        window: &Window,
    ) -> Vec<String> {
        let mut style = window.text_style();
        style.font_family = slimevr_gpui::desktop::font_family(&self.preferences.value).into();
        let size = px(self.preferences.value["textSize"].as_f64().unwrap_or(12.) as f32);
        if let Ok(shaped) = window.text_system().shape_text(
            text.clone().into(),
            size,
            &[style.to_run(text.len())],
            Some(px((width - 24.).max(80.))),
            None,
        ) {
            let mut result = Vec::new();
            for line in shaped {
                let mut start = 0;
                for end in line
                    .wrap_boundaries
                    .iter()
                    .map(|b| line.unwrapped_layout.runs[b.run_ix].glyphs[b.glyph_ix].index)
                    .chain([line.text.len()])
                {
                    result.push(line.text[start..end].to_owned());
                    start = end;
                }
            }
            result
        } else {
            text.lines().map(str::to_owned).collect()
        }
    }
    pub(super) fn settings_pane(
        &self,
        title: &str,
        icon: &str,
        content: AnyElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        SettingsPane::new(
            svg()
                .path(format!("slime/{icon}.svg"))
                .size(px(22.))
                .text_color(cx.theme().foreground),
            content,
        )
        .title(if title.is_empty() {
            String::new()
        } else {
            self.text(title)
        })
        .build(cx)
        .into_any_element()
    }
    pub(super) fn home_settings_pane(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut choices = div().h_flex().gap_4();
        for (layout, label, columns) in [
            ("default", "settings-home-list-layout-grid", 2),
            ("table", "settings-home-list-layout-table", 1),
        ] {
            let active = self.preferences.value["homeLayout"] == layout;
            let mut bars = div().flex().flex_wrap().gap_2().p_2().w_full();
            for _ in 0..4 {
                bars = bars.child(
                    div()
                        .h(px(8.))
                        .w(relative(if columns == 2 { 0.45 } else { 1. }))
                        .rounded_lg()
                        .bg(cx.theme().muted_foreground),
                );
            }
            choices = choices.child(
                Button::new(format!("home-layout-{layout}"))
                    .ghost()
                    .w(px(160.))
                    .h(px(96.))
                    .rounded_lg()
                    .p_0()
                    .border_2()
                    .border_color(if active {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .child(
                        div()
                            .v_flex()
                            .w_full()
                            .child(div().p_2().child(self.text(label)))
                            .child(div().h(px(2.)).w_full().bg(if active {
                                cx.theme().primary
                            } else {
                                cx.theme().border
                            }))
                            .child(bars),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.preference("homeLayout", json!(layout), cx)
                    })),
            );
        }
        self.settings_pane(
            "home-settings",
            "Home",
            div()
                .v_flex()
                .gap_3()
                .child(
                    div()
                        .font_bold()
                        .child(self.text("settings-home-list-layout")),
                )
                .child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(self.text("settings-home-list-layout-desc")),
                )
                .child(choices)
                .into_any_element(),
            cx,
        )
    }
    pub(super) fn settings_form(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.snapshot.rpc.contains_key("SettingsResponse") {
            return div().child(self.text("native-waiting")).into_any_element();
        }
        let mut page = div().v_flex().w_full().gap_2();
        if let Some(pane) = settings_layout::pane(section) {
            let content = self.layout_nodes(&pane["children"], window, cx);
            page = page.child(self.settings_pane(
                "",
                pane["icon"].as_str().unwrap_or("Wrench"),
                content,
                cx,
            ));
            let represented = settings_layout::paths(pane);
            let additional: Vec<_> = settings::fields(section)
                .into_iter()
                .filter(|f| !represented.contains(&f.path))
                .filter(|f| {
                    !f.path.starts_with("/resets_settings/") || section != Section::Mechanics
                })
                .filter(|f| {
                    !f.path.starts_with("/resets_settings/")
                        || section != Section::Fk
                        || !matches!(
                            f.path.rsplit('/').next(),
                            Some("save_mounting_reset" | "yaw_reset_smooth_time")
                        )
                })
                .filter(|f| {
                    section != Section::StayAligned || f.path.ends_with("extraYawCorrection")
                })
                .collect();
            if !additional.is_empty() {
                let mut extra = div().v_flex().gap_3();
                for field in additional {
                    let node = json!({"kind":"control","path":field.path,"field_type":field.kind,"label":field.label,"widget":"Input"});
                    extra = extra.child(self.layout_control(&node, window, cx));
                }
                page = page.child(self.settings_pane(
                    "native-advanced-settings",
                    "Wrench",
                    extra.into_any_element(),
                    cx,
                ));
            }
        } else {
            let mut content = div().v_flex().gap_3();
            for field in settings::fields(section) {
                let node = json!({"kind":"control","path":field.path,"field_type":field.kind,"label":field.label,"widget":"Input"});
                content = content.child(self.layout_control(&node, window, cx));
            }
            page = page.child(self.settings_pane(
                section.label(),
                "Wrench",
                content.into_any_element(),
                cx,
            ));
        }
        page.child(self.settings_save_bar(cx)).into_any_element()
    }
    pub(super) fn layout_nodes(
        &mut self,
        nodes: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut container = div().v_flex().w_full().gap_2();
        for node in nodes.as_array().into_iter().flatten() {
            container = container.child(self.layout_node(node, window, cx));
        }
        container.into_any_element()
    }
    fn layout_node(
        &mut self,
        node: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match node["kind"].as_str().unwrap_or("") {
            "text" => {
                let label = node["label"].as_str().unwrap_or("");
                let variant = node["variant"].as_str().unwrap_or("");
                let mut text = div()
                    .v_flex()
                    .w_full()
                    .when(variant.is_empty() && node["bold"] != true, |d| {
                        d.w(px(
                            (f32::from(window.viewport_size().width) - 414.).max(160.)
                        ))
                    })
                    .whitespace_normal()
                    .when(variant == "main-title", |d| d.text_2xl().font_bold().mb_2())
                    .when(variant == "section-title", |d| {
                        d.text_lg().font_bold().mt_3()
                    })
                    .when(node["bold"] == true, |d| d.font_bold())
                    .when(node["color"] == "secondary", |d| {
                        d.text_color(cx.theme().muted_foreground)
                    })
                    .children(if variant.is_empty() && node["bold"] != true {
                        self.settings_text_lines(
                            self.text(label),
                            f32::from(window.viewport_size().width) - 414.,
                            window,
                        )
                    } else {
                        vec![self.text(label)]
                    });
                if label == "settings-general-steamvr-description" && self.driver_notice {
                    text = text.child(
                        div()
                            .mt_3()
                            .p_3()
                            .rounded_lg()
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(self.text("steamvr-existing-driver-description")),
                    );
                }
                text.into_any_element()
            }
            "control" => self.layout_control(node, window, cx),
            "condition" => {
                let guard = node["guard"].as_str().unwrap_or("");
                if guard.contains("config?.debug") && self.preferences.value["debug"] != true {
                    div().into_any_element()
                } else {
                    self.layout_nodes(&node["children"], window, cx)
                }
            }
            "container" => {
                let classes = node["classes"].as_str().unwrap_or("");
                let children = node["children"].as_array().cloned().unwrap_or_default();
                let column_count = if classes.contains("grid-cols-3") {
                    3
                } else if classes.contains("grid-cols-2") {
                    2
                } else {
                    1
                };
                let columns = column_count > 1 && f32::from(window.viewport_size().width) >= 1000.;
                let horizontal = (classes.contains("flex-row")
                    || (classes.contains("flex gap") && !classes.contains("flex-col")))
                    && f32::from(window.viewport_size().width) >= 1100.;
                let mut container = div()
                    .flex()
                    .w_full()
                    .gap_3()
                    .when(columns || horizontal, |d| d.flex_row().flex_wrap())
                    .when(!columns && !horizontal, |d| d.flex_col())
                    .when(classes.contains("pt-5") || classes.contains("mt-6"), |d| {
                        d.mt_5()
                    })
                    .when(classes.contains("pb-5") || classes.contains("pb-4"), |d| {
                        d.mb_4()
                    });
                for child in children {
                    let element = self.layout_node(&child, window, cx);
                    container = container.child(
                        div()
                            .min_w_0()
                            .when(columns, |d| d.w(relative(1. / column_count as f32 - 0.02)))
                            .when(horizontal && !columns, |d| d.flex_1())
                            .when(!columns && !horizontal, |d| d.w_full())
                            .child(element),
                    );
                }
                container.into_any_element()
            }
            "action" => self.layout_action(node, cx),
            _ => div().into_any_element(),
        }
    }
    fn layout_disabled(&self, node: &Value) -> bool {
        let path = node["path"].as_str().unwrap_or("");
        if path.starts_with("/steam_vr_trackers/")
            && !path.ends_with("automaticTrackerToggle")
            && self.draft.value["steam_vr_trackers"]["automaticTrackerToggle"] == true
        {
            return true;
        }
        if node["disabled"]
            .as_str()
            .unwrap_or("")
            .contains("!config.setupComplete")
            && self.draft.value["stay_aligned"]["setupComplete"] != true
        {
            return true;
        }
        let d = node["disabled"].as_str().unwrap_or("");
        for pose in ["Standing", "Sitting", "Flat"] {
            if d.contains(&format!("!has{pose}Pose")) {
                let lower = pose.to_lowercase();
                let value = &self.draft.value["stay_aligned"];
                if value[format!("{lower}Enabled")] != true
                    && ["UpperLegAngle", "LowerLegAngle", "FootAngle"]
                        .iter()
                        .all(|angle| value[format!("{lower}{angle}")].as_f64().unwrap_or(0.) == 0.)
                {
                    return true;
                }
            }
        }
        self.snapshot.connection != Connection::Connected || self.draft.saving.is_some()
    }
    fn layout_value(&self, node: &Value) -> Value {
        if let Some(path) = node["path"].as_str() {
            self.draft
                .value
                .pointer(path)
                .cloned()
                .unwrap_or(Value::Null)
        } else {
            self.preferences
                .value
                .pointer(&format!("/{}", node["preference"].as_str().unwrap_or("")))
                .cloned()
                .unwrap_or(Value::Null)
        }
    }
    fn layout_set(&mut self, node: &Value, value: Value, cx: &mut Context<Self>) {
        if let Some(path) = node["path"].as_str() {
            self.draft.set(path, value);
            cx.notify();
        } else if let Some(pref) = node["preference"].as_str() {
            self.preference(pref, value, cx);
        }
    }
    fn layout_toggle(
        &mut self,
        node: &Value,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = node["path"].as_str().unwrap_or("");
        let shares = &self.draft.value["steam_vr_trackers"];
        if enabled
            && matches!(
                path,
                "/steam_vr_trackers/left_hand" | "/steam_vr_trackers/right_hand"
            )
            && shares["left_hand"] != true
            && shares["right_hand"] != true
        {
            let view = cx.entity().downgrade();
            let node = node.clone();
            let warning = self.text("settings-general-steamvr-trackers-hands-warning");
            let accept = self.text("settings-general-steamvr-trackers-hands-warning-done");
            let cancel = self.text("settings-general-steamvr-trackers-hands-warning-cancel");
            window.open_alert_dialog(cx, move |dialog, _, _| {
                let view = view.clone();
                let node = node.clone();
                dialog
                    .confirm()
                    .title(warning.clone())
                    .button_props(
                        DialogButtonProps::default()
                            .ok_text(accept.clone())
                            .cancel_text(cancel.clone())
                            .show_cancel(true),
                    )
                    .on_ok(move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| this.layout_set(&node, json!(true), cx));
                        true
                    })
            });
        } else {
            self.layout_set(node, json!(enabled), cx);
        }
    }
    fn layout_control(
        &mut self,
        node: &Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let path = node["path"]
            .as_str()
            .or(node["preference"].as_str())
            .unwrap_or("");
        let field = Field {
            path: path.into(),
            kind: node["field_type"].as_str().unwrap_or("bool").into(),
            label: node["label"].as_str().unwrap_or("").into(),
        };
        let label = self.text(&field.label);
        let value = self.layout_value(node);
        let disabled = if node.get("preference").is_some() {
            false
        } else {
            self.layout_disabled(node)
        };
        let widget = node["widget"].as_str().unwrap_or("Input");
        if widget == "SystemFileInput" {
            return Button::new("bvh-directory-picker")
                .label(
                    value
                        .as_str()
                        .filter(|s| !s.is_empty())
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
                                this.preference("bvhDirectory", json!(path.to_string_lossy()), cx)
                            });
                        }
                    })
                    .detach();
                }))
                .into_any_element();
        }
        if node["preference"] == "fonts" {
            let current = value
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .unwrap_or_default();
            let view = cx.entity().downgrade();
            return Button::new("font-choice")
                .w_full()
                .label(current)
                .dropdown_menu(move |mut menu, _, _| {
                    for font in [
                        "poppins",
                        "OpenDyslexic",
                        "Lexend",
                        "Ubuntu",
                        "Noto Sans",
                        "ui-sans-serif",
                    ] {
                        let view = view.clone();
                        menu = menu.item(PopupMenuItem::new(font).on_click(move |_, _, cx| {
                            let _ = view.update(cx, |this, cx| {
                                this.preference(
                                    "fonts",
                                    json!(
                                        font.split(',')
                                            .map(|s| s.trim_matches('"'))
                                            .collect::<Vec<_>>()
                                    ),
                                    cx,
                                )
                            });
                        }));
                    }
                    menu
                })
                .into_any_element();
        }
        if field.kind == "bool" && widget != "NumberSelector" {
            let node = node.clone();
            let switch = Switch::new(format!("toggle-{path}"))
                .checked(value.as_bool().unwrap_or(false))
                .small()
                .disabled(disabled)
                .accessibility_label(label.clone())
                .on_click(cx.listener(move |this, enabled: &bool, window, cx| {
                    this.layout_toggle(&node, *enabled, window, cx)
                }));
            return SwitchRow::new(label, switch)
                .disabled(disabled)
                .build(cx)
                .into_any_element();
        }
        if widget == "Radio" {
            let selected = settings_layout::radio_value(node).unwrap_or(0);
            let active = value.as_u64() == Some(selected);
            let description = node["description"].as_str().map(|id| self.text(id).into());
            let node = node.clone();
            return ChoiceCard::new(format!("radio-{path}-{selected}"), label)
                .description(description)
                .checked(active)
                .disabled(disabled)
                .text_size(px(
                    self.preferences.value["textSize"].as_f64().unwrap_or(12.) as f32,
                ))
                .build(cx)
                .on_click(
                    cx.listener(move |this, _, _, cx| this.layout_set(&node, json!(selected), cx)),
                )
                .into_any_element();
        }
        if widget == "NumberSelector" {
            let current = value.as_f64().unwrap_or(0.);
            let min = node["min"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.);
            let max = node["max"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(100.);
            let step = node["step"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(1.);
            let display = match node["unit"].as_str().unwrap_or("") {
                "percent" => format!("{:.0}%", current * 100.),
                "seconds" => format!("{current:.2}s"),
                _ => format!("{}", (current * 100.).round() / 100.),
            };
            let [decrease, increase] = [-1., 1.].map(|direction| {
                let next = ((current + direction * step) * 100.).round() / 100.;
                let node = node.clone();
                let integer = field.kind.starts_with("uint") || field.kind.starts_with("int");
                let id = format!("step-{path}-{direction}");
                let button = if direction < 0. {
                    NumberSelector::decrement(id)
                } else {
                    NumberSelector::increment(id)
                };
                button
                    .disabled(disabled || next < min - 1e-6 || next > max + 1e-6)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.layout_set(
                            &node,
                            if integer {
                                json!(next as u64)
                            } else {
                                json!(next)
                            },
                            cx,
                        )
                    }))
            });
            return NumberSelector::new(label, display, decrease, increase)
                .build(cx)
                .into_any_element();
        }
        if !rpc_generated::enum_choices(&field.kind).is_empty() {
            let selected = value.as_u64().unwrap_or(0);
            let choices: Vec<_> = rpc_generated::enum_choices(&field.kind)
                .iter()
                .filter(|(name, _)| {
                    field.kind != "BodyPart"
                        || (*name != "NONE" && slimevr_gpui::assignment::allowed("full-body", name))
                })
                .map(|(n, v)| (*v, self.choice_label(&field, n)))
                .collect();
            let title = choices
                .iter()
                .find(|(v, _)| *v == selected)
                .map(|(_, s)| s.clone())
                .unwrap_or_else(|| "—".into());
            let view = cx.entity().downgrade();
            let node = node.clone();
            return div()
                .v_flex()
                .gap_2()
                .w_full()
                .when(!field.label.is_empty(), |d| {
                    d.child(div().font_bold().child(label))
                })
                .child(
                    Button::new(format!("choice-{path}"))
                        .label(title)
                        .disabled(disabled)
                        .w_full()
                        .dropdown_menu(move |mut menu, _, _| {
                            for (value, label) in &choices {
                                let view = view.clone();
                                let node = node.clone();
                                let value = *value;
                                menu = menu.item(PopupMenuItem::new(label.clone()).on_click(
                                    move |_, _, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.layout_set(&node, json!(value), cx)
                                        });
                                    },
                                ));
                            }
                            menu
                        }),
                )
                .into_any_element();
        }
        let text = if value.is_null() {
            String::new()
        } else if let Some(s) = value.as_str() {
            s.into()
        } else {
            value.to_string()
        };
        div()
            .v_flex()
            .gap_2()
            .w_full()
            .when(!field.label.is_empty(), |d| {
                d.child(div().font_bold().child(label))
            })
            .child(self.input(path, text, false, Some(field.clone()), window, cx))
            .when(self.draft.invalid.contains_key(path), |d| {
                d.child(
                    div()
                        .text_color(rgb(0xff9f9f))
                        .child(self.text(self.draft.invalid.get(path).unwrap())),
                )
            })
            .into_any_element()
    }
    fn layout_action(&self, node: &Value, cx: &mut Context<Self>) -> AnyElement {
        match node["widget"].as_str().unwrap_or("") {
            "ThemeSelector" => {
                let mut themes = div().h_flex().gap_3().flex_wrap();
                for (name, color) in [
                    ("slime", 0x65459a),
                    ("slime-green", 0x69ba82),
                    ("slime-yellow", 0xd7b65c),
                    ("slime-orange", 0xd49860),
                    ("slime-red", 0xc96f7c),
                    ("dark", 0x252525),
                    ("light", 0xf2f2f2),
                    ("trans", 0x83cbdc),
                    ("asexual", 0x9773b9),
                    ("snep", 0x8babb6),
                ] {
                    themes = themes.child(
                        Button::new(format!("theme-{name}"))
                            .ghost()
                            .size(px(46.))
                            .rounded_full()
                            .selected(self.preferences.value["theme"] == name)
                            .child(div().size(px(30.)).rounded_full().bg(rgb(color)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.preference("theme", json!(name), cx);
                                this.apply_theme(cx)
                            })),
                    );
                }
                themes.into_any_element()
            }
            "Range" => div()
                .v_flex()
                .gap_2()
                .w_full()
                .child(gpui_kit::component::slider::Slider::new(&self.font_slider).w_full())
                .child(
                    div()
                        .h_flex()
                        .justify_between()
                        .children(["10pt", "11pt", "12pt", "13pt", "14pt", "15pt"]),
                )
                .into_any_element(),
            "LangSelector" => {
                let view = cx.entity().downgrade();
                let locale = self.preferences.value["lang"]
                    .as_str()
                    .unwrap_or("zh-Hans")
                    .to_owned();
                Button::new("language")
                    .w_full()
                    .label(settings_layout::locale_name(&locale).to_owned())
                    .dropdown_menu(move |mut menu, _, _| {
                        for locale in slimevr_gpui::locales::LOCALES {
                            let view = view.clone();
                            menu = menu.item(
                                PopupMenuItem::new(settings_layout::locale_name(locale).to_owned())
                                    .on_click(move |_, _, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            match Localizer::new(locale) {
                                                Ok(mut l10n) => {
                                                    if let Ok(source) = std::fs::read_to_string(
                                                        this.paths.root.join("override.ftl"),
                                                    ) {
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
                                                    this.preference("lang", json!(locale), cx);
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
                                    }),
                            );
                        }
                        menu
                    })
                    .into_any_element()
            }
            "DeveloperToggles" => {
                let mut toggles = div().v_flex().gap_2();
                for (key, label) in [
                    ("highContrast", "high_contrast"),
                    ("preciseRotation", "precise_rotation"),
                    ("fastDataFeed", "fast_data_feed"),
                    ("filterSlimesAndHMD", "filter_slimes_and_hmd"),
                    ("sortByName", "sort_by_name"),
                    ("rawSlimeRotation", "raw_slime_rotation"),
                    ("moreInfo", "more_info"),
                ] {
                    toggles = toggles.child(self.pref_toggle(
                        &format!("devSettings/{key}"),
                        &format!("widget-developer_mode-{label}"),
                        cx,
                    ));
                }
                toggles.into_any_element()
            }
            "VMCFileUpload" => self.avatar_controls(cx),
            "MagnetometerToggleSetting" => {
                div()
                    .v_flex()
                    .gap_3()
                    .mt_4()
                    .child(div().text_lg().font_bold().child(
                        self.text("settings-general-tracker_mechanics-use_mag_on_all_trackers"),
                    ))
                    .child(self.text(
                        "settings-general-tracker_mechanics-use_mag_on_all_trackers-description",
                    ))
                    .child(
                        div()
                            .h_flex()
                            .gap_3()
                            .p_3()
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded_lg()
                            .child(
                                Switch::new("global-mag-toggle")
                                    .checked(self.global_mag)
                                    .small()
                                    .disabled(self.snapshot.connection != Connection::Connected)
                                    .on_click(cx.listener(|this, enabled: &bool, _, cx| {
                                        this.rpc(
                                            "ChangeMagToggleRequest",
                                            json!({"enable":*enabled}),
                                            cx,
                                        )
                                    })),
                            )
                            .child(self.text(
                                "settings-general-tracker_mechanics-use_mag_on_all_trackers-label",
                            )),
                    )
                    .into_any_element()
            }
            "CopySettingsButton" => Button::new("copy-stay-aligned-settings")
                .label(self.text("settings-stay_aligned-debug-copy-label"))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        serde_json::to_string_pretty(&this.draft.value["stay_aligned"])
                            .unwrap_or_default(),
                    ))
                }))
                .into_any_element(),
            "Button"
                if node["callback"]
                    .as_str()
                    .unwrap_or("")
                    .contains("openStanding")
                    || node["callback"]
                        .as_str()
                        .unwrap_or("")
                        .contains("openSitting")
                    || node["callback"].as_str().unwrap_or("").contains("openFlat") =>
            {
                let callback = node["callback"].as_str().unwrap_or("");
                let (pose, name, image) = if callback.contains("openStanding") {
                    (0, "standing", "StayAlignedStanding")
                } else if callback.contains("openSitting") {
                    (1, "sitting", "StayAlignedSitting")
                } else {
                    (2, "flat", "StayAlignedFloor")
                };
                let title = self.text(&format!(
                    "onboarding-stay_aligned-relaxed_poses-{name}-title"
                ));
                let step0 = self.text(&format!(
                    "onboarding-stay_aligned-relaxed_poses-{name}-step-0"
                ));
                let step1 = self.text(&format!(
                    "onboarding-stay_aligned-relaxed_poses-{name}-step-1-v2"
                ));
                let save = self.text("settings-stay_aligned-relaxed_poses-save_pose");
                let reset = self.text("settings-stay_aligned-relaxed_poses-reset_pose");
                let close = self.text("settings-stay_aligned-relaxed_poses-close");
                let view = cx.entity().downgrade();
                Button::new(format!("relaxed-pose-edit-{pose}"))
                    .primary()
                    .label(save.clone())
                    .disabled(self.draft.value["stay_aligned"]["setupComplete"] != true)
                    .on_click(move |_, window, cx| {
                        let view = view.clone();
                        let title = title.clone();
                        let step0 = step0.clone();
                        let step1 = step1.clone();
                        let save = save.clone();
                        let reset = reset.clone();
                        let close = close.clone();
                        window.open_dialog(cx, move |dialog, _, cx| {
                            let detect = view.clone();
                            let clear = view.clone();
                            dialog
                                .title(title.clone())
                                .child(
                                    div()
                                        .v_flex()
                                        .gap_3()
                                        .child(step0.clone())
                                        .child(step1.clone())
                                        .child(
                                            crate::ui_assets::image(
                                                &format!("slime/stay-aligned/{image}.webp"),
                                                cx,
                                            )
                                            .w(px(360.))
                                            .h(px(220.)),
                                        ),
                                )
                                .footer(
                                    div()
                                        .h_flex()
                                        .gap_3()
                                        .child(
                                            Button::new("relaxed-close")
                                                .label(close.clone())
                                                .on_click(|_, window, cx| window.close_dialog(cx)),
                                        )
                                        .child(
                                            Button::new("relaxed-reset")
                                                .label(reset.clone())
                                                .on_click(move |_, window, cx| {
                                                    let _ = clear.update(cx, |this, cx| {
                                                        this.rpc(
                                                            "ResetStayAlignedRelaxedPoseRequest",
                                                            json!({"pose":pose}),
                                                            cx,
                                                        )
                                                    });
                                                    window.close_dialog(cx);
                                                }),
                                        )
                                        .child(
                                            Button::new("relaxed-detect")
                                                .primary()
                                                .label(save.clone())
                                                .on_click(move |_, window, cx| {
                                                    let _ = detect.update(cx, |this, cx| {
                                                        this.rpc(
                                                            "DetectStayAlignedRelaxedPoseRequest",
                                                            json!({"pose":pose}),
                                                            cx,
                                                        )
                                                    });
                                                    window.close_dialog(cx);
                                                }),
                                        ),
                                )
                        });
                    })
                    .into_any_element()
            }
            "Button"
                if node["callback"] == "openConfigFolder"
                    || node["callback"] == "openLogsFolder" =>
            {
                let logs = node["callback"] == "openLogsFolder";
                let target = if logs {
                    self.paths.logs.clone()
                } else {
                    self.paths.root.clone()
                };
                Button::new(if logs {
                    "original-open-logs"
                } else {
                    "original-open-config"
                })
                .label(self.text(node["label"].as_str().unwrap_or("native-open")))
                .on_click(move |_, _, cx| cx.open_with_system(&target))
                .into_any_element()
            }
            "Button"
                if node["callback"]
                    .as_str()
                    .unwrap_or("")
                    .contains("setShowWarning") =>
            {
                let callback = node["callback"].as_str().unwrap_or("");
                let variant = if callback.contains("GUI") {
                    "gui"
                } else if callback.contains("Server") {
                    "server"
                } else {
                    "all"
                };
                let mut args = fluent_bundle::FluentArgs::new();
                args.set("type", variant);
                let warning = self
                    .l10n
                    .format("settings-utils-advanced-reset_warning", &args);
                let cancel = self.text("settings-utils-advanced-reset_warning-cancel");
                let accept = self.text("settings-utils-advanced-reset_warning-reset");
                let view = cx.entity().downgrade();
                Button::new(format!("reset-original-{variant}"))
                    .label(self.text(node["label"].as_str().unwrap_or("native-settings-reset")))
                    .on_click(move |_, window, cx| {
                        let view = view.clone();
                        let warning = warning.clone();
                        let cancel = cancel.clone();
                        let accept = accept.clone();
                        window.open_alert_dialog(cx, move |dialog, _, _| {
                            let view = view.clone();
                            dialog
                                .confirm()
                                .title(warning.clone())
                                .button_props(
                                    DialogButtonProps::default()
                                        .ok_text(accept.clone())
                                        .cancel_text(cancel.clone())
                                        .show_cancel(true),
                                )
                                .on_ok(move |_, _, cx| {
                                    let _ = view.update(cx, |this, cx| {
                                        if matches!(variant, "gui" | "all") {
                                            this.ui_error = this.preferences.reset_known().err();
                                            this.apply_theme(cx);
                                        }
                                        if matches!(variant, "server" | "all") {
                                            this.draft = Default::default();
                                            this.rpc("SettingsResetRequest", json!({}), cx);
                                            let _ = this.client.send(Command::ReadSettings);
                                        }
                                        cx.notify();
                                    });
                                    true
                                })
                        });
                    })
                    .into_any_element()
            }
            "Button" if node["target"] == "/onboarding/stay-aligned" => {
                Button::new("stay-aligned-setup")
                    .primary()
                    .label(self.text("settings-stay_aligned-setup-label"))
                    .on_click(cx.listener(|this, _, _, cx| this.go(Page::StayAlignedSetup, cx)))
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }
    pub(super) fn settings_extra_fields(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut content = div().v_flex().w_full().gap_3();
        for field in settings::fields(section) {
            let node = json!({"kind":"control","path":field.path,"field_type":field.kind,"label":field.label,"widget":"Input"});
            content = content.child(self.layout_control(&node, window, cx));
        }
        content.into_any_element()
    }
    pub(super) fn settings_save_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let saving = self.draft.saving.is_some();
        let can_save = !self.draft.dirty.is_empty()
            && self.draft.invalid.is_empty()
            && !saving
            && self.snapshot.connection == Connection::Connected;
        div()
            .h_flex()
            .w_full()
            .gap_3()
            .items_center()
            .px_4()
            .py_3()
            .rounded_lg()
            .bg(cx.theme().muted)
            .child(
                Button::new("apply-settings")
                    .primary()
                    .label(self.text(if saving {
                        "native-saving"
                    } else {
                        "native-apply"
                    }))
                    .disabled(!can_save)
                    .on_click(cx.listener(|this, _, _, cx| {
                        match this.draft.request() {
                            Ok(value) => match this.client.rpc("ChangeSettingsRequest", value) {
                                Ok(_) => {
                                    this.draft.begin_save(this.snapshot.session);
                                    let _ = this.client.send(Command::ReadSettings);
                                    this.ui_error = None;
                                }
                                Err(error) => this.ui_error = Some(error),
                            },
                            Err(error) => this.ui_error = Some(this.text(&error)),
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("discard-settings")
                    .label(self.text("native-discard"))
                    .disabled(saving)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.draft = Default::default();
                        if let Some(state) = this.snapshot.rpc.get("SettingsResponse") {
                            this.draft.merge(&state.value);
                        }
                        this.inputs.clear();
                        this.input_subscriptions.clear();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}
