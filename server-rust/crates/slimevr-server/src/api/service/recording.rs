//! AutoBone recording lifecycle and BVH export.
use super::Service;
use crate::api::{
    protocol::{self, rpc_frame},
    Request, Wire,
};
use serde_json::json;
use slimevr_core::autobone::{AutoBoneResult, Epoch};
use solarxr_protocol::rpc;
use std::path::{Path, PathBuf};

impl Service {
    pub fn finish_bvh(&mut self) {
        if let Some(mut recorder) = self.bvh.take() {
            match recorder.finish() {
                Ok(saved) => {
                    crate::logging::diagnostic(
                        crate::log_level::LogLevel::Info,
                        &json!({"type":"bvh_saved", "path":saved.path, "frames":saved.frames}),
                    );
                    let _ = self.events.send(Wire::Text(
                        json!({
                            "type": "backend_file_saved", "path": saved.path, "frames": saved.frames
                        })
                        .to_string(),
                    ));
                }
                Err(e) => self.error(format!("Unable to finalize BVH recording: {e}")),
            }
            self.broadcast(bvh_status(0, false));
        }
    }
    pub(super) fn recording_root(&self) -> PathBuf {
        self.state_path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or(Path::new("."))
            .to_owned()
    }
    pub(super) fn save_recording(&mut self, record: bool) {
        let recording = crate::pose_recording::Recording::from_motion(
            &self.frames,
            self.config.sample_ms as f32 / 1000.0,
        );
        let dir = self.recording_root().join("AutoBone Recordings");
        let numbered = !record || self.config.save_recordings || self.save_after_record;
        self.saving = true;
        let count = self.frames.len();
        let sender = self.commands.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut paths = Vec::new();
                if record {
                    paths.push(
                        crate::pose_recording::save_last(&dir, &recording)
                            .map_err(|e| e.to_string())?,
                    );
                }
                if numbered {
                    paths.push(
                        crate::pose_recording::save_next(&dir, &recording)
                            .map_err(|e| e.to_string())?,
                    );
                }
                Ok(paths)
            })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
            let _ = sender
                .send(Request::AutoBoneSaved {
                    result,
                    record,
                    count,
                })
                .await;
        });
    }
    pub fn auto_saved(&mut self, result: Result<Vec<PathBuf>, String>, record: bool, count: usize) {
        self.saving = false;
        let success = result.is_ok();
        match result {
            Ok(paths) => {
                for path in paths {
                    let _ = self.events.send(Wire::Text(
                        json!({"type":"backend_file_saved", "kind":"autobone", "path":path,"frames":count})
                            .to_string(),
                    ));
                }
            }
            Err(e) => self.error(format!("Unable to save AutoBone recording: {e}")),
        }
        self.broadcast(auto_status(
            if record {
                rpc::AutoBoneProcessType::RECORD
            } else {
                rpc::AutoBoneProcessType::SAVE
            },
            count,
            count.max(1),
            true,
            success,
        ));
        if record && self.save_after_record {
            self.broadcast(auto_status(
                rpc::AutoBoneProcessType::SAVE,
                count,
                count.max(1),
                true,
                success,
            ));
        }
        self.save_after_record = false;
    }
    pub fn auto_epoch(&self, epoch: &Epoch, total: u32) {
        self.broadcast(rpc_frame(rpc::RpcMessage::AutoBoneEpochResponse, 0, |f| {
            let parts = protocol::skeleton_parts(f, epoch.skeleton);
            rpc::AutoBoneEpochResponse::create(
                f,
                &rpc::AutoBoneEpochResponseArgs {
                    current_epoch: epoch.epoch,
                    total_epochs: total,
                    epoch_error: epoch.mean_error,
                    adjusted_skeleton_parts: Some(parts),
                },
            )
            .as_union_value()
        }));
    }
    pub fn auto_done(&mut self, result: Result<AutoBoneResult, String>) {
        self.processing = false;
        match result {
            Ok(r) => {
                let success = r.accepted;
                self.result = Some(r);
                self.broadcast(auto_status(
                    rpc::AutoBoneProcessType::PROCESS,
                    1,
                    1,
                    true,
                    success,
                ));
                if !success {
                    self.error("AutoBone error exceeds the acceptance threshold");
                }
            }
            Err(e) => {
                self.broadcast(auto_status(
                    rpc::AutoBoneProcessType::PROCESS,
                    0,
                    1,
                    true,
                    false,
                ));
                self.error(e);
            }
        }
    }
}
pub(super) fn auto_status(
    kind: rpc::AutoBoneProcessType,
    current: usize,
    total: usize,
    completed: bool,
    success: bool,
) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::AutoBoneProcessStatusResponse, 0, |f| {
        rpc::AutoBoneProcessStatusResponse::create(
            f,
            &rpc::AutoBoneProcessStatusResponseArgs {
                process_type: kind,
                current: current as u32,
                total: total as u32,
                completed,
                success,
                eta: if completed { 0.0 } else { -1.0 },
            },
        )
        .as_union_value()
    })
}
pub(super) fn bvh_status(tx: u32, recording: bool) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::RecordBVHStatus, tx, |f| {
        rpc::RecordBVHStatus::create(f, &rpc::RecordBVHStatusArgs { recording }).as_union_value()
    })
}
