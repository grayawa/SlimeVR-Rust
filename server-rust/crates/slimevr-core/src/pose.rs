//! Event-driven tracker calibration/filtering and explicit-clock pose snapshots.
use crate::{
    calibration::{ArmsResetMode, Calibration, ResetKind},
    filtering::{FilterConfig, QuaternionFilter},
    skeleton::{BodyPosition, HeadPose, Skeleton, SkeletonConfig, SkeletonPose},
    EventKind, InputEvent, Quaternion as Q, SensorStatus, Timed, TrackerSample, Vector3 as V,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

fn default_mounting() -> Q {
    Q::rotation_y(std::f32::consts::PI)
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TrackerBinding {
    pub device_key: String,
    pub sensor_id: u8,
    pub body: BodyPosition,
    #[serde(default = "default_mounting")]
    pub mounting: Q,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PoseConfig {
    pub bindings: Vec<TrackerBinding>,
    pub filter: FilterConfig,
    pub skeleton: SkeletonConfig,
    pub yaw_reset_smooth_seconds: f32,
    pub full_reset_delay_ms: u64,
    pub mounting_reset_delay_ms: u64,
    pub arms_reset_mode: ArmsResetMode,
    pub legs: crate::legs::LegConfig,
    pub alignment: crate::alignment::AlignmentConfig,
    pub localizer: crate::localizer::LocalizerConfig,
    pub imu_types: BTreeMap<BodyPosition, u8>,
    pub magnetometers: BTreeSet<BodyPosition>,
    pub taps: crate::gestures::TapConfig,
    pub flex_resistance: BTreeSet<BodyPosition>,
    pub flex_angles: BTreeSet<BodyPosition>,
    pub save_mounting_reset: bool,
    pub saved_mounting_resets: BTreeMap<BodyPosition, Q>,
    pub hmd_height: Option<f32>,
    pub send_derived_velocity: bool,
    pub reset_hmd_pitch: bool,
    pub reset_mounting_feet: bool,
}
impl Default for PoseConfig {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
            filter: FilterConfig::default(),
            skeleton: SkeletonConfig::default(),
            yaw_reset_smooth_seconds: 0.0,
            full_reset_delay_ms: 3000,
            mounting_reset_delay_ms: 3000,
            arms_reset_mode: ArmsResetMode::Back,
            legs: Default::default(),
            alignment: Default::default(),
            localizer: Default::default(),
            imu_types: BTreeMap::new(),
            magnetometers: BTreeSet::new(),
            taps: Default::default(),
            flex_resistance: BTreeSet::new(),
            flex_angles: BTreeSet::new(),
            save_mounting_reset: false,
            saved_mounting_resets: BTreeMap::new(),
            hmd_height: None,
            send_derived_velocity: false,
            reset_hmd_pitch: false,
            reset_mounting_feet: false,
        }
    }
}
impl PoseConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self
            .hmd_height
            .is_some_and(|h| !h.is_finite() || !(1.2..=1.936).contains(&h))
        {
            return Err("HMD height must be in 1.2..1.936 meters".into());
        }
        if self
            .saved_mounting_resets
            .values()
            .any(|q| !q.is_rotation())
        {
            return Err("invalid saved mounting reset".into());
        }
        self.filter.validate()?;
        self.skeleton.validate()?;
        self.legs.validate()?;
        self.alignment.validate()?;
        self.taps.validate()?;
        if self
            .flex_angles
            .intersection(&self.flex_resistance)
            .next()
            .is_some()
        {
            return Err("flex sensor cannot be both resistance and angle".into());
        }
        if self.localizer.enabled && !self.legs.enabled {
            return Err("localizer requires leg processing".into());
        }
        if self.bindings.len() > 128 {
            return Err("at most 128 tracker bindings".into());
        }
        if !self.yaw_reset_smooth_seconds.is_finite()
            || !(0.0..=60.0).contains(&self.yaw_reset_smooth_seconds)
        {
            return Err("yaw reset smooth time must be 0..60 seconds".into());
        }
        if self.full_reset_delay_ms > 60000 || self.mounting_reset_delay_ms > 60000 {
            return Err("reset delays must be <=60000 ms".into());
        }
        let mut identities = BTreeSet::new();
        let mut bodies = BTreeSet::new();
        for b in &self.bindings {
            if b.device_key.is_empty() || b.device_key.len() > 128 || !b.mounting.is_rotation() {
                return Err("invalid tracker identity or mounting quaternion".into());
            }
            if !identities.insert((&b.device_key, b.sensor_id)) || !bodies.insert(b.body) {
                return Err("duplicate tracker identity or body assignment".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct TrackerPose {
    pub device_key: String,
    pub sensor_id: u8,
    pub body: BodyPosition,
    pub session: u64,
    pub position: Option<V>,
    pub capabilities: crate::TrackerCapabilities,
    pub status: SensorStatus,
    pub raw: Option<Timed<Q>>,
    pub calibrated: Option<Q>,
    pub filtered: Option<Q>,
    pub rotation: Option<Q>,
    pub acceleration_world: Option<Timed<V>>,
    pub pose_age_ms: Option<u64>,
    pub pose_stale: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose_processing_age_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pose_queue_delay_ms: Option<u64>,
    pub calibration: Calibration,
    pub filter_impact_radians: f32,
}
impl TrackerPose {
    pub fn usable(&self) -> bool {
        matches!(self.status, SensorStatus::Ok | SensorStatus::Busy)
            || self.status == SensorStatus::TimedOut && self.capabilities.is_imu
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct PoseSnapshot {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub at_ms: u64,
    pub tick_dt_seconds: f32,
    pub frame: u64,
    pub paused: bool,
    pub trackers: Vec<TrackerPose>,
    pub skeleton: SkeletonPose,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub computed_velocities: BTreeMap<String, V>,
    pub ignored_samples: u64,
    pub reset_count: u64,
    pub last_full_reset_ms: Option<u64>,
    pub mounting_completed: bool,
    pub feet_mounting_completed: bool,
    pub positioned_controllers: BTreeSet<BodyPosition>,
    pub leg_frame: Option<crate::legs::LegFrame>,
    pub alignment: BTreeMap<BodyPosition, crate::alignment::AlignmentState>,
    pub localizer_root: V,
    pub height_status: crate::gestures::HeightStatus,
    pub measured_height: f32,
    pub flex_rotations: BTreeMap<BodyPosition, Q>,
}

struct TrackerState {
    binding: TrackerBinding,
    session: u64,
    status: SensorStatus,
    raw: Option<Timed<Q>>,
    socket_received_at_ms: Option<u64>,
    acceleration: Option<Timed<V>>,
    calibration: Calibration,
    filter: QuaternionFilter,
    capabilities: crate::TrackerCapabilities,
}
impl TrackerState {
    fn new(binding: TrackerBinding, config: FilterConfig) -> Self {
        let calibration = Calibration::new(binding.mounting);
        Self {
            binding,
            session: 0,
            status: SensorStatus::Disconnected,
            raw: None,
            socket_received_at_ms: None,
            acceleration: None,
            calibration,
            filter: QuaternionFilter::new(config, Q::IDENTITY),
            capabilities: Default::default(),
        }
    }
    fn set_capabilities(&mut self, caps: crate::TrackerCapabilities) {
        if !caps.allow_mounting {
            self.calibration.mounting = Q::IDENTITY;
            if self.capabilities.allow_mounting {
                self.calibration.mount_rot_fix = Q::IDENTITY;
            }
        }
        self.capabilities = caps;
    }
    fn usable(&self) -> bool {
        matches!(self.status, SensorStatus::Ok | SensorStatus::Busy)
            || self.status == SensorStatus::TimedOut && self.capabilities.is_imu
    }
    fn adjusted(&self, q: Q) -> Q {
        if self.capabilities.allow_reset || self.capabilities.allow_mounting {
            self.calibration.adjust(q)
        } else {
            q
        }
    }
    fn filter_config(&self, config: FilterConfig) -> FilterConfig {
        if self.capabilities.allow_filter {
            config
        } else {
            FilterConfig {
                mode: crate::filtering::FilterType::None,
                ..config
            }
        }
    }
    fn rotation(&self) -> Option<Q> {
        self.raw.as_ref().map(|_| {
            if self.capabilities.allow_reset {
                self.calibration.yaw_transition * self.filter.output()
            } else {
                self.filter.output()
            }
        })
    }
}

pub struct PoseEngine {
    config: PoseConfig,
    config_revision: u64,
    trackers: BTreeMap<(String, u8), TrackerState>,
    capabilities: BTreeMap<(String, u8), crate::TrackerCapabilities>,
    head: Option<HeadPose>,
    hmd_calibration: crate::calibration::HmdCalibration,
    skeleton: Skeleton,
    velocities: BTreeMap<String, crate::velocity::DerivedVelocity>,
    positions: BTreeMap<BodyPosition, V>,
    controllers: BTreeMap<BodyPosition, Q>,
    legs: crate::legs::LegTweaks,
    leg_overrides: [Option<bool>; 4],
    localizer: crate::localizer::Localizer,
    alignment: crate::alignment::StayAligned,
    taps: [crate::gestures::TapDetector; 3],
    flex_sensors: BTreeMap<BodyPosition, crate::flex::FlexSensor>,
    flex_rotations: BTreeMap<BodyPosition, Q>,
    height_calibration: crate::gestures::HeightCalibration,
    clock_ms: u64,
    last_tick_ms: Option<u64>,
    frame: u64,
    paused: bool,
    pending: VecDeque<(u64, ResetKind)>,
    ignored_samples: u64,
    reset_count: u64,
    last_full_reset_ms: Option<u64>,
    mounting_completed: bool,
    feet_mounting_completed: bool,
    last_snapshot: PoseSnapshot,
}
impl PoseEngine {
    pub fn new(config: PoseConfig) -> Result<Self, String> {
        config.validate()?;
        let trackers = config
            .bindings
            .iter()
            .cloned()
            .map(|b| {
                ((b.device_key.clone(), b.sensor_id), {
                    let mut state = TrackerState::new(b, config.filter);
                    if let Some(q) = config.saved_mounting_resets.get(&state.binding.body) {
                        state.calibration.mount_rot_fix = *q;
                    }
                    state
                })
            })
            .collect();
        let skeleton = Skeleton::new(config.skeleton);
        let mut legs = crate::legs::LegTweaks::new(config.legs);
        legs.set_localizer(config.localizer.enabled);
        let localizer = crate::localizer::Localizer::new(config.localizer.enabled);
        let alignment = crate::alignment::StayAligned::new(config.alignment);
        Ok(Self {
            config,
            config_revision: 0,
            positions: BTreeMap::new(),
            controllers: BTreeMap::new(),
            legs,
            leg_overrides: [None; 4],
            localizer,
            alignment,
            taps: Default::default(),
            flex_sensors: BTreeMap::new(),
            flex_rotations: BTreeMap::new(),
            height_calibration: Default::default(),
            trackers,
            capabilities: BTreeMap::new(),
            head: None,
            hmd_calibration: Default::default(),
            skeleton,
            velocities: BTreeMap::new(),
            clock_ms: 0,
            last_tick_ms: None,
            frame: 0,
            paused: false,
            pending: VecDeque::new(),
            ignored_samples: 0,
            reset_count: 0,
            last_full_reset_ms: None,
            mounting_completed: false,
            feet_mounting_completed: false,
            last_snapshot: PoseSnapshot {
                kind: "pose_snapshot",
                at_ms: 0,
                tick_dt_seconds: 0.0,
                frame: 0,
                paused: false,
                trackers: Vec::new(),
                skeleton: SkeletonPose::default(),
                computed_velocities: BTreeMap::new(),
                ignored_samples: 0,
                reset_count: 0,
                last_full_reset_ms: None,
                mounting_completed: false,
                feet_mounting_completed: false,
                positioned_controllers: BTreeSet::new(),
                leg_frame: None,
                alignment: BTreeMap::new(),
                localizer_root: V::ZERO,
                height_status: crate::gestures::HeightStatus::Idle,
                measured_height: 0.0,
                flex_rotations: BTreeMap::new(),
            },
        })
    }
    fn advance(&mut self, at: u64) -> Result<(), String> {
        if at < self.clock_ms {
            return Err("pose clock went backwards".into());
        }
        self.clock_ms = at;
        Ok(())
    }
    pub fn ingest(&mut self, event: &InputEvent) -> Result<(), String> {
        self.advance(event.at_ms)?;
        match &event.kind {
            EventKind::ExternalTracker {
                device_key,
                sensor_id,
                capabilities,
                ..
            } => {
                let key = (device_key.clone(), *sensor_id);
                self.capabilities.insert(key.clone(), *capabilities);
                if let Some(state) = self.trackers.get_mut(&key) {
                    if state.capabilities != *capabilities {
                        self.config_revision = self.config_revision.wrapping_add(1);
                    }
                    state.set_capabilities(*capabilities);
                    let q = state
                        .raw
                        .as_ref()
                        .map(|r| state.adjusted(r.value))
                        .unwrap_or(Q::IDENTITY);
                    state.filter =
                        QuaternionFilter::new(state.filter_config(self.config.filter), q);
                }
                Ok(())
            }
            EventKind::Sample { sample } => self.sample(sample),
            EventKind::DeviceConnected {
                device_key,
                session,
                preserve_calibration,
                ..
            } => {
                let mut changed = false;
                for ((key, _), s) in &mut self.trackers {
                    if key == device_key && s.session != *session {
                        changed = true;
                        let calibration = (*preserve_calibration && s.session != 0)
                            .then(|| s.calibration.clone());
                        let binding = s.binding.clone();
                        *s = TrackerState::new(binding, self.config.filter);
                        s.session = *session;
                        s.set_capabilities(
                            self.capabilities
                                .get(&(key.clone(), s.binding.sensor_id))
                                .copied()
                                .unwrap_or_default(),
                        );
                        s.filter =
                            QuaternionFilter::new(s.filter_config(self.config.filter), Q::IDENTITY);
                        if let Some(calibration) = calibration {
                            s.calibration = calibration;
                        } else if let Some(q) = self
                            .config
                            .saved_mounting_resets
                            .get(&s.binding.body)
                            .filter(|_| s.capabilities.allow_mounting)
                        {
                            s.calibration.mount_rot_fix = *q;
                        }
                        self.alignment.reset_body(s.binding.body);
                        self.positions.remove(&s.binding.body);
                        self.flex_sensors.remove(&s.binding.body);
                        self.flex_rotations.remove(&s.binding.body);
                        self.legs.reset_at(event.at_ms, false);
                        self.velocities.clear();
                    }
                }
                if changed {
                    self.config_revision = self.config_revision.wrapping_add(1);
                }
                Ok(())
            }
            EventKind::SensorRegistered {
                device_key,
                sensor_id,
                status,
            }
            | EventKind::SensorState {
                device_key,
                sensor_id,
                status,
            } => {
                if let Some(s) = self.trackers.get_mut(&(device_key.clone(), *sensor_id)) {
                    if s.status != *status
                        && matches!(status, SensorStatus::Disconnected | SensorStatus::Error)
                    {
                        self.velocities.clear();
                    }
                    s.status = *status;
                    if !s.capabilities.is_imu && !s.usable() {
                        self.positions.remove(&s.binding.body);
                        self.velocities.clear();
                    }
                }
                Ok(())
            }
            EventKind::SensorMetadata {
                device_key,
                sensor_id,
                imu_type,
                data_type,
                magnetometer_enabled,
            } => {
                if let Some(s) = self.trackers.get(&(device_key.clone(), *sensor_id)) {
                    let body = s.binding.body;
                    let changed = self.config.imu_types.get(&body) != Some(imu_type)
                        || self.config.magnetometers.contains(&body) != *magnetometer_enabled
                        || self.config.flex_resistance.contains(&body) != (*data_type == 1)
                        || self.config.flex_angles.contains(&body) != (*data_type == 2);
                    self.config.imu_types.insert(body, *imu_type);
                    if *magnetometer_enabled {
                        self.config.magnetometers.insert(body);
                    } else {
                        self.config.magnetometers.remove(&body);
                    }
                    match data_type {
                        1 => {
                            self.config.flex_resistance.insert(body);
                            self.config.flex_angles.remove(&body);
                        }
                        2 => {
                            self.config.flex_angles.insert(body);
                            self.config.flex_resistance.remove(&body);
                        }
                        _ => {
                            self.config.flex_angles.remove(&body);
                            self.config.flex_resistance.remove(&body);
                            self.flex_sensors.remove(&body);
                            self.flex_rotations.remove(&body);
                        }
                    }
                    if changed {
                        self.config_revision = self.config_revision.wrapping_add(1);
                    }
                }
                Ok(())
            }
            EventKind::FlexValue {
                device_key,
                sensor_id,
                session,
                value,
            } => {
                if let Some(s) = self.trackers.get(&(device_key.clone(), *sensor_id)) {
                    if s.session == 0 || s.session == *session {
                        let body = s.binding.body;
                        self.set_flex(event.at_ms, body, *value)?;
                    }
                }
                Ok(())
            }
            EventKind::UserAction { action, .. } => self.action(event.at_ms, *action),
            _ => Ok(()),
        }
    }
    pub fn sample(&mut self, sample: &TrackerSample) -> Result<(), String> {
        self.advance(sample.received_at_ms)?;
        self.sample_value(sample)
    }
    /// Restore a receiver observation after assignment without changing its freshness clock.
    pub fn restore_sample(&mut self, at: u64, sample: &TrackerSample) -> Result<(), String> {
        if sample.received_at_ms > at {
            return Err("observation is in the future".into());
        }
        self.advance(at)?;
        self.sample_value(sample)
    }
    fn sample_value(&mut self, sample: &TrackerSample) -> Result<(), String> {
        let Some(s) = self
            .trackers
            .get_mut(&(sample.device_key.clone(), sample.sensor_id))
        else {
            self.ignored_samples += 1;
            return Ok(());
        };
        if s.session != 0 && s.session != sample.session {
            self.ignored_samples += 1;
            return Ok(());
        }
        if sample.server_rotation.is_some_and(|q| !q.is_rotation())
            || sample.server_acceleration.is_some_and(|v| !v.is_finite())
            || sample.position.is_some_and(|v| !v.is_finite())
        {
            return Err("invalid pose sample".into());
        }
        s.session = sample.session;
        if let Some(position) = sample.position {
            self.positions.insert(s.binding.body, position);
        } else if !s.capabilities.is_imu {
            self.positions.remove(&s.binding.body);
        }
        if let Some(q) = sample.server_rotation {
            let calibrated = s.adjusted(q);
            if !calibrated.is_rotation() {
                return Err("invalid calibrated rotation".into());
            }
            let first_rotation = s.raw.is_none();
            s.raw = Some(Timed {
                value: q,
                received_at_ms: sample.received_at_ms,
            });
            s.socket_received_at_ms = sample.socket_received_at_ms;
            s.status = match s.status {
                SensorStatus::Disconnected | SensorStatus::TimedOut => SensorStatus::Ok,
                status => status,
            };
            if first_rotation {
                // A new session must not extrapolate from the identity placeholder.
                s.filter.reset(calibrated, calibrated);
            } else {
                s.filter.add(calibrated);
            }
        }
        if let Some(a) = sample.server_acceleration {
            s.acceleration = Some(Timed {
                value: a,
                received_at_ms: sample.received_at_ms,
            });
        }
        Ok(())
    }
    pub fn set_head(&mut self, at: u64, head: HeadPose) -> Result<(), String> {
        if !head.rotation.is_rotation() || head.position.is_some_and(|v| !v.is_finite()) {
            return Err("invalid head pose".into());
        }
        self.advance(at)?;
        if self.head.is_some_and(|h| h.position.is_some()) != head.position.is_some() {
            self.velocities.clear();
        }
        self.head = Some(head);
        Ok(())
    }
    pub fn set_controller(
        &mut self,
        at: u64,
        body: BodyPosition,
        rotation: Q,
        position: V,
    ) -> Result<(), String> {
        if !matches!(body, BodyPosition::LeftHand | BodyPosition::RightHand)
            || !rotation.is_rotation()
            || !position.is_finite()
        {
            return Err("invalid controller pose".into());
        }
        self.advance(at)?;
        self.positions.insert(body, position);
        self.controllers.insert(body, rotation);
        Ok(())
    }
    pub fn set_flex(&mut self, at: u64, body: BodyPosition, value: f32) -> Result<(), String> {
        if !value.is_finite() {
            return Err("invalid flex sample".into());
        }
        self.advance(at)?;
        let q = if self.config.flex_resistance.contains(&body) {
            self.flex_sensors
                .entry(body)
                .or_default()
                .resistance(body, value)?
        } else if self.config.flex_angles.contains(&body) {
            crate::flex::FlexSensor::angle(body, value)
        } else {
            return Ok(());
        };
        self.flex_rotations.insert(body, q);
        Ok(())
    }
    pub fn start_height_calibration(&mut self, at: u64) -> Result<(), String> {
        self.advance(at)?;
        if self.calibrated_head().is_none_or(|h| h.position.is_none())
            || (self.controllers.is_empty()
                && !self.trackers.values().any(|s| {
                    matches!(
                        s.binding.body,
                        BodyPosition::LeftHand | BodyPosition::RightHand
                    ) && s.usable()
                        && self.positions.contains_key(&s.binding.body)
                }))
        {
            return Err("height calibration requires HMD and controller positions".into());
        }
        self.height_calibration.start(at);
        Ok(())
    }
    pub fn height_calibration(&self) -> &crate::gestures::HeightCalibration {
        &self.height_calibration
    }
    pub fn clear_head(&mut self, at: u64) -> Result<(), String> {
        self.advance(at)?;
        self.head = None;
        self.hmd_calibration.clear();
        self.velocities.clear();
        Ok(())
    }
    pub fn clear_controller(&mut self, at: u64, body: BodyPosition) -> Result<(), String> {
        if !matches!(body, BodyPosition::LeftHand | BodyPosition::RightHand) {
            return Err("controller must be left_hand or right_hand".into());
        }
        self.advance(at)?;
        self.controllers.remove(&body);
        self.positions.remove(&body);
        self.velocities.clear();
        Ok(())
    }
    /// Like UserHeightCalibration.applyCalibration: reset offset defaults using eye height.
    pub fn apply_height(&mut self, at: u64, height: f32) -> Result<(), String> {
        let skeleton = self.config.skeleton.reset_offsets_for_height(height)?;
        self.advance(at)?;
        self.config.skeleton = skeleton;
        self.config.hmd_height = Some(height);
        self.config_revision = self.config_revision.wrapping_add(1);
        self.skeleton = Skeleton::new(skeleton);
        self.legs.reset_at(at, true);
        self.localizer.reset();
        self.velocities.clear();
        Ok(())
    }
    fn reference(&self) -> Q {
        self.calibrated_head()
            .map(|h| h.rotation)
            .or_else(|| {
                self.trackers
                    .values()
                    .find(|s| s.binding.body == BodyPosition::Head)
                    .and_then(TrackerState::rotation)
            })
            .unwrap_or(Q::IDENTITY)
    }
    pub fn reset(&mut self, at: u64, kind: ResetKind) -> Result<(), String> {
        let bodies = if matches!(kind, ResetKind::Mounting) {
            self.default_mounting_bodies()
        } else {
            BTreeSet::new()
        };
        self.reset_selected(at, kind, &bodies)
    }
    pub fn default_mounting_bodies(&self) -> BTreeSet<BodyPosition> {
        use BodyPosition::*;
        let mut bodies = BTreeSet::from([
            Head,
            Neck,
            UpperChest,
            Chest,
            Waist,
            Hip,
            LeftUpperLeg,
            RightUpperLeg,
            LeftLowerLeg,
            RightLowerLeg,
            LeftLowerArm,
            RightLowerArm,
            LeftUpperArm,
            RightUpperArm,
            LeftHand,
            RightHand,
            LeftShoulder,
            RightShoulder,
        ]);
        if self.config.reset_mounting_feet {
            bodies.extend([LeftFoot, RightFoot]);
        }
        bodies
    }
    pub fn reset_selected(
        &mut self,
        at: u64,
        kind: ResetKind,
        bodies: &BTreeSet<BodyPosition>,
    ) -> Result<(), String> {
        self.advance(at)?;
        if matches!(kind, ResetKind::Full)
            && (bodies.is_empty() || bodies.contains(&BodyPosition::Head))
        {
            if let Some(head) = self.head {
                self.hmd_calibration
                    .reset_full(head.rotation, self.config.reset_hmd_pitch)?;
            }
        }
        let reference = self.reference();
        if !reference.yaw_projection().is_rotation() {
            return Err("reset reference has degenerate yaw projection".into());
        }
        for s in self.trackers.values_mut() {
            if !bodies.is_empty() && !bodies.contains(&s.binding.body) {
                continue;
            }
            if (matches!(kind, ResetKind::Mounting) && !s.capabilities.allow_mounting)
                || (!matches!(kind, ResetKind::Mounting) && !s.capabilities.allow_reset)
            {
                continue;
            }
            if let Some(raw) = &s.raw {
                match kind {
                    ResetKind::Full if !s.capabilities.is_imu => s.calibration.reset_full_computed(
                        raw.value,
                        reference,
                        s.binding.body,
                        self.config.arms_reset_mode,
                    ),
                    ResetKind::Full => s.calibration.reset_full_body(
                        raw.value,
                        reference,
                        s.binding.body,
                        self.config.arms_reset_mode,
                    ),
                    ResetKind::Yaw => s.calibration.reset_yaw(
                        raw.value,
                        reference,
                        self.config.yaw_reset_smooth_seconds,
                    ),
                    ResetKind::Mounting => s.calibration.reset_mounting_body(
                        raw.value,
                        reference,
                        s.binding.body,
                        self.config.arms_reset_mode,
                    ),
                }
                s.filter.reset(s.adjusted(raw.value), reference);
                if self.config.save_mounting_reset && matches!(kind, ResetKind::Mounting) {
                    self.config
                        .saved_mounting_resets
                        .insert(s.binding.body, s.calibration.mount_rot_fix);
                }
            }
        }
        for body in self.config.flex_resistance.iter().copied() {
            if !bodies.is_empty() && !bodies.contains(&body) {
                continue;
            }
            if let Some(sensor) = self.flex_sensors.get_mut(&body) {
                let q = match kind {
                    ResetKind::Full => Some(sensor.reset_min(body)),
                    ResetKind::Mounting => Some(sensor.reset_max(body)),
                    ResetKind::Yaw => None,
                };
                if let Some(q) = q {
                    self.flex_rotations.insert(body, q);
                }
            }
        }
        self.alignment.reset();
        for tap in &mut self.taps {
            tap.reset();
        }
        self.legs.reset_at(at, matches!(kind, ResetKind::Full));
        if !matches!(kind, ResetKind::Yaw) {
            self.localizer.reset();
        }
        if matches!(kind, ResetKind::Full) {
            self.last_full_reset_ms = Some(at);
        }
        if matches!(kind, ResetKind::Mounting) {
            let selected = if bodies.is_empty() {
                self.default_mounting_bodies()
            } else {
                bodies.clone()
            };
            self.mounting_completed |= selected.iter().any(|b| {
                self.config.reset_mounting_feet
                    || !matches!(b, BodyPosition::LeftFoot | BodyPosition::RightFoot)
            });
            self.feet_mounting_completed |= selected
                .iter()
                .any(|b| matches!(b, BodyPosition::LeftFoot | BodyPosition::RightFoot));
        }
        self.reset_count += 1;
        self.config_revision = self.config_revision.wrapping_add(1);
        self.velocities.clear();
        Ok(())
    }
    pub fn action(&mut self, at: u64, action: u8) -> Result<(), String> {
        self.advance(at)?;
        let action = match action {
            2 => Some((self.config.full_reset_delay_ms, ResetKind::Full)),
            3 => Some((0, ResetKind::Yaw)),
            4 => Some((self.config.mounting_reset_delay_ms, ResetKind::Mounting)),
            5 => {
                self.set_paused(at, !self.paused)?;
                None
            }
            _ => None,
        };
        if let Some((delay, kind)) = action {
            if self.pending.len() >= 64 {
                return Err("too many pending reset actions".into());
            }
            self.pending.push_back((at.saturating_add(delay), kind));
        }
        Ok(())
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn set_paused(&mut self, at: u64, paused: bool) -> Result<(), String> {
        self.advance(at)?;
        if self.paused != paused {
            self.legs.reset_at(at, false);
            self.velocities.clear();
        }
        self.paused = paused;
        Ok(())
    }
    pub fn tick(&mut self, at: u64) -> Result<&PoseSnapshot, String> {
        self.advance(at)?;
        // Preserve scheduling order for all due commands, without blocking a later immediate yaw.
        let mut due = Vec::new();
        self.pending.retain(|(time, kind)| {
            if *time <= at {
                due.push(*kind);
                false
            } else {
                true
            }
        });
        for kind in due {
            self.reset(at, kind)?;
        }
        let dt = self
            .last_tick_ms
            .map_or(0.0, |last| (at - last) as f32 / 1000.0);
        let mut inputs = BTreeMap::new();
        let mut accelerations = BTreeMap::new();
        let mut poses = Vec::new();
        for s in self.trackers.values_mut() {
            s.filter.update(dt);
            s.calibration.tick(dt);
            let rotation = s.rotation();
            if rotation.is_some_and(|q| !q.is_rotation()) {
                return Err("filter produced invalid rotation".into());
            }
            if s.usable() {
                if let Some(q) = rotation {
                    self.alignment.update_rest(
                        at,
                        s.binding.body,
                        s.raw.as_ref().unwrap().value,
                        s.adjusted(s.raw.as_ref().unwrap().value),
                    );
                    inputs.insert(s.binding.body, q);
                }
            }
            let age = s
                .raw
                .as_ref()
                .map(|q| at.saturating_sub(s.socket_received_at_ms.unwrap_or(q.received_at_ms)));
            let acceleration_world = if let (Some(raw), Some(a)) = (&s.raw, &s.acceleration) {
                Some(Timed {
                    value: raw.value.rotate(a.value),
                    received_at_ms: a.received_at_ms,
                })
            } else {
                None
            };
            if s.usable() {
                if let Some(a) = &acceleration_world {
                    accelerations.insert(s.binding.body, a.value);
                }
            }
            poses.push(TrackerPose {
                device_key: s.binding.device_key.clone(),
                sensor_id: s.binding.sensor_id,
                body: s.binding.body,
                session: s.session,
                position: self.positions.get(&s.binding.body).copied(),
                capabilities: s.capabilities,
                status: s.status,
                raw: s.raw.clone(),
                calibrated: s.raw.as_ref().map(|q| s.adjusted(q.value)),
                filtered: s.raw.as_ref().map(|_| s.filter.output()),
                rotation,
                acceleration_world,
                pose_age_ms: age,
                pose_stale: age.map(|x| x > 2000),
                pose_processing_age_ms: s
                    .socket_received_at_ms
                    .and_then(|_| s.raw.as_ref().map(|q| at.saturating_sub(q.received_at_ms))),
                pose_queue_delay_ms: s.socket_received_at_ms.and_then(|received| {
                    s.raw
                        .as_ref()
                        .map(|q| q.received_at_ms.saturating_sub(received))
                }),
                calibration: s.calibration.clone(),
                filter_impact_radians: s.filter.impact,
            });
        }
        if self.config.taps.enabled && !self.config.taps.setup_mode {
            use BodyPosition as B;
            let yaw = [B::Chest, B::UpperChest, B::Hip, B::Waist]
                .into_iter()
                .find(|b| inputs.contains_key(b));
            let full = [B::LeftUpperLeg, B::LeftLowerLeg]
                .into_iter()
                .find(|b| inputs.contains_key(b));
            let mount = [B::RightUpperLeg, B::RightLowerLeg]
                .into_iter()
                .find(|b| inputs.contains_key(b));
            let cfg = self.config.taps;
            let yaw = if cfg.yaw_enabled {
                cfg.yaw_tracker.or(yaw)
            } else {
                None
            };
            let full = if cfg.full_enabled {
                cfg.full_tracker.or(full)
            } else {
                None
            };
            let mount = if cfg.mounting_enabled {
                cfg.mounting_tracker.or(mount)
            } else {
                None
            };
            for (i, body, count, delay, kind) in [
                (0, yaw, cfg.yaw_taps, cfg.yaw_delay_ms, ResetKind::Yaw),
                (1, full, cfg.full_taps, cfg.full_delay_ms, ResetKind::Full),
                (
                    2,
                    mount,
                    cfg.mounting_taps,
                    cfg.mounting_delay_ms,
                    ResetKind::Mounting,
                ),
            ] {
                if let Some(body) = body {
                    if self.taps[i].update(at, body, &accelerations, count, cfg.max_moving)
                        && self.pending.len() < 64
                    {
                        self.pending.push_back((at.saturating_add(delay), kind));
                    }
                }
            }
        }
        let mut hands: BTreeMap<_, _> = self
            .controllers
            .iter()
            .filter_map(|(b, q)| self.positions.get(b).map(|p| (*b, (*q, *p))))
            .collect();
        for s in self.trackers.values().filter(|s| {
            matches!(
                s.binding.body,
                BodyPosition::LeftHand | BodyPosition::RightHand
            ) && s.usable()
        }) {
            if let (Some(q), Some(p)) = (s.rotation(), self.positions.get(&s.binding.body)) {
                hands.entry(s.binding.body).or_insert((q, *p));
            }
        }
        if let Some(height) = self
            .height_calibration
            .update(at, self.calibrated_head(), &hands)
        {
            self.apply_height(at, height)?;
        }
        inputs.extend(self.controllers.iter().map(|(b, q)| (*b, *q)));
        let rates = inputs
            .keys()
            .filter(|b| {
                !self.controllers.contains_key(b)
                    && self
                        .trackers
                        .values()
                        .any(|s| s.binding.body == **b && s.capabilities.is_imu)
            })
            .map(|b| {
                (
                    *b,
                    crate::alignment::imu_rate(
                        *self.config.imu_types.get(b).unwrap_or(&0),
                        self.config.magnetometers.contains(b),
                    ),
                )
            })
            .collect();
        let mut alignment_inputs: BTreeMap<_, _> = poses
            .iter()
            .filter(|p| {
                matches!(
                    p.status,
                    SensorStatus::Ok | SensorStatus::Busy | SensorStatus::TimedOut
                )
            })
            .filter(|p| {
                self.trackers
                    .get(&(p.device_key.clone(), p.sensor_id))
                    .is_some_and(|s| s.capabilities.is_imu)
            })
            .filter_map(|p| p.calibrated.map(|q| (p.body, q)))
            .collect();
        alignment_inputs.extend(self.controllers.iter().map(|(b, q)| (*b, *q)));
        if let Some(head) = self.calibrated_head() {
            alignment_inputs.insert(BodyPosition::Head, head.rotation);
        }
        self.alignment.adjust(dt, &alignment_inputs, &rates);
        for (body, q) in &mut inputs {
            *q = self.alignment.correction(*body) * *q;
        }
        for p in &mut poses {
            if let Some(q) = p.rotation {
                p.rotation = Some(self.alignment.correction(p.body) * q);
            }
        }
        let head = self.calibrated_head().or_else(|| {
            inputs.get(&BodyPosition::Head).map(|q| HeadPose {
                rotation: *q,
                position: self.positions.get(&BodyPosition::Head).copied(),
            })
        });
        self.skeleton
            .set_root_override(if self.config.localizer.enabled {
                Some(self.localizer.root)
            } else {
                None
            });
        for (body, q) in &self.flex_rotations {
            if self.trackers.values().any(|s| {
                s.binding.body == *body
                    && !matches!(
                        s.status,
                        SensorStatus::Ok | SensorStatus::Busy | SensorStatus::TimedOut
                    )
            }) {
                continue;
            }
            let q = if body.is_left_finger() {
                self.skeleton.previous_hand_rotation(true) * *q
            } else if body.is_right_finger() {
                self.skeleton.previous_hand_rotation(false) * *q
            } else {
                *q
            };
            inputs.insert(*body, q);
        }
        let mut skeleton =
            self.skeleton
                .solve_with_positions(&inputs, &self.positions, head, self.paused)?;
        skeleton.world_anchor_present = head.is_some_and(|h| h.position.is_some());
        if !self.paused {
            self.legs
                .update(at, &mut skeleton, &inputs, &accelerations)?;
            self.localizer.update(&self.legs, &skeleton, &accelerations);
        }
        let mut computed_velocities = BTreeMap::new();
        if self.config.send_derived_velocity && skeleton.world_anchor_present && !self.paused {
            if let Some(at_us) = at.checked_mul(1000) {
                self.velocities
                    .retain(|name, _| skeleton.computed.contains_key(name));
                for (name, pose) in &skeleton.computed {
                    if let Some(velocity) = self
                        .velocities
                        .entry(name.clone())
                        .or_default()
                        .update(at_us, Some(pose.position))
                    {
                        computed_velocities.insert(name.clone(), velocity);
                    }
                }
            } else {
                self.velocities.clear();
            }
        } else {
            self.velocities.clear();
        }
        self.frame += 1;
        self.last_tick_ms = Some(at);
        self.last_snapshot = PoseSnapshot {
            kind: "pose_snapshot",
            at_ms: at,
            tick_dt_seconds: dt,
            frame: self.frame,
            paused: self.paused,
            trackers: poses,
            skeleton,
            computed_velocities,
            ignored_samples: self.ignored_samples,
            reset_count: self.reset_count,
            last_full_reset_ms: self.last_full_reset_ms,
            mounting_completed: self.mounting_completed,
            feet_mounting_completed: self.feet_mounting_completed,
            positioned_controllers: self
                .controllers
                .keys()
                .filter(|b| self.positions.contains_key(b))
                .copied()
                .collect(),
            leg_frame: self.legs.frame().cloned(),
            alignment: self.alignment.states.clone(),
            localizer_root: self.localizer.root,
            height_status: self.height_calibration.status,
            measured_height: self.height_calibration.height,
            flex_rotations: self.flex_rotations.clone(),
        };
        Ok(&self.last_snapshot)
    }
    /// Changes only when exported configuration may change, never on ordinary samples/ticks.
    pub fn config_revision(&self) -> u64 {
        self.config_revision
    }
    /// Export explicit configuration and optionally persist automatically measured mounting.
    pub fn export_config(&self) -> PoseConfig {
        let mut c = self.config.clone();
        if c.save_mounting_reset {
            for s in self.trackers.values() {
                if s.calibration.mounting_reset_done {
                    c.saved_mounting_resets
                        .insert(s.binding.body, s.calibration.mount_rot_fix);
                }
            }
        }
        c
    }
    pub fn motion_frame(&self) -> Result<crate::autobone::MotionFrame, String> {
        let head = self
            .calibrated_head()
            .or_else(|| {
                self.positions.get(&BodyPosition::Head).and_then(|p| {
                    self.last_snapshot
                        .trackers
                        .iter()
                        .find(|t| t.body == BodyPosition::Head && t.usable())
                        .and_then(|t| t.calibrated)
                        .map(|q| HeadPose {
                            rotation: q,
                            position: Some(*p),
                        })
                })
            })
            .filter(|h| h.position.is_some())
            .ok_or("motion recording requires HMD position")?;
        let mut rotations: BTreeMap<_, _> = self
            .last_snapshot
            .trackers
            .iter()
            .filter(|p| p.usable())
            .filter_map(|p| p.calibrated.map(|q| (p.body, q)))
            .collect();
        rotations.extend(self.controllers.iter().map(|(b, q)| (*b, *q)));
        Ok(crate::autobone::MotionFrame {
            at_ms: self.last_snapshot.at_ms,
            rotations,
            head,
            positions: self.positions.clone(),
        })
    }
    pub fn snapshot(&self) -> &PoseSnapshot {
        &self.last_snapshot
    }
    pub fn calibrated_head(&self) -> Option<HeadPose> {
        self.head
            .map(|head| HeadPose {
                rotation: self.hmd_calibration.adjust(head.rotation),
                position: head.position,
            })
            .or_else(|| {
                self.trackers
                    .values()
                    .find(|s| s.binding.body == BodyPosition::Head && s.usable())
                    .and_then(|s| {
                        s.rotation().map(|rotation| HeadPose {
                            rotation,
                            position: self.positions.get(&BodyPosition::Head).copied(),
                        })
                    })
            })
    }
    /// Apply validated live settings, preserving calibration of unchanged bindings.
    pub fn configure(&mut self, at: u64, config: PoseConfig) -> Result<(), String> {
        config.validate()?;
        self.advance(at)?;
        let old = self.export_config();
        let filter_changed = old.filter != config.filter;
        let shape_changed = old.skeleton != config.skeleton;
        let bindings_changed = old.bindings != config.bindings;
        if shape_changed
            || bindings_changed
            || filter_changed
            || old.legs != config.legs
            || old.alignment != config.alignment
            || old.localizer.enabled != config.localizer.enabled
            || old.saved_mounting_resets != config.saved_mounting_resets
            || old.send_derived_velocity != config.send_derived_velocity
        {
            self.velocities.clear();
        }
        self.trackers.retain(|_, s| {
            config.bindings.iter().any(|b| {
                b.device_key == s.binding.device_key
                    && b.sensor_id == s.binding.sensor_id
                    && b.body == s.binding.body
            })
        });
        for binding in &config.bindings {
            let state = self
                .trackers
                .entry((binding.device_key.clone(), binding.sensor_id))
                .or_insert_with(|| TrackerState::new(binding.clone(), config.filter));
            if state.binding.mounting != binding.mounting {
                state.calibration.mounting = binding.mounting;
                state.calibration.mount_rot_fix = Q::IDENTITY;
                state.calibration.mounting_reset_done = false;
            }
            state.binding = binding.clone();
            state.set_capabilities(
                self.capabilities
                    .get(&(binding.device_key.clone(), binding.sensor_id))
                    .copied()
                    .unwrap_or_default(),
            );
            if filter_changed || !state.capabilities.allow_filter {
                let q = state
                    .raw
                    .as_ref()
                    .map(|r| state.adjusted(r.value))
                    .unwrap_or(Q::IDENTITY);
                state.filter = QuaternionFilter::new(state.filter_config(config.filter), q);
            }
            if let Some(q) = config
                .saved_mounting_resets
                .get(&binding.body)
                .filter(|_| state.capabilities.allow_mounting)
            {
                state.calibration.mount_rot_fix = *q;
            }
        }
        if bindings_changed {
            self.positions.retain(|body, _| {
                self.controllers.contains_key(body)
                    || config
                        .bindings
                        .iter()
                        .any(|b| b.body == *body && old.bindings.contains(b))
            });
            self.flex_sensors.clear();
            self.flex_rotations.clear();
            self.alignment.reset();
        }
        if shape_changed {
            self.skeleton = Skeleton::new(config.skeleton);
        }
        if shape_changed || bindings_changed {
            self.mounting_completed = false;
            self.feet_mounting_completed = false;
        }
        if shape_changed || bindings_changed || old.legs != config.legs {
            self.legs = crate::legs::LegTweaks::new(config.legs);
            self.legs.set_localizer(config.localizer.enabled);
            self.legs.reset_at(at, true);
        }
        if old.alignment != config.alignment {
            self.alignment = crate::alignment::StayAligned::new(config.alignment);
        }
        if self.localizer.enabled != config.localizer.enabled {
            self.localizer = crate::localizer::Localizer::new(config.localizer.enabled);
        }
        self.legs.set_localizer(config.localizer.enabled);
        self.config = config;
        self.config_revision = self.config_revision.wrapping_add(1);
        self.apply_leg_overrides();
        Ok(())
    }
    pub fn leg_overrides(&self) -> [Option<bool>; 4] {
        self.leg_overrides
    }
    pub fn set_leg_overrides(&mut self, at: u64, values: [Option<bool>; 4]) -> Result<(), String> {
        self.advance(at)?;
        if self.leg_overrides != values {
            self.leg_overrides = values;
            self.apply_leg_overrides();
            self.legs.reset_at(at, false);
            self.velocities.clear();
        }
        Ok(())
    }
    fn apply_leg_overrides(&mut self) {
        let mut c = self.config.legs;
        if let Some(v) = self.leg_overrides[0] {
            c.floor_clip = v;
        }
        if let Some(v) = self.leg_overrides[1] {
            c.skating = v;
        }
        if let Some(v) = self.leg_overrides[2] {
            c.toe_snap = v;
        }
        if let Some(v) = self.leg_overrides[3] {
            c.foot_plant = v;
        }
        self.legs.config = c;
    }
    pub fn clear_mounting(&mut self, at: u64) -> Result<(), String> {
        self.advance(at)?;
        for state in self.trackers.values_mut() {
            if state.capabilities.allow_mounting {
                state.calibration.mount_rot_fix = Q::IDENTITY;
                state.calibration.mounting_reset_done = false;
            }
        }
        self.config.saved_mounting_resets.clear();
        self.config_revision = self.config_revision.wrapping_add(1);
        self.mounting_completed = false;
        self.feet_mounting_completed = false;
        self.legs.reset_at(at, false);
        self.velocities.clear();
        self.reset_count += 1;
        Ok(())
    }
    pub fn cancel_height_calibration(&mut self, at: u64) -> Result<(), String> {
        self.advance(at)?;
        self.height_calibration = Default::default();
        Ok(())
    }
}

/// Algorithm scenes use server-space input. UDP input remains in the receiver's journal.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneInput {
    ClearMounting {
        at_ms: u64,
    },
    LegOverrides {
        at_ms: u64,
        values: [Option<bool>; 4],
    },
    Configure {
        at_ms: u64,
        config: Box<PoseConfig>,
    },
    RestoreSample {
        at_ms: u64,
        sample: TrackerSample,
    },
    ResetSelected {
        at_ms: u64,
        kind: ResetKind,
        bodies: BTreeSet<BodyPosition>,
    },
    ClearSource {
        at_ms: u64,
        body: BodyPosition,
    },
    CancelHeightCalibration {
        at_ms: u64,
    },
    Input {
        event: InputEvent,
    },
    Sample {
        sample: TrackerSample,
    },
    Head {
        at_ms: u64,
        rotation: Q,
        position: Option<V>,
    },
    Flex {
        at_ms: u64,
        body: BodyPosition,
        value: f32,
    },
    HeightCalibration {
        at_ms: u64,
    },
    Controller {
        at_ms: u64,
        body: BodyPosition,
        rotation: Q,
        position: V,
    },
    Reset {
        at_ms: u64,
        kind: ResetKind,
    },
    Pause {
        at_ms: u64,
        paused: bool,
    },
    Tick {
        at_ms: u64,
    },
}
impl PoseEngine {
    pub fn scene_input(&mut self, input: SceneInput) -> Result<bool, String> {
        match input {
            SceneInput::ClearMounting { at_ms } => {
                self.clear_mounting(at_ms)?;
                Ok(false)
            }
            SceneInput::LegOverrides { at_ms, values } => {
                self.set_leg_overrides(at_ms, values)?;
                Ok(false)
            }
            SceneInput::Configure { at_ms, config } => {
                self.configure(at_ms, *config)?;
                Ok(false)
            }
            SceneInput::RestoreSample { at_ms, sample } => {
                self.restore_sample(at_ms, &sample)?;
                Ok(false)
            }
            SceneInput::ResetSelected {
                at_ms,
                kind,
                bodies,
            } => {
                self.reset_selected(at_ms, kind, &bodies)?;
                Ok(false)
            }
            SceneInput::ClearSource { at_ms, body } => {
                if body == BodyPosition::Head {
                    self.clear_head(at_ms)?;
                } else {
                    self.clear_controller(at_ms, body)?;
                }
                Ok(false)
            }
            SceneInput::CancelHeightCalibration { at_ms } => {
                self.cancel_height_calibration(at_ms)?;
                Ok(false)
            }
            SceneInput::Input { event } => {
                self.ingest(&event)?;
                Ok(false)
            }
            SceneInput::Sample { sample } => {
                self.sample(&sample)?;
                Ok(false)
            }
            SceneInput::Head {
                at_ms,
                rotation,
                position,
            } => {
                self.set_head(at_ms, HeadPose { rotation, position })?;
                Ok(false)
            }
            SceneInput::Flex { at_ms, body, value } => {
                self.set_flex(at_ms, body, value)?;
                Ok(false)
            }
            SceneInput::HeightCalibration { at_ms } => {
                self.start_height_calibration(at_ms)?;
                Ok(false)
            }
            SceneInput::Controller {
                at_ms,
                body,
                rotation,
                position,
            } => {
                self.set_controller(at_ms, body, rotation, position)?;
                Ok(false)
            }
            SceneInput::Reset { at_ms, kind } => {
                self.reset(at_ms, kind)?;
                Ok(false)
            }
            SceneInput::Pause { at_ms, paused } => {
                self.set_paused(at_ms, paused)?;
                Ok(false)
            }
            SceneInput::Tick { at_ms } => {
                self.tick(at_ms)?;
                Ok(true)
            }
        }
    }
}
