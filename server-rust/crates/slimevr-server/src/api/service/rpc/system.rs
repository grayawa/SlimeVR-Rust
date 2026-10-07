//! Heartbeat, driver management, checklist, status and VRChat requests.
use super::super::Service;
use crate::api::{diagnostics, protocol::rpc_frame, vrchat, FrontendConfig, Request};
use slimevr_core::pose::PoseEngine;
use solarxr_protocol::rpc;

impl Service {
    pub(super) fn rpc_system(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        engine: &mut PoseEngine,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        match h.message_type() {
            rpc::RpcMessage::HeartbeatRequest => {
                out.push(rpc_frame(rpc::RpcMessage::HeartbeatResponse, tx, |f| {
                    rpc::HeartbeatResponse::create(f, &Default::default()).as_union_value()
                }))
            }
            rpc::RpcMessage::EnableSteamVRDriverRequest => {
                h.message_as_enable_steam_vrdriver_request()
                    .ok_or("missing driver enable request")?;
                let manager = self
                    .driver_manager
                    .clone()
                    .ok_or("SteamVR driver management is disabled")?;
                let sender = self.commands.clone();
                let old = self.driver_status.clone();
                tokio::spawn(async move {
                    let (status, error) = match manager.enable().await {
                        Ok(status) => (status, None),
                        Err(e) => (old, Some(format!("Unable to enable SteamVR driver: {e}"))),
                    };
                    let _ = sender.send(Request::DriverStatus { status, error }).await;
                });
            }
            rpc::RpcMessage::TrackingChecklistRequest => out.push(diagnostics::frame(
                tx,
                engine.snapshot(),
                &self.config,
                &self.steam_vr,
                &self.driver_status,
                &self.diagnostics,
            )),
            rpc::RpcMessage::IgnoreTrackingChecklistStepRequest => {
                let r = h
                    .message_as_ignore_tracking_checklist_step_request()
                    .ok_or("missing checklist request")?;
                let id = r.step_id().0;
                if !(1..=12).contains(&id) {
                    return Err("unknown checklist step".into());
                }
                if diagnostics::ignorable(id) {
                    let mut c = self.config.clone();
                    if r.ignore() {
                        c.ignored_steps.insert(id);
                    } else {
                        c.ignored_steps.remove(&id);
                    }
                    c.save(self.state_path.as_deref())?;
                    self.config = c;
                }
                out.push(diagnostics::frame(
                    tx,
                    engine.snapshot(),
                    &self.config,
                    &self.steam_vr,
                    &self.driver_status,
                    &self.diagnostics,
                ));
            }
            rpc::RpcMessage::StatusSystemRequest => out.push(self.statuses.frame(tx)),
            rpc::RpcMessage::VRCConfigStateRequest => out.push(vrchat::frame(
                tx,
                self.vrchat.as_ref(),
                &self.config,
                engine.snapshot(),
            )),
            rpc::RpcMessage::VRCConfigSettingToggleMute => {
                let r = h
                    .message_as_vrcconfig_setting_toggle_mute()
                    .ok_or("missing VRChat mute request")?;
                let key = r.key().ok_or("missing VRChat validity key")?;
                if !vrchat::FIELDS.contains(&key) {
                    return Err("unknown VRChat validity key".into());
                }
                let mut c = self.config.clone();
                let mut muted = c.yaml["vrcConfig"]["mutedWarnings"]
                    .as_sequence()
                    .cloned()
                    .unwrap_or_default();
                let value = serde_yaml_ng::Value::String(key.into());
                if let Some(index) = muted.iter().position(|v| *v == value) {
                    muted.remove(index);
                } else {
                    muted.push(value);
                }
                crate::config::put(&mut c.yaml, &["vrcConfig", "mutedWarnings"], muted)
                    .map_err(|e| e.to_string())?;
                c.save(self.state_path.as_deref())?;
                self.config = c;
                self.broadcast(vrchat::frame(
                    tx,
                    self.vrchat.as_ref(),
                    &self.config,
                    engine.snapshot(),
                ));
            }
            rpc::RpcMessage::OverlayDisplayModeRequest => out.push(overlay_frame(tx, &self.config)),
            rpc::RpcMessage::OverlayDisplayModeChangeRequest => {
                let r = h
                    .message_as_overlay_display_mode_change_request()
                    .ok_or("missing overlay settings")?;
                let mut c = self.config.clone();
                if let Some(value) = r.is_visible() {
                    crate::config::put(&mut c.yaml, &["overlay", "isVisible"], value)
                        .map_err(|e| e.to_string())?;
                }
                if let Some(value) = r.is_mirrored() {
                    crate::config::put(&mut c.yaml, &["overlay", "isMirrored"], value)
                        .map_err(|e| e.to_string())?;
                }
                c.save(self.state_path.as_deref())?;
                self.config = c;
                self.broadcast(overlay_frame(tx, &self.config));
            }
            rpc::RpcMessage::ServerInfosRequest => {
                out.push(rpc_frame(rpc::RpcMessage::ServerInfosResponse, tx, |f| {
                    let ip = f.create_string(&self.local_ip.to_string());
                    rpc::ServerInfosResponse::create(
                        f,
                        &rpc::ServerInfosResponseArgs { localIp: Some(ip) },
                    )
                    .as_union_value()
                }));
            }
            rpc::RpcMessage::InstalledInfoRequest => {
                out.push(rpc_frame(rpc::RpcMessage::InstalledInfoResponse, tx, |f| {
                    rpc::InstalledInfoResponse::create(
                        f,
                        &rpc::InstalledInfoResponseArgs {
                            isUdevInstalled: self.diagnostics.udev,
                            isWayland: self.diagnostics.wayland,
                        },
                    )
                    .as_union_value()
                }))
            }
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
fn overlay_frame(tx: u32, c: &FrontendConfig) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::OverlayDisplayModeResponse, tx, |f| {
        rpc::OverlayDisplayModeResponse::create(
            f,
            &rpc::OverlayDisplayModeResponseArgs {
                is_visible: c.yaml["overlay"]["isVisible"].as_bool().unwrap_or(false),
                is_mirrored: c.yaml["overlay"]["isMirrored"].as_bool().unwrap_or(false),
            },
        )
        .as_union_value()
    })
}
