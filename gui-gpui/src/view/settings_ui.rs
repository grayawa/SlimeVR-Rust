use super::*;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use serde_json::Value;
use slimevr_gpui::settings::Field;

impl SlimeView {
    pub(super) fn rpc(&mut self, name: &str, value: Value, cx: &mut Context<Self>) {
        if name == "OverlayDisplayModeChangeRequest" {
            let mut state = self
                .overlay
                .value
                .clone()
                .unwrap_or_else(|| (*self.read("OverlayDisplayModeResponse")).clone());
            for key in ["is_visible", "is_mirrored"] {
                if let Some(v) = value.get(key) {
                    state[key] = v.clone();
                }
            }
            self.ui_error = self
                .client
                .send(Command::PubSub(slimevr_gpui::overlay::message(Some(
                    &state,
                ))))
                .err();
            if self.ui_error.is_none() {
                self.overlay.value = Some(state);
            }
        }
        self.ui_error = self.client.rpc(name, value).err();
        cx.notify();
    }
    pub(super) fn input(
        &mut self,
        id: &str,
        value: String,
        masked: bool,
        field: Option<Field>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let entry = if let Some((input, model)) = self.inputs.get_mut(id) {
            if *model != value {
                input.update(cx, |s, cx| s.set_value(value.clone(), window, cx));
                *model = value;
            }
            input.clone()
        } else {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value.clone())
                    .placeholder(if id == "wifi-ssid" {
                        self.text("onboarding-wifi_creds-ssid.placeholder")
                    } else if id == "wifi-password" {
                        self.text("onboarding-wifi_creds-password.placeholder")
                    } else {
                        String::new()
                    })
                    .masked(masked)
            });
            let key = id.to_owned();
            let subscription = cx.subscribe_in(&input, window, move |this, input, event, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let text = input.read(cx).value().to_string();
                    if let Some(field) = &field {
                        let _ = this.draft.edit(field, &text);
                    } else {
                        this.form_values.insert(key.clone(), text);
                    }
                    cx.notify();
                }
            });
            self.input_subscriptions.push(subscription);
            self.inputs.insert(id.into(), (input.clone(), value));
            input
        };
        let width = if self.navigation.page == Page::Connect && id.starts_with("wifi-") {
            let available = f32::from(window.viewport_size().width) - 190.;
            if available > 810. {
                (available / 2. - 48.).min(400.)
            } else {
                (available - 48.).min(400.)
            }
        } else {
            240.
        };
        if self.in_onboarding() && id.starts_with("wifi-") {
            Input::new(&entry)
                .large()
                .w_full()
                .h(px(42.))
                .bg(cx.theme().popover)
                .into_any_element()
        } else if id.starts_with('/') && self.navigation.page.in_settings() {
            Input::new(&entry).w_full().into_any_element()
        } else {
            Input::new(&entry).w(px(width)).into_any_element()
        }
    }
    pub(super) fn local_input(
        &mut self,
        id: &str,
        default: &str,
        masked: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = self
            .form_values
            .get(id)
            .cloned()
            .unwrap_or_else(|| default.into());
        self.input(id, value, masked, None, window, cx)
    }
    pub(super) fn form_value(&self, id: &str, default: &str) -> String {
        self.form_values
            .get(id)
            .cloned()
            .unwrap_or_else(|| default.into())
    }
    pub(super) fn choice_label(&self, field: &Field, label: &str) -> String {
        let id = match field.kind.as_str() {
            "BodyPart" => format!("body_part-{label}"),
            "FilteringType" => format!(
                "settings-general-tracker_mechanics-filtering-type-{}",
                label.to_lowercase()
            ),
            "ArmsMountingResetMode" => format!(
                "settings-general-fk_settings-arm_fk-reset-{}",
                label.to_lowercase()
            ),
            _ => format!("native-choice-{}-{}", field.kind, label),
        };
        let localized = self.text(&id);
        if localized == id {
            label.replace('_', " ")
        } else {
            localized
        }
    }
}
