//! SteamVR sharing and driver status notifications.
use super::Service;
use crate::api::{diagnostics, settings};
use serde_json::json;
use slimevr_core::pose::PoseSnapshot;

impl Service {
    pub fn steamvr_auto_share(&mut self, pose: &PoseSnapshot) -> Result<(), String> {
        let mut config = self.config.clone();
        if config.steam_vr.update_automatic(pose) {
            config.save(self.state_path.as_deref())?;
            self.config = config;
            self.broadcast(settings::frame(0, &self.config));
        }
        Ok(())
    }
    pub fn steamvr_status(&mut self, status: crate::steamvr::Status, pose: &PoseSnapshot) {
        if self.steam_vr != status {
            let level =
                if (self.steam_vr.connected && !status.connected) || status.last_error.is_some() {
                    crate::log_level::LogLevel::Warn
                } else {
                    crate::log_level::LogLevel::Info
                };
            crate::logging::diagnostic(level, &json!({"type":"steamvr_status", "status":status}));
            self.steam_vr = status;
            self.broadcast(diagnostics::frame(
                0,
                pose,
                &self.config,
                &self.steam_vr,
                &self.driver_status,
                &self.diagnostics,
            ));
        }
    }
    pub fn driver_update(
        &mut self,
        status: crate::steamvr::manager::DriverStatus,
        error: Option<String>,
        pose: &PoseSnapshot,
    ) {
        if let Some(e) = error {
            self.error(e);
        }
        if self.driver_status.registration_error != status.registration_error {
            if let Some(e) = &status.registration_error {
                self.error(format!("SteamVR driver registration failed: {e}"));
            }
        }
        if self.driver_status != status {
            crate::logging::diagnostic(
                crate::log_level::LogLevel::Info,
                &json!({"type":"steamvr_driver_status", "status":status}),
            );
            self.driver_status = status;
            self.broadcast(diagnostics::frame(
                0,
                pose,
                &self.config,
                &self.steam_vr,
                &self.driver_status,
                &self.diagnostics,
            ));
        }
    }
}
