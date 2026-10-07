use super::{protocol::rpc_frame, FrontendConfig};
use slimevr_core::{alignment::RelaxedPose, calibration::ArmsResetMode, filtering::FilterType};
use solarxr_protocol::{datatypes as dt, rpc};

pub fn frame(tx: u32, c: &FrontendConfig) -> Vec<u8> {
    rpc_frame(rpc::RpcMessage::SettingsResponse, tx, |f| {
        let p = &c.pose;
        let s = p.skeleton;
        let filtering = rpc::FilteringSettings::create(
            f,
            &rpc::FilteringSettingsArgs {
                type_: match p.filter.mode {
                    FilterType::None => dt::FilteringType::NONE,
                    FilterType::Smoothing => dt::FilteringType::SMOOTHING,
                    FilterType::Prediction => dt::FilteringType::PREDICTION,
                },
                amount: p.filter.amount,
            },
        );
        let toggles = rpc::settings::ModelToggles::create(
            f,
            &rpc::settings::ModelTogglesArgs {
                extended_spine: Some(s.extended_spine),
                extended_pelvis: Some(s.extended_pelvis),
                extended_knee: Some(s.extended_knee),
                force_arms_from_hmd: Some(s.force_arms_from_hmd),
                floor_clip: Some(p.legs.floor_clip),
                skating_correction: Some(p.legs.skating),
                toe_snap: Some(p.legs.toe_snap),
                foot_plant: Some(p.legs.foot_plant),
                self_localization: Some(p.localizer.enabled),
                use_position: Some(p.skeleton.use_position),
                enforce_constraints: Some(s.enforce_constraints),
                correct_constraints: Some(p.skeleton.correct_constraints),
            },
        );
        let ratios = rpc::settings::ModelRatios::create(
            f,
            &rpc::settings::ModelRatiosArgs {
                impute_waist_from_chest_hip: Some(s.waist_from_chest_hip),
                impute_waist_from_chest_legs: Some(s.waist_from_chest_legs),
                impute_hip_from_chest_legs: Some(s.hip_from_chest_legs),
                impute_hip_from_waist_legs: Some(s.hip_from_waist_legs),
                interp_hip_legs: Some(s.hip_legs),
                interp_knee_tracker_ankle: Some(s.knee_tracker_ankle),
                interp_knee_ankle: Some(s.knee_ankle),
            },
        );
        let leg_tweaks = rpc::settings::LegTweaksSettings::create(
            f,
            &rpc::settings::LegTweaksSettingsArgs {
                correction_strength: Some(p.legs.correction_strength),
            },
        );
        let height = rpc::settings::SkeletonHeight::create(
            f,
            &rpc::settings::SkeletonHeightArgs {
                hmd_height: p.hmd_height,
                floor_height: Some(0.0),
            },
        );
        let model = rpc::settings::ModelSettings::create(
            f,
            &rpc::settings::ModelSettingsArgs {
                toggles: Some(toggles),
                ratios: Some(ratios),
                leg_tweaks: Some(leg_tweaks),
                skeleton_height: Some(height),
            },
        );
        let resets = rpc::ResetsSettings::create(
            f,
            &rpc::ResetsSettingsArgs {
                arms_mounting_reset_mode: rpc::ArmsMountingResetMode(match p.arms_reset_mode {
                    ArmsResetMode::Back => 0,
                    ArmsResetMode::Forward => 1,
                    ArmsResetMode::TposeUp => 2,
                    ArmsResetMode::TposeDown => 3,
                }),
                yaw_reset_smooth_time: p.yaw_reset_smooth_seconds,
                save_mounting_reset: p.save_mounting_reset,
                reset_hmd_pitch: p.reset_hmd_pitch,
                reset_mounting_feet: p.reset_mounting_feet,
            },
        );
        let a = p.alignment;
        let aligned = rpc::StayAlignedSettings::create(
            f,
            &rpc::StayAlignedSettingsArgs {
                enabled: a.enabled,
                hideYawCorrection: a.hide_correction,
                standingEnabled: a.standing.enabled,
                standingUpperLegAngle: a.standing.upper_leg_degrees,
                standingLowerLegAngle: a.standing.lower_leg_degrees,
                standingFootAngle: a.standing.foot_degrees,
                sittingEnabled: a.sitting.enabled,
                sittingUpperLegAngle: a.sitting.upper_leg_degrees,
                sittingLowerLegAngle: a.sitting.lower_leg_degrees,
                sittingFootAngle: a.sitting.foot_degrees,
                flatEnabled: a.flat.enabled,
                flatUpperLegAngle: a.flat.upper_leg_degrees,
                flatLowerLegAngle: a.flat.lower_leg_degrees,
                flatFootAngle: a.flat.foot_degrees,
                setupComplete: a.standing.enabled || a.sitting.enabled || a.flat.enabled,
                ..Default::default()
            },
        );
        let t = p.taps;
        let taps = rpc::TapDetectionSettings::create(
            f,
            &rpc::TapDetectionSettingsArgs {
                setup_mode: Some(t.setup_mode),
                yaw_reset_enabled: Some(t.enabled && t.yaw_enabled),
                full_reset_enabled: Some(t.enabled && t.full_enabled),
                mounting_reset_enabled: Some(t.enabled && t.mounting_enabled),
                yaw_reset_delay: Some(t.yaw_delay_ms as f32 / 1000.0),
                full_reset_delay: Some(t.full_delay_ms as f32 / 1000.0),
                mounting_reset_delay: Some(t.mounting_delay_ms as f32 / 1000.0),
                yaw_reset_taps: Some(t.yaw_taps),
                full_reset_taps: Some(t.full_taps),
                mounting_reset_taps: Some(t.mounting_taps),
                number_trackers_over_threshold: Some(t.max_moving as u8),
                yaw_reset_tracker: Some(
                    t.yaw_tracker
                        .map(super::protocol::body)
                        .unwrap_or(dt::BodyPart::CHEST),
                ),
                full_reset_tracker: Some(
                    t.full_tracker
                        .map(super::protocol::body)
                        .unwrap_or(dt::BodyPart::LEFT_UPPER_LEG),
                ),
                mounting_reset_tracker: Some(
                    t.mounting_tracker
                        .map(super::protocol::body)
                        .unwrap_or(dt::BodyPart::RIGHT_UPPER_LEG),
                ),
            },
        );
        let a = c.auto_bone;
        let auto = rpc::AutoBoneSettings::create(
            f,
            &rpc::AutoBoneSettingsArgs {
                num_epochs: Some(a.epochs as i32),
                cursor_increment: Some(a.cursor_increment as i32),
                min_data_distance: Some(a.min_distance as i32),
                max_data_distance: Some(a.max_distance as i32),
                initial_adjust_rate: Some(a.initial_adjust_rate),
                adjust_rate_decay: Some(a.adjust_rate_decay),
                slide_error_factor: Some(a.slide_factor),
                offset_slide_error_factor: Some(a.offset_slide_factor),
                foot_height_offset_error_factor: Some(a.foot_height_factor),
                body_proportion_error_factor: Some(a.proportion_factor),
                height_error_factor: Some(a.height_factor),
                position_error_factor: Some(a.position_factor),
                position_offset_error_factor: Some(a.position_offset_factor),
                calc_init_error: Some(a.calc_initial_error),
                use_skeleton_height: Some(a.use_skeleton_height),
                randomize_frame_order: Some(a.randomize),
                scale_each_step: Some(a.scale_each_step),
                rand_seed: Some(a.seed as i64),
                save_recordings: Some(c.save_recordings),
                sample_count: Some(c.sample_count as i32),
                sample_rate_ms: Some(c.sample_ms as i64),
                ..Default::default()
            },
        );
        let steam = rpc::SteamVRTrackersSetting::create(
            f,
            &rpc::SteamVRTrackersSettingArgs {
                waist: c.steam_vr.enabled("waist"),
                chest: c.steam_vr.enabled("chest"),
                left_foot: c.steam_vr.enabled("left_foot"),
                right_foot: c.steam_vr.enabled("right_foot"),
                left_knee: c.steam_vr.enabled("left_knee"),
                right_knee: c.steam_vr.enabled("right_knee"),
                left_elbow: c.steam_vr.enabled("left_elbow"),
                right_elbow: c.steam_vr.enabled("right_elbow"),
                left_hand: c.steam_vr.enabled("left_hand"),
                right_hand: c.steam_vr.enabled("right_hand"),
                automaticTrackerToggle: c.steam_vr.automatic,
            },
        );
        let drift = rpc::DriftCompensationSettings::create(f, &Default::default());
        fn endpoint<'a>(
            f: &mut solarxr_protocol::flatbuffers::FlatBufferBuilder<'a>,
            e: &crate::osc::config::Endpoint,
        ) -> solarxr_protocol::flatbuffers::WIPOffset<rpc::OSCSettings<'a>> {
            let address = f.create_string(&e.address);
            rpc::OSCSettings::create(
                f,
                &rpc::OSCSettingsArgs {
                    enabled: e.enabled,
                    port_in: e.port_in,
                    port_out: e.port_out,
                    address: Some(address),
                },
            )
        }
        let osc = endpoint(f, &c.osc.router);
        let osc_router = rpc::OSCRouterSettings::create(
            f,
            &rpc::OSCRouterSettingsArgs {
                osc_settings: Some(osc),
            },
        );
        let enabled = |role: &str| c.osc.vrc.trackers.get(role) == Some(&true);
        let trackers = rpc::OSCTrackersSetting::create(
            f,
            &rpc::OSCTrackersSettingArgs {
                head: false,
                hands: false,
                chest: enabled("chest"),
                waist: enabled("waist"),
                knees: enabled("left_knee") || enabled("right_knee"),
                feet: enabled("left_foot") || enabled("right_foot"),
                elbows: enabled("left_elbow") || enabled("right_elbow"),
            },
        );
        let osc = endpoint(f, &c.osc.vrc.endpoint);
        let vrc = rpc::VRCOSCSettings::create(
            f,
            &rpc::VRCOSCSettingsArgs {
                osc_settings: Some(osc),
                trackers: Some(trackers),
                oscquery_enabled: c.osc.vrc.oscquery_enabled,
            },
        );
        let osc = endpoint(f, &c.osc.vmc.endpoint);
        let vmc = rpc::VMCOSCSettings::create(
            f,
            &rpc::VMCOSCSettingsArgs {
                osc_settings: Some(osc),
                anchor_hip: c.osc.vmc.anchor_hip,
                mirror_tracking: c.osc.vmc.mirror_tracking,
            },
        );
        let vrm_json = c.osc.vmc.vrm_json.as_ref().map(|s| f.create_string(s));
        let vrm = rpc::VRMSettings::create(f, &rpc::VRMSettingsArgs { vrm_json });
        let hid = rpc::HIDSettings::create(
            f,
            &rpc::HIDSettingsArgs {
                trackersOverHID: c.yaml["hidConfig"]["trackersOverHID"]
                    .as_bool()
                    .unwrap_or(false),
            },
        );
        let velocity = rpc::VelocitySettings::create(
            f,
            &rpc::VelocitySettingsArgs {
                send_derived_velocity: c.pose.send_derived_velocity,
            },
        );
        rpc::SettingsResponse::create(
            f,
            &rpc::SettingsResponseArgs {
                filtering: Some(filtering),
                model_settings: Some(model),
                resets_settings: Some(resets),
                tap_detection_settings: Some(taps),
                stay_aligned: Some(aligned),
                auto_bone_settings: Some(auto),
                steam_vr_trackers: Some(steam),
                drift_compensation: Some(drift),
                osc_router: Some(osc_router),
                vrc_osc: Some(vrc),
                vmc_osc: Some(vmc),
                vrm: Some(vrm),
                hid_settings: Some(hid),
                velocity_settings: Some(velocity),
                ..Default::default()
            },
        )
        .as_union_value()
    })
}
fn seconds(v: f32) -> Result<u64, String> {
    if !v.is_finite() || !(0.0..=60.0).contains(&v) {
        Err("delay must be 0..60 seconds".into())
    } else {
        Ok((v * 1000.0).round() as u64)
    }
}
fn positive(v: i32) -> Result<usize, String> {
    usize::try_from(v)
        .ok()
        .filter(|v| *v > 0)
        .ok_or("value must be positive".into())
}
pub fn change(
    c: &FrontendConfig,
    r: rpc::ChangeSettingsRequest<'_>,
) -> Result<FrontendConfig, String> {
    let mut c = c.clone();
    let p = &mut c.pose;
    if let Some(v) = r.filtering() {
        p.filter.mode = match v.type_() {
            dt::FilteringType::NONE => FilterType::None,
            dt::FilteringType::SMOOTHING => FilterType::Smoothing,
            dt::FilteringType::PREDICTION => FilterType::Prediction,
            _ => return Err("unknown filter".into()),
        };
        p.filter.amount = v.amount();
    }
    if let Some(m) = r.model_settings() {
        if let Some(t) = m.toggles() {
            macro_rules! set {
                ($source:ident,$target:expr) => {
                    if let Some(v) = t.$source() {
                        $target = v;
                    }
                };
            }
            set!(extended_spine, p.skeleton.extended_spine);
            set!(extended_pelvis, p.skeleton.extended_pelvis);
            set!(extended_knee, p.skeleton.extended_knee);
            set!(force_arms_from_hmd, p.skeleton.force_arms_from_hmd);
            set!(enforce_constraints, p.skeleton.enforce_constraints);
            set!(floor_clip, p.legs.floor_clip);
            set!(skating_correction, p.legs.skating);
            set!(toe_snap, p.legs.toe_snap);
            set!(foot_plant, p.legs.foot_plant);
            set!(self_localization, p.localizer.enabled);
            set!(use_position, p.skeleton.use_position);
            set!(correct_constraints, p.skeleton.correct_constraints);
        }
        if let Some(r) = m.ratios() {
            macro_rules! set {
                ($source:ident,$target:expr) => {
                    if let Some(v) = r.$source() {
                        $target = v;
                    }
                };
            }
            set!(impute_waist_from_chest_hip, p.skeleton.waist_from_chest_hip);
            set!(
                impute_waist_from_chest_legs,
                p.skeleton.waist_from_chest_legs
            );
            set!(impute_hip_from_chest_legs, p.skeleton.hip_from_chest_legs);
            set!(impute_hip_from_waist_legs, p.skeleton.hip_from_waist_legs);
            set!(interp_hip_legs, p.skeleton.hip_legs);
            set!(interp_knee_tracker_ankle, p.skeleton.knee_tracker_ankle);
            set!(interp_knee_ankle, p.skeleton.knee_ankle);
        }
        if let Some(r) = m.leg_tweaks() {
            if let Some(v) = r.correction_strength() {
                p.legs.correction_strength = v;
            }
        }
        if let Some(h) = m.skeleton_height() {
            if let Some(v) = h.hmd_height() {
                if !v.is_finite() || !(1.2..=1.936).contains(&v) {
                    return Err("invalid HMD height".into());
                }
                p.hmd_height = Some(v - h.floor_height().unwrap_or(0.0));
            }
        }
    }
    if let Some(r) = r.resets_settings() {
        p.reset_mounting_feet = r.reset_mounting_feet();
        p.reset_hmd_pitch = r.reset_hmd_pitch();
        p.arms_reset_mode = match r.arms_mounting_reset_mode().0 {
            0 => ArmsResetMode::Back,
            1 => ArmsResetMode::Forward,
            2 => ArmsResetMode::TposeUp,
            3 => ArmsResetMode::TposeDown,
            _ => return Err("unknown mounting mode".into()),
        };
        p.yaw_reset_smooth_seconds = r.yaw_reset_smooth_time();
        p.save_mounting_reset = r.save_mounting_reset();
    }
    if let Some(a) = r.stay_aligned() {
        p.alignment.enabled = a.enabled();
        p.alignment.hide_correction = a.hideYawCorrection();
        p.alignment.standing = RelaxedPose {
            enabled: a.standingEnabled(),
            upper_leg_degrees: a.standingUpperLegAngle(),
            lower_leg_degrees: a.standingLowerLegAngle(),
            foot_degrees: a.standingFootAngle(),
        };
        p.alignment.sitting = RelaxedPose {
            enabled: a.sittingEnabled(),
            upper_leg_degrees: a.sittingUpperLegAngle(),
            lower_leg_degrees: a.sittingLowerLegAngle(),
            foot_degrees: a.sittingFootAngle(),
        };
        p.alignment.flat = RelaxedPose {
            enabled: a.flatEnabled(),
            upper_leg_degrees: a.flatUpperLegAngle(),
            lower_leg_degrees: a.flatLowerLegAngle(),
            foot_degrees: a.flatFootAngle(),
        };
        // extraYawCorrection is deprecated and ignored by the upstream handler.
    }
    if let Some(t) = r.tap_detection_settings() {
        if let Some(mode) = t.setup_mode() {
            p.taps.setup_mode = mode;
        }
        let old = p.taps.enabled;
        p.taps.yaw_enabled = t.yaw_reset_enabled().unwrap_or(old && p.taps.yaw_enabled);
        p.taps.full_enabled = t.full_reset_enabled().unwrap_or(old && p.taps.full_enabled);
        p.taps.mounting_enabled = t
            .mounting_reset_enabled()
            .unwrap_or(old && p.taps.mounting_enabled);
        p.taps.enabled = p.taps.yaw_enabled || p.taps.full_enabled || p.taps.mounting_enabled;
        if let Some(v) = t.yaw_reset_delay() {
            p.taps.yaw_delay_ms = seconds(v)?;
        }
        if let Some(v) = t.full_reset_delay() {
            p.taps.full_delay_ms = seconds(v)?;
        }
        if let Some(v) = t.mounting_reset_delay() {
            p.taps.mounting_delay_ms = seconds(v)?;
        }
        if let Some(v) = t.yaw_reset_taps() {
            p.taps.yaw_taps = v;
        }
        if let Some(v) = t.full_reset_taps() {
            p.taps.full_taps = v;
        }
        if let Some(v) = t.mounting_reset_taps() {
            p.taps.mounting_taps = v;
        }
        if let Some(v) = t.number_trackers_over_threshold() {
            p.taps.max_moving = v as usize;
        }
        if let Some(v) = t.yaw_reset_tracker() {
            p.taps.yaw_tracker = Some(super::protocol::from_body(v)?);
        }
        if let Some(v) = t.full_reset_tracker() {
            p.taps.full_tracker = Some(super::protocol::from_body(v)?);
        }
        if let Some(v) = t.mounting_reset_tracker() {
            p.taps.mounting_tracker = Some(super::protocol::from_body(v)?);
        }
    }
    if let Some(a) = r.auto_bone_settings() {
        if let Some(save) = a.save_recordings() {
            c.save_recordings = save;
        }
        macro_rules! set {
            ($source:ident,$target:expr) => {
                if let Some(v) = a.$source() {
                    $target = v;
                }
            };
        }
        if let Some(v) = a.num_epochs() {
            c.auto_bone.epochs = u32::try_from(v).map_err(|_| "invalid epochs")?;
        }
        if let Some(v) = a.cursor_increment() {
            c.auto_bone.cursor_increment = positive(v)?;
        }
        if let Some(v) = a.min_data_distance() {
            c.auto_bone.min_distance = positive(v)?;
        }
        if let Some(v) = a.max_data_distance() {
            c.auto_bone.max_distance = positive(v)?;
        }
        set!(initial_adjust_rate, c.auto_bone.initial_adjust_rate);
        set!(adjust_rate_decay, c.auto_bone.adjust_rate_decay);
        set!(slide_error_factor, c.auto_bone.slide_factor);
        set!(offset_slide_error_factor, c.auto_bone.offset_slide_factor);
        set!(
            foot_height_offset_error_factor,
            c.auto_bone.foot_height_factor
        );
        set!(body_proportion_error_factor, c.auto_bone.proportion_factor);
        set!(height_error_factor, c.auto_bone.height_factor);
        set!(position_error_factor, c.auto_bone.position_factor);
        set!(
            position_offset_error_factor,
            c.auto_bone.position_offset_factor
        );
        set!(calc_init_error, c.auto_bone.calc_initial_error);
        set!(use_skeleton_height, c.auto_bone.use_skeleton_height);
        set!(randomize_frame_order, c.auto_bone.randomize);
        set!(scale_each_step, c.auto_bone.scale_each_step);
        if let Some(v) = a.rand_seed() {
            c.auto_bone.seed = v as u64;
        }
        if let Some(v) = a.sample_count() {
            c.sample_count = positive(v)?;
        }
        if let Some(v) = a.sample_rate_ms() {
            c.sample_ms = u64::try_from(v).map_err(|_| "invalid sample interval")?;
        }
    }
    if r.drift_compensation().is_some_and(|v| v.enabled()) {
        return Err("reset-history drift compensation is disabled in the reference server".into());
    }
    if let Some(v) = r.hid_settings() {
        c.yaml["hidConfig"]["trackersOverHID"] = serde_yaml_ng::Value::Bool(v.trackersOverHID());
    }
    if let Some(v) = r.velocity_settings() {
        c.pose.send_derived_velocity = v.send_derived_velocity();
    }
    fn endpoint(e: &mut crate::osc::config::Endpoint, v: rpc::OSCSettings<'_>) {
        e.enabled = v.enabled();
        e.port_in = v.port_in();
        e.port_out = v.port_out();
        if let Some(address) = v.address() {
            e.address = address.into();
        }
    }
    if let Some(v) = r.osc_router().and_then(|v| v.osc_settings()) {
        endpoint(&mut c.osc.router, v);
    }
    if let Some(v) = r.vrc_osc() {
        if let Some(e) = v.osc_settings() {
            endpoint(&mut c.osc.vrc.endpoint, e);
        }
        c.osc.vrc.oscquery_enabled = v.oscquery_enabled();
        if let Some(t) = v.trackers() {
            for (role, value) in [
                ("waist", t.waist()),
                ("chest", t.chest()),
                ("left_knee", t.knees()),
                ("right_knee", t.knees()),
                ("left_foot", t.feet()),
                ("right_foot", t.feet()),
                ("left_elbow", t.elbows()),
                ("right_elbow", t.elbows()),
            ] {
                c.osc.vrc.trackers.insert(role.into(), value);
            }
        }
    }
    if let Some(v) = r.vmc_osc() {
        if let Some(e) = v.osc_settings() {
            endpoint(&mut c.osc.vmc.endpoint, e);
        }
        c.osc.vmc.anchor_hip = v.anchor_hip();
        c.osc.vmc.mirror_tracking = v.mirror_tracking();
    }
    if let Some(t) = r.steam_vr_trackers() {
        c.steam_vr.automatic = t.automaticTrackerToggle();
        for (role, enabled) in [
            ("waist", t.waist()),
            ("chest", t.chest()),
            ("left_foot", t.left_foot()),
            ("right_foot", t.right_foot()),
            ("left_knee", t.left_knee()),
            ("right_knee", t.right_knee()),
            ("left_elbow", t.left_elbow()),
            ("right_elbow", t.right_elbow()),
            ("left_hand", t.left_hand()),
            ("right_hand", t.right_hand()),
        ] {
            c.steam_vr.trackers.insert(role.into(), enabled);
        }
    }
    if let Some(json) = r.vrm().and_then(|v| v.vrm_json()) {
        c.osc.vmc.vrm_json = if json.is_empty() {
            None
        } else {
            Some(json.into())
        };
    }
    c.validate()?;
    Ok(c)
}
