//! SolarXR validation and ordered dispatch; business handlers are grouped by domain.
mod calibration;
mod configuration;
mod hardware;
mod recording;
mod system;
mod trackers;
use super::Service;
use crate::{
    api::{settings, types::error_wire, Wire},
    receiver::Receiver,
};
use serde_json::json;
use slimevr_core::pose::PoseEngine;
use solarxr_protocol as sx;
use solarxr_protocol::{flatbuffers as fb, rpc};
use tokio_tungstenite::tungstenite::Message;

impl Service {
    pub fn handle(
        &mut self,
        data: Message,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
    ) -> Vec<Wire> {
        self.sync_pose_config(engine);
        match self.try_handle(data, receiver, engine, at) {
            Ok(v) => v,
            Err(e) => {
                crate::logging::diagnostic(
                    crate::log_level::LogLevel::Warn,
                    &json!({"type":"backend_request_error", "message":e}),
                );
                vec![
                    error_wire(e),
                    Wire::Binary(settings::frame(0, &self.config)),
                ]
            }
        }
    }
    fn try_handle(
        &mut self,
        data: Message,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
    ) -> Result<Vec<Wire>, String> {
        if let Message::Text(text) = data {
            return self.legacy_input(&text, engine, at);
        }
        let Message::Binary(bytes) = data else {
            return Ok(Vec::new());
        };
        let bundle =
            fb::root::<sx::MessageBundle>(&bytes).map_err(|_| "invalid SolarXR message")?;
        let mut out = Vec::new();
        if let Some(headers) = bundle.rpc_msgs() {
            if headers.len() > 32 {
                return Err("too many commands".into());
            }
            for h in headers {
                let tx = h.tx_id().map_or(0, |id| id.id());
                let kind = h.message_type();
                let level = if kind == rpc::RpcMessage::HeartbeatRequest {
                    crate::log_level::LogLevel::Trace
                } else {
                    crate::log_level::LogLevel::Debug
                };
                if crate::logging::enabled(level) {
                    crate::logging::diagnostic(
                        level,
                        &json!({"type":"api_request", "at_ms":at, "request":format!("{kind:?}"), "tx":tx}),
                    );
                }
                if self.firmware.serial_busy()
                    && matches!(
                        kind,
                        rpc::RpcMessage::OpenSerialRequest
                            | rpc::RpcMessage::CloseSerialRequest
                            | rpc::RpcMessage::SetWifiRequest
                            | rpc::RpcMessage::StartWifiProvisioningRequest
                            | rpc::RpcMessage::SerialTrackerRebootRequest
                            | rpc::RpcMessage::SerialTrackerFactoryResetRequest
                            | rpc::RpcMessage::SerialTrackerGetInfoRequest
                            | rpc::RpcMessage::SerialTrackerGetWifiScanRequest
                            | rpc::RpcMessage::SerialTrackerCustomCommandRequest
                    )
                {
                    return Err("serial port is reserved for firmware updating".into());
                }
                match kind {
                    rpc::RpcMessage::HeartbeatRequest
                    | rpc::RpcMessage::EnableSteamVRDriverRequest
                    | rpc::RpcMessage::TrackingChecklistRequest
                    | rpc::RpcMessage::IgnoreTrackingChecklistStepRequest
                    | rpc::RpcMessage::StatusSystemRequest
                    | rpc::RpcMessage::VRCConfigStateRequest
                    | rpc::RpcMessage::VRCConfigSettingToggleMute
                    | rpc::RpcMessage::OverlayDisplayModeRequest
                    | rpc::RpcMessage::OverlayDisplayModeChangeRequest
                    | rpc::RpcMessage::ServerInfosRequest
                    | rpc::RpcMessage::InstalledInfoRequest => {
                        self.rpc_system(h, engine, tx, &mut out)?
                    }
                    rpc::RpcMessage::MagToggleRequest
                    | rpc::RpcMessage::ChangeMagToggleRequest
                    | rpc::RpcMessage::AssignTrackerRequest
                    | rpc::RpcMessage::ForgetDeviceRequest
                    | rpc::RpcMessage::AddUnknownDeviceRequest => {
                        self.rpc_trackers(h, receiver, engine, at, tx, &mut out)?
                    }
                    rpc::RpcMessage::SettingsRequest
                    | rpc::RpcMessage::ChangeSettingsRequest
                    | rpc::RpcMessage::SkeletonConfigRequest
                    | rpc::RpcMessage::ChangeSkeletonConfigRequest
                    | rpc::RpcMessage::SkeletonResetAllRequest
                    | rpc::RpcMessage::SettingsResetRequest
                    | rpc::RpcMessage::KeybindRequest
                    | rpc::RpcMessage::ChangeKeybindRequest => {
                        self.rpc_configuration(h, receiver, engine, at, tx, &mut out)?
                    }
                    rpc::RpcMessage::ClearMountingResetRequest
                    | rpc::RpcMessage::ResetRequest
                    | rpc::RpcMessage::SetPauseTrackingRequest
                    | rpc::RpcMessage::TrackingPauseStateRequest
                    | rpc::RpcMessage::EnableStayAlignedRequest
                    | rpc::RpcMessage::LegTweaksTmpChange
                    | rpc::RpcMessage::LegTweaksTmpClear
                    | rpc::RpcMessage::DetectStayAlignedRelaxedPoseRequest
                    | rpc::RpcMessage::ResetStayAlignedRelaxedPoseRequest
                    | rpc::RpcMessage::HeightRequest
                    | rpc::RpcMessage::ClearDriftCompensationRequest
                    | rpc::RpcMessage::StartUserHeightCalibration
                    | rpc::RpcMessage::CancelUserHeightCalibration => {
                        self.rpc_calibration(h, receiver, engine, at, tx, &mut out)?
                    }
                    rpc::RpcMessage::FirmwareUpdateRequest
                    | rpc::RpcMessage::FirmwareUpdateStopQueuesRequest
                    | rpc::RpcMessage::StartWifiProvisioningRequest
                    | rpc::RpcMessage::StopWifiProvisioningRequest
                    | rpc::RpcMessage::SerialDevicesRequest
                    | rpc::RpcMessage::OpenSerialRequest
                    | rpc::RpcMessage::CloseSerialRequest
                    | rpc::RpcMessage::SetWifiRequest
                    | rpc::RpcMessage::SerialTrackerRebootRequest
                    | rpc::RpcMessage::SerialTrackerFactoryResetRequest
                    | rpc::RpcMessage::SerialTrackerGetInfoRequest
                    | rpc::RpcMessage::SerialTrackerGetWifiScanRequest
                    | rpc::RpcMessage::SerialTrackerCustomCommandRequest => {
                        self.rpc_hardware(h, receiver, at, tx, &mut out)?
                    }
                    rpc::RpcMessage::AutoBoneProcessRequest
                    | rpc::RpcMessage::AutoBoneStopRecordingRequest
                    | rpc::RpcMessage::AutoBoneCancelRecordingRequest
                    | rpc::RpcMessage::AutoBoneApplyRequest
                    | rpc::RpcMessage::RecordBVHRequest
                    | rpc::RpcMessage::RecordBVHStatusRequest => {
                        self.rpc_recording(h, receiver, engine, at, tx, &mut out)?
                    }
                    _ => {
                        return Err(format!(
                            "{} is not available in the connected backend",
                            kind.variant_name().unwrap_or("operation")
                        ))
                    }
                }
            }
        }
        if bundle.pub_sub_msgs().is_some_and(|m| !m.is_empty()) {
            // Pub/sub is dispatched by the connection, which owns subscription identity.
            // Native RPC clients use request/response messages on this endpoint.
        }
        Ok(out.into_iter().map(Wire::Binary).collect())
    }
}
