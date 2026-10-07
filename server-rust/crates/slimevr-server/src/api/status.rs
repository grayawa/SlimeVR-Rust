//! Stable status IDs with original snapshot/update/fixed messages.
use super::{diagnostics::Context, protocol::rpc_frame, FrontendConfig};
use slimevr_core::{pose::PoseSnapshot, SensorStatus};
use solarxr_protocol::{datatypes as dt, flatbuffers as fb, rpc};
use std::collections::BTreeMap;
#[derive(Clone, PartialEq, Eq)]
pub enum Value {
    Reset(Vec<(u8, u8)>),
    Error(Vec<(u8, u8)>),
    Steam,
    Public(Vec<String>),
}
fn message<'a>(
    f: &mut fb::FlatBufferBuilder<'a>,
    id: u32,
    value: &Value,
) -> fb::WIPOffset<rpc::StatusMessage<'a>> {
    let (kind, data) = match value {
        Value::Steam => {
            let name = f.create_string("steamvr");
            (
                rpc::StatusData::StatusSteamVRDisconnected,
                rpc::StatusSteamVRDisconnected::create(
                    f,
                    &rpc::StatusSteamVRDisconnectedArgs {
                        bridge_settings_name: Some(name),
                    },
                )
                .as_union_value(),
            )
        }
        Value::Public(names) => {
            let names = names.iter().map(|n| f.create_string(n)).collect::<Vec<_>>();
            let adapters = f.create_vector(&names);
            (
                rpc::StatusData::StatusPublicNetwork,
                rpc::StatusPublicNetwork::create(
                    f,
                    &rpc::StatusPublicNetworkArgs {
                        adapters: Some(adapters),
                    },
                )
                .as_union_value(),
            )
        }
        Value::Reset(ids) | Value::Error(ids) => {
            let ids = ids
                .iter()
                .map(|(device, sensor)| {
                    let device = dt::DeviceId::new(*device);
                    dt::TrackerId::create(
                        f,
                        &dt::TrackerIdArgs {
                            device_id: Some(&device),
                            tracker_num: *sensor,
                        },
                    )
                })
                .collect::<Vec<_>>();
            let tracker_id = f.create_vector(&ids);
            if matches!(value, Value::Reset(_)) {
                (
                    rpc::StatusData::StatusTrackerReset,
                    rpc::StatusTrackerReset::create(
                        f,
                        &rpc::StatusTrackerResetArgs {
                            tracker_id: Some(tracker_id),
                        },
                    )
                    .as_union_value(),
                )
            } else {
                (
                    rpc::StatusData::StatusTrackerError,
                    rpc::StatusTrackerError::create(
                        f,
                        &rpc::StatusTrackerErrorArgs {
                            tracker_id: Some(tracker_id),
                        },
                    )
                    .as_union_value(),
                )
            }
        }
    };
    rpc::StatusMessage::create(
        f,
        &rpc::StatusMessageArgs {
            id,
            prioritized: matches!(value, Value::Error(_) | Value::Public(_)),
            data_type: kind,
            data: Some(data),
        },
    )
}
#[derive(Default)]
pub struct Store {
    values: BTreeMap<u32, Value>,
}
impl Store {
    pub fn refresh(
        &mut self,
        p: &PoseSnapshot,
        c: &FrontendConfig,
        steam: &crate::steamvr::Status,
        context: &Context,
    ) -> Vec<Vec<u8>> {
        let mut next = BTreeMap::new();
        let mut reset = Vec::new();
        let mut errors = Vec::new();
        for t in &p.trackers {
            if t.status == SensorStatus::Disconnected
                || !context
                    .rest
                    .contains_key(&(t.device_key.clone(), t.sensor_id))
            {
                continue;
            }
            if let Some(id) = c.device_ids.get(&t.device_key) {
                if t.status == SensorStatus::Error {
                    errors.push((*id, t.sensor_id));
                } else if !t.calibration.full_reset_done {
                    reset.push((*id, t.sensor_id));
                }
            }
        }
        if !reset.is_empty() {
            next.insert(1, Value::Reset(reset));
        }
        if !errors.is_empty() {
            next.insert(2, Value::Error(errors));
        }
        if steam.available && !steam.connected {
            next.insert(3, Value::Steam);
        }
        if !context.public_networks.is_empty() {
            next.insert(5, Value::Public(context.public_networks.clone()));
        }
        let mut frames = Vec::new();
        for (id, value) in &next {
            if self.values.get(id) != Some(value) {
                frames.push(rpc_frame(rpc::RpcMessage::StatusSystemUpdate, 0, |f| {
                    let status = message(f, *id, value);
                    rpc::StatusSystemUpdate::create(
                        f,
                        &rpc::StatusSystemUpdateArgs {
                            new_status: Some(status),
                        },
                    )
                    .as_union_value()
                }));
            }
        }
        for id in self.values.keys().filter(|id| !next.contains_key(id)) {
            frames.push(rpc_frame(rpc::RpcMessage::StatusSystemFixed, 0, |f| {
                rpc::StatusSystemFixed::create(
                    f,
                    &rpc::StatusSystemFixedArgs {
                        fixed_status_id: *id,
                    },
                )
                .as_union_value()
            }));
        }
        self.values = next;
        frames
    }
    pub fn frame(&self, tx: u32) -> Vec<u8> {
        rpc_frame(rpc::RpcMessage::StatusSystemResponse, tx, |f| {
            let statuses = self
                .values
                .iter()
                .map(|(id, value)| message(f, *id, value))
                .collect::<Vec<_>>();
            let current_statuses = f.create_vector(&statuses);
            rpc::StatusSystemResponse::create(
                f,
                &rpc::StatusSystemResponseArgs {
                    current_statuses: Some(current_statuses),
                },
            )
            .as_union_value()
        })
    }
}
