//! Compatibility for the original JSON WebSocket bridge.
use super::Service;
use crate::api::Wire;
use serde::Deserialize;
use serde_json::json;
use slimevr_core::{
    calibration::ResetKind,
    pose::{PoseEngine, SceneInput},
    skeleton::{BodyPosition as B, HeadPose},
    Quaternion as Q, Vector3 as V,
};

impl Service {
    pub(super) fn legacy_input(
        &mut self,
        text: &str,
        engine: &mut PoseEngine,
        at: u64,
    ) -> Result<Vec<Wire>, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|_| "invalid JSON command")?;
        match value["type"].as_str() {
            Some("config") => Ok(Vec::new()),
            Some("pos") => {
                if value["tracker_id"].as_i64() != Some(0) {
                    return Ok(Vec::new());
                }
                let number = |key: &str| -> Result<f32, String> {
                    let n = value[key]
                        .as_f64()
                        .ok_or_else(|| format!("missing {key}"))?
                        as f32;
                    if n.is_finite() {
                        Ok(n)
                    } else {
                        Err(format!("invalid {key}"))
                    }
                };
                // Preserve the original WebSocketVRBridge VRWorkout height offset.
                let position = V::new(number("x")?, number("y")? + 0.2, number("z")?);
                let rotation = Q::new(number("qw")?, number("qx")?, number("qy")?, number("qz")?);
                let command = json!({"type":"pose_input", "body":"head", "rotation":rotation, "position":position});
                self.input_pose(&command.to_string(), engine, at)?;
                Ok(crate::steamvr::ROLES.iter().enumerate().filter_map(|(i, (_, _, bone))| {
                    engine.snapshot().skeleton.computed.get(*bone).map(|p| Wire::Text(json!({
                        "type":"pos", "src":"full", "tracker_id":format!("SlimeVR Tracker {}", i + 1),
                        "x":p.position.x, "y":p.position.y, "z":p.position.z,
                        "qw":p.rotation.w, "qx":p.rotation.x, "qy":p.rotation.y, "qz":p.rotation.z
                    }).to_string()))
                }).collect())
            }
            Some("action") => {
                let input = match value["name"].as_str() {
                    Some("calibrate") => SceneInput::Reset {
                        at_ms: at,
                        kind: ResetKind::Yaw,
                    },
                    Some("full_calibrate") => SceneInput::Reset {
                        at_ms: at,
                        kind: ResetKind::Full,
                    },
                    Some("mounting_calibrate") => SceneInput::Reset {
                        at_ms: at,
                        kind: ResetKind::Mounting,
                    },
                    Some("mounting_clear") => SceneInput::ClearMounting { at_ms: at },
                    Some("toggle_pause_tracking") => SceneInput::Pause {
                        at_ms: at,
                        paused: !engine.snapshot().paused,
                    },
                    _ => return Ok(Vec::new()),
                };
                self.steamvr_input(input, engine)?;
                self.config.pose = engine.export_config();
                self.config.save(self.state_path.as_deref())?;
                Ok(Vec::new())
            }
            _ => self.input_pose(text, engine, at).map(|_| Vec::new()),
        }
    }
    fn input_pose(&mut self, text: &str, engine: &mut PoseEngine, at: u64) -> Result<(), String> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Input {
            PoseInput {
                body: B,
                rotation: Q,
                position: Option<V>,
            },
            PoseClear {
                body: B,
            },
        }
        match serde_json::from_str::<Input>(text).map_err(|_| "invalid external pose command")? {
            Input::PoseInput {
                body,
                rotation,
                position,
            } => {
                self.steam_sources.remove(&body);
                let input = if body == B::Head {
                    SceneInput::Head {
                        at_ms: at,
                        rotation,
                        position,
                    }
                } else {
                    SceneInput::Controller {
                        at_ms: at,
                        body,
                        rotation,
                        position: position.ok_or("controller position required")?,
                    }
                };
                engine.scene_input(input.clone())?;
                self.changes.push(input);
                self.external.insert(body, HeadPose { rotation, position });
            }
            Input::PoseClear { body } => {
                self.steam_sources.remove(&body);
                let input = SceneInput::ClearSource { at_ms: at, body };
                engine.scene_input(input.clone())?;
                self.changes.push(input);
                self.external.remove(&body);
            }
        }
        Ok(())
    }
}
