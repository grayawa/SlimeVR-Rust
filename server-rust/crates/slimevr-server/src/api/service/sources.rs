//! External pose sources and SteamVR input ownership.
use super::Service;
use crate::receiver::Receiver;
use slimevr_core::{
    pose::{PoseEngine, SceneInput},
    skeleton::{BodyPosition as B, HeadPose},
};

impl Service {
    pub fn source_input(
        &mut self,
        event: slimevr_core::InputEvent,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
    ) -> Result<(), String> {
        if !receiver.external(&event)? {
            return Ok(());
        }
        engine.ingest(&event)?;
        self.changes.push(SceneInput::Input {
            event: event.clone(),
        });
        if let slimevr_core::EventKind::ExternalTracker {
            device_key,
            sensor_id,
            name,
            body,
            ..
        } = &event.kind
        {
            let mut c = self.config.clone();
            let fresh = c.yaml["rust"]["sourceNames"][device_key].as_str() != Some(name);
            if fresh {
                crate::config::put(&mut c.yaml, &["rust", "sourceNames", device_key], name)
                    .map_err(|e| e.to_string())?;
                let original = &c.yaml["trackers"][name];
                let preferred = if original.is_null() {
                    *body
                } else {
                    original["designation"]
                        .as_str()
                        .and_then(|s| crate::config::body(s).ok())
                };
                if !c
                    .pose
                    .bindings
                    .iter()
                    .any(|b| b.device_key == *device_key && b.sensor_id == *sensor_id)
                {
                    if let Some(body) =
                        preferred.filter(|body| !c.pose.bindings.iter().any(|b| b.body == *body))
                    {
                        c.pose.bindings.push(slimevr_core::pose::TrackerBinding {
                            device_key: device_key.clone(),
                            sensor_id: *sensor_id,
                            body,
                            mounting: slimevr_core::Quaternion::IDENTITY,
                        });
                    }
                }
                if let Some(custom) = original["customName"].as_str() {
                    c.tracker_names
                        .insert(format!("{device_key}/{sensor_id}"), custom.into());
                }
                c.save(self.state_path.as_deref())?;
                engine.configure(event.at_ms, c.pose.clone())?;
                self.changes.push(SceneInput::Configure {
                    at_ms: event.at_ms,
                    config: Box::new(c.pose.clone()),
                });
                self.config = c;
            }
        }
        Ok(())
    }
    pub fn steamvr_input(
        &mut self,
        input: SceneInput,
        engine: &mut PoseEngine,
    ) -> Result<(), String> {
        if let SceneInput::ClearSource { body, .. } = &input {
            if !self.steam_sources.remove(body) {
                return Ok(());
            }
        }
        engine.scene_input(input.clone())?;
        match &input {
            SceneInput::Head {
                rotation, position, ..
            } => {
                self.steam_sources.insert(B::Head);
                self.external.insert(
                    B::Head,
                    HeadPose {
                        rotation: *rotation,
                        position: *position,
                    },
                );
            }
            SceneInput::Controller {
                body,
                rotation,
                position,
                ..
            } => {
                self.steam_sources.insert(*body);
                self.external.insert(
                    *body,
                    HeadPose {
                        rotation: *rotation,
                        position: Some(*position),
                    },
                );
            }
            SceneInput::ClearSource { body, .. } => {
                self.external.remove(body);
            }
            _ => {}
        }
        self.changes.push(input);
        Ok(())
    }
}
