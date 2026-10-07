//! Messages crossing the transport / single-owner runtime boundary.
use crate::{
    api::{diagnostics, vrchat, FrontendConfig},
    receiver::DeviceState,
};
use serde_json::json;
use slimevr_core::{
    autobone::{AutoBoneResult, Epoch},
    pose::PoseSnapshot,
    skeleton::{BodyPosition as B, HeadPose},
};
use std::{collections::BTreeMap, path::PathBuf};
use tokio::sync::oneshot;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone)]
pub struct LiveState {
    pub at: u64,
    pub devices: BTreeMap<String, DeviceState>,
    pub config: FrontendConfig,
    pub pose: PoseSnapshot,
    pub external: BTreeMap<B, HeadPose>,
    pub persistent: bool,
    pub steam_vr: crate::steamvr::Status,
    pub driver_status: crate::steamvr::manager::DriverStatus,
}
#[derive(Clone, Debug)]
pub enum Wire {
    PubSub {
        origin: u64,
        handle: u16,
        bytes: Vec<u8>,
    },
    Binary(Vec<u8>),
    Text(String),
    Serial(Vec<u8>),
    Provisioning(Vec<u8>),
}
impl Wire {
    pub(super) fn message(self) -> Message {
        match self {
            Self::Binary(b) | Self::Serial(b) | Self::Provisioning(b) => Message::Binary(b.into()),
            Self::PubSub { bytes, .. } => Message::Binary(bytes.into()),
            Self::Text(s) => Message::Text(s.into()),
        }
    }
}
pub enum Request {
    Hotkey {
        action: crate::hotkeys::Action,
        delay_ms: u64,
    },
    HotkeyError(String),
    Vrchat(Option<vrchat::Values>),
    Osc(crate::osc::Event),
    Hid(crate::hid::Event),
    Diagnostics(diagnostics::Context),
    Firmware(crate::firmware::Event),
    Serial(crate::serial::Event),
    Client {
        data: Message,
        reply: oneshot::Sender<Vec<Wire>>,
    },
    AutoBoneEpoch {
        epoch: Box<Epoch>,
        total: u32,
    },
    DriverStatus {
        status: crate::steamvr::manager::DriverStatus,
        error: Option<String>,
    },
    AutoBoneDone(Result<AutoBoneResult, String>),
    AutoBoneSaved {
        result: Result<Vec<PathBuf>, String>,
        record: bool,
        count: usize,
    },
}
pub(super) fn error_wire(message: String) -> Wire {
    Wire::Text(json!({"type":"backend_error","message":message}).to_string())
}
