//! AutoBone and BVH requests.
use super::super::{
    recording::{auto_status, bvh_status},
    Service,
};
use crate::{api::Request, receiver::Receiver};
use serde_json::json;
use slimevr_core::{autobone, pose::PoseEngine};
use solarxr_protocol::rpc;
use std::path::{Path, PathBuf};

impl Service {
    pub(super) fn rpc_recording(
        &mut self,
        h: rpc::RpcMessageHeader<'_>,
        receiver: &mut Receiver,
        engine: &mut PoseEngine,
        at: u64,
        tx: u32,
        out: &mut Vec<Vec<u8>>,
    ) -> Result<(), String> {
        match h.message_type() {
            rpc::RpcMessage::AutoBoneProcessRequest => {
                let r = h
                    .message_as_auto_bone_process_request()
                    .ok_or("missing AutoBone operation")?;
                match r.process_type() {
                    rpc::AutoBoneProcessType::RECORD => {
                        if self.processing || self.recording || self.saving {
                            return Err("AutoBone is busy".into());
                        }
                        engine.motion_frame()?;
                        self.frames.clear();
                        self.result = None;
                        self.recording = true;
                        self.next_sample = at;
                        out.push(auto_status(
                            rpc::AutoBoneProcessType::RECORD,
                            0,
                            self.config.sample_count,
                            false,
                            false,
                        ));
                    }
                    rpc::AutoBoneProcessType::PROCESS => {
                        if self.processing || self.recording || self.saving {
                            return Err("AutoBone is busy".into());
                        }
                        self.processing = true;
                        self.result = None;
                        let frames = self.frames.clone();
                        let root = self.recording_root();
                        let c = self.config.clone();
                        let sender = self.commands.clone();
                        tokio::spawn(async move {
                            let progress = sender.clone();
                            let result = tokio::task::spawn_blocking(move || {
                                let mut recordings = crate::pose_recording::load_directory(
                                    &root.join("Load AutoBone Recordings"),
                                )
                                .map_err(|e| e.to_string())?;
                                if recordings.is_empty() {
                                    let frames = if frames.len() >= 3 {
                                        frames
                                    } else {
                                        crate::pose_recording::load(
                                            &root.join("AutoBone Recordings/LastABRecording.pfs"),
                                        )
                                        .and_then(|r| r.motion_frames())
                                        .map_err(|e| format!("No usable AutoBone recording: {e}"))?
                                    };
                                    recordings.push((PathBuf::from("<Recording>"), frames));
                                }
                                let mut last = None;
                                for (path, frames) in recordings {
                                    let result = autobone::optimize_with_progress(
                                        c.pose.skeleton,
                                        &frames,
                                        c.pose.hmd_height,
                                        c.auto_bone,
                                        |epoch| {
                                            let _ =
                                                progress.blocking_send(Request::AutoBoneEpoch {
                                                    epoch: Box::new(epoch.clone()),
                                                    total: c.auto_bone.epochs,
                                                });
                                        },
                                    )
                                    .map_err(|e| format!("{}: {e}", path.display()))?;
                                    if !result.accepted {
                                        return Err(format!(
                                            "{}: AutoBone error exceeds the acceptance threshold",
                                            path.display()
                                        ));
                                    }
                                    last = Some(result);
                                }
                                last.ok_or_else(|| "No AutoBone recordings".to_string())
                            })
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()));
                            let _ = sender.send(Request::AutoBoneDone(result)).await;
                        });
                        out.push(auto_status(
                            rpc::AutoBoneProcessType::PROCESS,
                            0,
                            self.config.auto_bone.epochs as usize,
                            false,
                            false,
                        ));
                    }
                    rpc::AutoBoneProcessType::SAVE => {
                        if self.saving {
                            return Err("AutoBone recording save is busy".into());
                        }
                        if self.recording {
                            self.save_after_record = true;
                        } else {
                            if self.frames.is_empty() {
                                return Err("No AutoBone recording to save".into());
                            }
                            self.save_recording(false);
                        }
                        out.push(auto_status(
                            rpc::AutoBoneProcessType::SAVE,
                            0,
                            1,
                            false,
                            false,
                        ));
                    }
                    _ => return Err("unsupported AutoBone operation".into()),
                }
            }
            rpc::RpcMessage::AutoBoneStopRecordingRequest => {
                if self.recording {
                    self.recording = false;
                    if self.frames.len() >= 3 {
                        self.save_recording(true);
                    } else {
                        if self.save_after_record {
                            self.broadcast(auto_status(
                                rpc::AutoBoneProcessType::SAVE,
                                0,
                                1,
                                true,
                                false,
                            ));
                            self.save_after_record = false;
                        }
                        self.broadcast(auto_status(
                            rpc::AutoBoneProcessType::RECORD,
                            self.frames.len(),
                            self.config.sample_count,
                            true,
                            false,
                        ));
                    }
                }
            }
            rpc::RpcMessage::AutoBoneCancelRecordingRequest => {
                if self.recording {
                    self.recording = false;
                    self.frames.clear();
                    if self.save_after_record {
                        self.broadcast(auto_status(
                            rpc::AutoBoneProcessType::SAVE,
                            0,
                            1,
                            true,
                            false,
                        ));
                    }
                    self.save_after_record = false;
                    self.broadcast(auto_status(
                        rpc::AutoBoneProcessType::RECORD,
                        0,
                        self.config.sample_count,
                        true,
                        false,
                    ));
                }
            }
            rpc::RpcMessage::AutoBoneApplyRequest => {
                let result = self
                    .result
                    .as_ref()
                    .filter(|r| r.accepted)
                    .ok_or("no accepted AutoBone calibration")?;
                let mut c = self.config.clone();
                c.pose.skeleton = result.skeleton;
                self.commit(c, engine, receiver, at)?;
            }
            rpc::RpcMessage::RecordBVHRequest => {
                let req = h
                    .message_as_record_bvhrequest()
                    .ok_or("missing BVH request")?;
                if req.stop() {
                    self.finish_bvh();
                } else if self.bvh.is_none() {
                    let path = if let Some(path) = req.path().filter(|p| !p.is_empty()) {
                        PathBuf::from(path)
                    } else {
                        let parent = self
                            .state_path
                            .as_deref()
                            .and_then(Path::parent)
                            .filter(|p| !p.as_os_str().is_empty())
                            .unwrap_or_else(|| Path::new("."));
                        let path = parent.join("recordings");
                        std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
                        path
                    };
                    self.bvh = Some(
                        crate::bvh::Recorder::start(&path, &engine.snapshot().skeleton)
                            .map_err(|e| format!("Unable to start BVH recording: {e}"))?,
                    );
                    crate::logging::diagnostic(
                        crate::log_level::LogLevel::Info,
                        &json!({"type":"bvh_started", "path":path}),
                    );
                    self.broadcast(bvh_status(0, true));
                }
                out.push(bvh_status(tx, self.bvh.is_some()));
            }
            rpc::RpcMessage::RecordBVHStatusRequest => out.push(bvh_status(tx, self.bvh.is_some())),
            _ => unreachable!("RPC routed to the wrong domain"),
        }
        Ok(())
    }
}
