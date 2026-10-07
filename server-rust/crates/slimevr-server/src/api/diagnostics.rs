//! Original tracking checklist and status messages, derived from active inputs.
use super::{protocol::rpc_frame, FrontendConfig};
use crate::steamvr::{manager::DriverStatus, Status};
use slimevr_core::{
    pose::{PoseSnapshot, TrackerPose},
    skeleton::BodyPosition as B,
    SensorStatus,
};
use solarxr_protocol::{datatypes as dt, flatbuffers as fb, rpc};
use std::collections::BTreeMap;
#[derive(Default, Clone, Debug, PartialEq)]
pub struct Context {
    pub vrchat: Option<bool>,
    pub rest: BTreeMap<(String, u8), Option<bool>>,
    pub public_networks: Vec<String>,
    pub network_supported: bool,
    pub controllers: bool,
    pub udev: bool,
    pub wayland: bool,
}
impl Context {
    pub fn detect() -> Self {
        let udev = cfg!(target_os = "linux")
            && [
                "/etc/udev/rules.d",
                "/usr/lib/udev/rules.d",
                "/lib/udev/rules.d",
            ]
            .into_iter()
            .any(|dir| {
                std::fs::read_dir(dir).ok().is_some_and(|entries| {
                    entries.filter_map(Result::ok).any(|e| {
                        std::fs::read_to_string(e.path())
                            .ok()
                            .is_some_and(|s| s.to_lowercase().contains("slime"))
                    })
                })
            });
        Self {
            udev,
            wayland: std::env::var("XDG_SESSION_TYPE").is_ok_and(|s| s == "wayland")
                || std::env::var_os("WAYLAND_DISPLAY").is_some(),
            network_supported: false,
            ..Default::default()
        }
    }
}
fn ids<'a>(
    f: &mut fb::FlatBufferBuilder<'a>,
    trackers: &[&TrackerPose],
    c: &FrontendConfig,
) -> fb::WIPOffset<fb::Vector<'a, fb::ForwardsUOffset<dt::TrackerId<'a>>>> {
    let values = trackers
        .iter()
        .filter_map(|t| {
            c.device_ids.get(&t.device_key).map(|id| {
                let device = dt::DeviceId::new(*id);
                dt::TrackerId::create(
                    f,
                    &dt::TrackerIdArgs {
                        device_id: Some(&device),
                        tracker_num: t.sensor_id,
                    },
                )
            })
        })
        .collect::<Vec<_>>();
    f.create_vector(&values)
}
pub fn ignorable(id: u8) -> bool {
    matches!(id, 1 | 3 | 4 | 7 | 8 | 9 | 10 | 12)
}
pub fn frame(
    tx: u32,
    p: &PoseSnapshot,
    c: &FrontendConfig,
    steam: &Status,
    driver: &DriverStatus,
    context: &Context,
) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::TrackingChecklistResponse, tx, |f| {
        use rpc::{TrackingChecklistExtraData as E, TrackingChecklistStepId as I};
        let active = p
            .trackers
            .iter()
            .filter(|t| {
                t.status != SensorStatus::Disconnected
                    && context
                        .rest
                        .contains_key(&(t.device_key.clone(), t.sensor_id))
            })
            .collect::<Vec<_>>();
        let imu = active
            .iter()
            .copied()
            .filter(|t| t.capabilities.is_imu && !p.flex_rotations.contains_key(&t.body))
            .collect::<Vec<_>>();
        let errors = imu
            .iter()
            .copied()
            .filter(|t| t.status == SensorStatus::Error)
            .collect::<Vec<_>>();
        let need_reset = imu
            .iter()
            .copied()
            .filter(|t| t.status != SensorStatus::Error && !t.calibration.full_reset_done)
            .collect::<Vec<_>>();
        let calibration = imu
            .iter()
            .copied()
            .filter(|t| {
                context.rest.get(&(t.device_key.clone(), t.sensor_id)) == Some(&Some(false))
            })
            .collect::<Vec<_>>();
        let automatic = c.yaml["resetsConfig"]["lastMountingMethod"]
            .as_str()
            .unwrap_or("AUTOMATIC")
            != "MANUAL";
        let saved = c.pose.save_mounting_reset
            && imu
                .iter()
                .all(|t| t.calibration.mount_rot_fix != slimevr_core::Quaternion::IDENTITY);
        let needs_mounting = automatic
            && !saved
            && !p.mounting_completed
            && !p
                .last_full_reset_ms
                .is_some_and(|at| p.at_ms.saturating_sub(at) < 120000);
        let feet = imu
            .iter()
            .filter(|t| matches!(t.body, B::LeftFoot | B::RightFoot))
            .collect::<Vec<_>>();
        let mut steps = Vec::new();
        macro_rules! add {
            ($f:expr,$id:expr,$valid:expr,$enabled:expr,$optional:expr,$visibility:expr,$kind:expr,$extra:expr) => {
                steps.push(rpc::TrackingChecklistStep::create(
                    $f,
                    &rpc::TrackingChecklistStepArgs {
                        id: $id,
                        valid: $valid,
                        enabled: $enabled,
                        optional: $optional,
                        visibility: $visibility,
                        ignorable: ignorable($id.0),
                        extra_data_type: $kind,
                        extra_data: $extra,
                    },
                ));
            };
        }

        let always = rpc::TrackingChecklistStepVisibility::ALWAYS;
        let invalid = rpc::TrackingChecklistStepVisibility::WHEN_INVALID;
        let extra = if context.public_networks.is_empty() {
            None
        } else {
            let names = context
                .public_networks
                .iter()
                .map(|s| f.create_string(s))
                .collect::<Vec<_>>();
            let adapters = f.create_vector(&names);
            Some(
                rpc::TrackingChecklistPublicNetworks::create(
                    f,
                    &rpc::TrackingChecklistPublicNetworksArgs {
                        adapters: Some(adapters),
                    },
                )
                .as_union_value(),
            )
        };
        add!(
            f,
            I::NETWORK_PROFILE_PUBLIC,
            context.public_networks.is_empty(),
            context.network_supported,
            false,
            invalid,
            if extra.is_some() {
                E::TrackingChecklistPublicNetworks
            } else {
                E::NONE
            },
            extra
        );
        let extra = if driver.known && !steam.connected {
            let name = f.create_string("steamvr");
            Some(
                rpc::TrackingChecklistSteamVRDisconnected::create(
                    f,
                    &rpc::TrackingChecklistSteamVRDisconnectedArgs {
                        bridge_settings_name: Some(name),
                        driver_installed: driver.installed,
                        driver_enabled: driver.enabled,
                        driver_blocked_by_safe_mode: driver.blocked,
                    },
                )
                .as_union_value(),
            )
        } else {
            None
        };
        add!(
            f,
            I::STEAMVR_DISCONNECTED,
            steam.connected,
            steam.available,
            false,
            invalid,
            if extra.is_some() {
                E::TrackingChecklistSteamVRDisconnected
            } else {
                E::NONE
            },
            extra
        );
        let hands = c
            .steam_vr
            .trackers
            .get("left_hand")
            .copied()
            .unwrap_or(false)
            || c.steam_vr
                .trackers
                .get("right_hand")
                .copied()
                .unwrap_or(false);
        let controllers = context.controllers;
        let hand_trackers = imu
            .iter()
            .any(|t| matches!(t.body, B::LeftHand | B::RightHand));
        add!(
            f,
            I::STEAMVR_HANDS_ENABLED,
            !steam.connected || !hands || (!controllers && hand_trackers),
            steam.available,
            false,
            invalid,
            E::NONE,
            None
        );
        add!(
            f,
            I::STANDABLE_INSTALLED,
            !driver.standable_installed,
            steam.available,
            false,
            invalid,
            E::NONE,
            None
        );
        let extra = if errors.is_empty() {
            None
        } else {
            let trackers_id = ids(f, &errors, c);
            Some(
                rpc::TrackingChecklistTrackerError::create(
                    f,
                    &rpc::TrackingChecklistTrackerErrorArgs {
                        trackers_id: Some(trackers_id),
                    },
                )
                .as_union_value(),
            )
        };
        add!(
            f,
            I::TRACKER_ERROR,
            errors.is_empty(),
            true,
            false,
            invalid,
            if extra.is_some() {
                E::TrackingChecklistTrackerError
            } else {
                E::NONE
            },
            extra
        );
        let extra = if calibration.is_empty() {
            None
        } else {
            let trackers_id = ids(f, &calibration, c);
            Some(
                rpc::TrackingChecklistNeedCalibration::create(
                    f,
                    &rpc::TrackingChecklistNeedCalibrationArgs {
                        trackers_id: Some(trackers_id),
                    },
                )
                .as_union_value(),
            )
        };
        let supported = imu.iter().any(|t| {
            context
                .rest
                .get(&(t.device_key.clone(), t.sensor_id))
                .is_some_and(Option::is_some)
        });
        add!(
            f,
            I::TRACKERS_REST_CALIBRATION,
            calibration.is_empty(),
            supported,
            false,
            always,
            if extra.is_some() {
                E::TrackingChecklistNeedCalibration
            } else {
                E::NONE
            },
            extra
        );
        let extra = if need_reset.is_empty() {
            None
        } else {
            let trackers_id = ids(f, &need_reset, c);
            Some(
                rpc::TrackingChecklistTrackerReset::create(
                    f,
                    &rpc::TrackingChecklistTrackerResetArgs {
                        trackers_id: Some(trackers_id),
                    },
                )
                .as_union_value(),
            )
        };
        add!(
            f,
            I::FULL_RESET,
            need_reset.is_empty() && !needs_mounting,
            !imu.is_empty(),
            false,
            always,
            if extra.is_some() {
                E::TrackingChecklistTrackerReset
            } else {
                E::NONE
            },
            extra
        );
        add!(
            f,
            I::MOUNTING_CALIBRATION,
            p.mounting_completed || saved,
            automatic && !imu.is_empty(),
            false,
            always,
            E::NONE,
            None
        );
        add!(
            f,
            I::FEET_MOUNTING_CALIBRATION,
            p.feet_mounting_completed || saved,
            automatic && !c.pose.reset_mounting_feet && !feet.is_empty(),
            false,
            always,
            E::NONE,
            None
        );
        add!(
            f,
            I::UNASSIGNED_HMD,
            true,
            true,
            false,
            invalid,
            E::NONE,
            None
        ); // External HMD is assigned when admitted.
        add!(
            f,
            I::STAY_ALIGNED_CONFIGURED,
            c.pose.alignment.enabled,
            true,
            true,
            invalid,
            E::NONE,
            None
        );
        add!(
            f,
            I::VRCHAT_SETTINGS,
            context.vrchat.unwrap_or(true),
            context.vrchat.is_some(),
            true,
            invalid,
            E::NONE,
            None
        ); // Updated by VRChat manager when available.
        let steps = f.create_vector(&steps);
        let ignored = c.ignored_steps.iter().map(|i| I(*i)).collect::<Vec<_>>();
        let ignored_steps = f.create_vector(&ignored);
        rpc::TrackingChecklistResponse::create(
            f,
            &rpc::TrackingChecklistResponseArgs {
                steps: Some(steps),
                ignored_steps: Some(ignored_steps),
            },
        )
        .as_union_value()
    })
}

/// OS inspection happens outside the pose owner; failures never report a public adapter as private.
pub async fn inspect() -> Context {
    let mut context = tokio::task::spawn_blocking(Context::detect)
        .await
        .unwrap_or_default();
    #[cfg(windows)]
    {
        let mut command = tokio::process::Command::new("powershell.exe");
        command.args(["-NoProfile","-NonInteractive","-Command","$ErrorActionPreference='Stop'; @((Get-NetConnectionProfile | Where-Object NetworkCategory -eq 'Public').InterfaceAlias) | ConvertTo-Json -Compress"]);
        command.creation_flags(0x08000000).kill_on_drop(true);
        context.network_supported = false;
        if let Ok(Ok(output)) =
            tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await
        {
            if output.status.success() {
                if let Ok(names) = serde_json::from_slice::<Vec<String>>(&output.stdout) {
                    context.public_networks = names;
                    context.network_supported = true;
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        context.network_supported = false;
    }
    context
}
pub fn monitor(sender: tokio::sync::mpsc::Sender<super::Request>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut clock = tokio::time::interval(std::time::Duration::from_secs(10));
        clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            clock.tick().await;
            if sender
                .send(super::Request::Diagnostics(inspect().await))
                .await
                .is_err()
            {
                break;
            }
        }
    })
}
