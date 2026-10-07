//! Overlay state comes from its original Pub/Sub topic; RPC persists desktop preferences.
use serde_json::{Value, json};
#[derive(Default)]
pub struct Overlay {
    handle: Option<u64>,
    pub value: Option<Value>,
    session: u64,
}
pub fn topic() -> Value {
    json!({"type":"TopicId","value":{"organization":"slimevr.dev","app_name":"overlay","topic":"display_settings"}})
}
pub fn message(value: Option<&Value>) -> Value {
    json!({"u":{"type":"Message","value":{"topic":topic(),"payload":value.map(|v|json!({"type":"KeyValues","value":{"keys":["is_visible","is_mirrored"],"values":[v["is_visible"].as_bool().unwrap_or(false).to_string(),v["is_mirrored"].as_bool().unwrap_or(false).to_string()]}}))}}})
}
impl Overlay {
    pub fn apply(&mut self, session: u64, event: &Value) {
        if self.session != session {
            self.session = session;
            self.handle = None;
            self.value = None;
        }
        let u = &event["u"];
        let v = &u["value"];
        if u["type"] == "TopicMapping" && v["id"] == topic()["value"] {
            self.handle = v["handle"]["id"].as_u64();
        }
        if u["type"] != "Message" {
            return;
        }
        let target = &v["topic"];
        if !(target == &topic()
            || (target["type"] == "TopicHandle"
                && target["value"]["id"].as_u64() == self.handle
                && self.handle.is_some()))
        {
            return;
        }
        if v["payload"]["type"] != "KeyValues" {
            return;
        }
        let p = &v["payload"]["value"];
        let mut value = self.value.clone().unwrap_or(json!({}));
        if let (Some(keys), Some(values)) = (p["keys"].as_array(), p["values"].as_array()) {
            for (k, v) in keys.iter().zip(values) {
                if let (
                    Some(key @ ("is_visible" | "is_mirrored")),
                    Some(text @ ("true" | "false")),
                ) = (k.as_str(), v.as_str())
                {
                    value[key] = json!(text == "true");
                }
            }
            self.value = Some(value);
        }
    }
}
