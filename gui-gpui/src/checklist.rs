//! Match the original tracking checklist: enabled/ignored filtering precedes
//! ordering, blocking and visibility. Session skips live in GUI session state.
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Complete,
    Skipped,
    Blocked,
    Invalid,
}
#[derive(Clone, Debug)]
pub struct Step {
    pub data: Value,
    pub id: u64,
    pub status: Status,
    pub first_required: bool,
}
#[derive(Clone, Debug)]
pub struct Checklist {
    pub steps: Vec<Step>,
    pub progress: f32,
    pub completion: &'static str,
    pub warnings: usize,
}
pub fn derive(value: &Value, session_ignored: &BTreeSet<u64>) -> Checklist {
    let ignored: BTreeSet<_> = value["ignored_steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .chain(session_ignored.iter().copied())
        .collect();
    let active: Vec<_> = value["steps"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["enabled"] == true && !ignored.contains(&s["id"].as_u64().unwrap_or(0)))
        .collect();
    let first = active.iter().position(|s| s["valid"] != true);
    let mut blocked = false;
    let mut steps = Vec::new();
    for (index, data) in active.iter().enumerate() {
        let valid = data["valid"] == true;
        let optional = data["optional"] == true;
        let status = if valid {
            Status::Complete
        } else if optional {
            // The original marks optional invalid steps as skipped.
            Status::Skipped
        } else if blocked {
            Status::Blocked
        } else {
            Status::Invalid
        };
        let first_required = first == Some(index);
        if first_required
            || data["visibility"].as_u64().unwrap_or(0) == 0
            || status != Status::Complete
        {
            steps.push(Step {
                data: (*data).clone(),
                id: data["id"].as_u64().unwrap_or(0),
                status,
                first_required,
            });
        }
        if !valid && !optional {
            blocked = true;
        }
    }
    let completed = steps
        .iter()
        .filter(|s| matches!(s.status, Status::Complete | Status::Skipped))
        .count();
    let progress = if steps.is_empty() {
        1.
    } else {
        completed as f32 / steps.len() as f32
    };
    let completion = if progress < 1. {
        "incomplete"
    } else if steps.iter().any(|s| s.status == Status::Skipped) {
        "partial"
    } else {
        "complete"
    };
    let warnings = steps.iter().filter(|s| s.data["valid"] != true).count();
    Checklist {
        steps,
        progress,
        completion,
        warnings,
    }
}
pub fn name(id: u64) -> &'static str {
    crate::rpc_generated::enum_choices("TrackingChecklistStepId")
        .iter()
        .find(|(_, n)| *n == id)
        .map(|(n, _)| *n)
        .unwrap_or("UNKNOWN")
}

pub fn highlighted_trackers(
    value: &Value,
    session_ignored: &BTreeSet<u64>,
) -> Vec<crate::protocol::TrackerKey> {
    let checklist = derive(value, session_ignored);
    let Some(step) = checklist.steps.iter().find(|s| {
        s.data["valid"] != true && s.data["optional"] != true && s.status != Status::Blocked
    }) else {
        return Vec::new();
    };
    let extra = &step.data["extra_data"]["value"];
    let ids: Vec<_> = if let Some(ids) = extra["trackers_id"].as_array() {
        ids.iter().collect()
    } else if extra["tracker_id"].is_object() {
        vec![&extra["tracker_id"]]
    } else {
        Vec::new()
    };
    ids.into_iter()
        .filter_map(|id| {
            Some(crate::protocol::TrackerKey {
                device: id["device_id"]["id"]
                    .as_u64()
                    .unwrap_or(0)
                    .try_into()
                    .ok()?,
                sensor: id["tracker_num"].as_u64()?.try_into().ok()?,
            })
        })
        .collect()
}
