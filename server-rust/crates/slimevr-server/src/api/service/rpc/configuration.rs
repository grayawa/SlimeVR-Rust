//! Settings, skeleton proportions and keybind requests.
use super::super::Service;
use crate::{
    api::{protocol, settings},
    receiver::Receiver,
};
use serde_json::json;
use slimevr_core::pose::PoseEngine;
use solarxr_protocol::rpc;

impl Service {
    pub(super) fn rpc_configuration(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        match h.message_type() {
            rpc::RpcMessage::SettingsRequest => out.push(settings::frame(tx, &self.config)),
            rpc::RpcMessage::ChangeSettingsRequest => {
                let r = h
                    .message_as_change_settings_request()
                    .ok_or("missing settings")?;
                let candidate = settings::change(&self.config, r)?;
                self.commit(candidate, engine, receiver, at)?;
            }
            rpc::RpcMessage::SkeletonConfigRequest => {
                out.push(protocol::skeleton_frame(tx, &self.config.pose))
            }
            rpc::RpcMessage::ChangeSkeletonConfigRequest => {
                let r = h
                    .message_as_change_skeleton_config_request()
                    .ok_or("missing bone settings")?;
                let key = protocol::offset_fields()
                    .into_iter()
                    .find(|(id, _)| *id == r.bone().0)
                    .ok_or("unknown bone")?
                    .1;
                let mut c = self.config.clone();
                let mut s = serde_json::to_value(c.pose.skeleton).unwrap();
                s[key] = json!(r.value());
                c.pose.skeleton = serde_json::from_value(s).map_err(|_| "invalid bone value")?;
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::SkeletonResetAllRequest => {
                let mut c = self.config.clone();
                c.pose.skeleton = if let Some(height) = c.pose.hmd_height {
                    c.pose.skeleton.reset_offsets_for_height(height)?
                } else {
                    Default::default()
                };
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::SettingsResetRequest => {
                let mut c =
                    crate::config::from_yaml(serde_yaml_ng::Value::Mapping(Default::default()))
                        .map_err(|e| e.to_string())?;
                c.pose.bindings = self.config.pose.bindings.clone();
                c.device_ids = self.config.device_ids.clone();
                c.allowed_macs = self.config.allowed_macs.clone();
                c.tracker_names = self.config.tracker_names.clone();
                c.yaml = self.config.yaml.clone();
                c.tracker_port = self.config.tracker_port;
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::KeybindRequest => out.push(crate::hotkeys::frame(
                tx,
                &crate::hotkeys::Settings::read(&self.config.yaml)?,
            )),
            rpc::RpcMessage::ChangeKeybindRequest => {
                let r = h
                    .message_as_change_keybind_request()
                    .and_then(|r| r.keybind())
                    .ok_or("missing keybind change")?;
                let mut c = self.config.clone();
                crate::hotkeys::change(&mut c.yaml, r)?;
                c.save(self.state_path.as_deref())?;
                self.config = c;
                self.broadcast(crate::hotkeys::frame(
                    tx,
                    &crate::hotkeys::Settings::read(&self.config.yaml)?,
                ));
            }
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
