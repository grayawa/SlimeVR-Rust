use serde_json::{json, Value};
use slimevr_core::{
    calibration::Calibration,
    filtering::{FilterConfig, FilterType, QuaternionFilter},
    skeleton::{BodyPosition, HeadPose, Skeleton, SkeletonConfig},
    Quaternion as Q, Vector3 as V,
};
use std::collections::BTreeMap;

fn q(n: &Value) -> Q {
    serde_json::from_value(n.clone()).unwrap()
}

#[derive(Default)]
struct SkeletonErrors {
    positions_mm: Vec<f32>,
    rotations_degrees: Vec<f32>,
}
impl SkeletonErrors {
    fn observe(
        &mut self,
        bones: &BTreeMap<String, slimevr_core::skeleton::BonePose>,
        expected: &Value,
    ) {
        for (name, bone) in bones {
            // Compare only bones present in each reference case.
            if expected.get(name).is_none() {
                continue;
            }
            let e = &expected[name];
            for (actual, target) in [(bone.head, &e["head"]), (bone.tail, &e["tail"])] {
                let target: V = serde_json::from_value(target.clone()).unwrap();
                self.positions_mm.push((actual - target).len() * 1000.0);
            }
            self.rotations_degrees
                .push(bone.rotation.angle_to_r(q(&e["rotation"])).to_degrees());
        }
    }
    fn report(&mut self) {
        fn metrics(values: &mut [f32]) -> Value {
            values.sort_by(f32::total_cmp);
            let percentile = |p: f64| values[((values.len() - 1) as f64 * p).ceil() as usize];
            json!({"count":values.len(),"max":percentile(1.0),"p95":percentile(0.95),"p99":percentile(0.99)})
        }
        let report = json!({"position_error_mm":metrics(&mut self.positions_mm),"rotation_error_degrees":metrics(&mut self.rotations_degrees)});
        assert!(report["position_error_mm"]["max"].as_f64().unwrap() < 1.0);
        assert!(report["rotation_error_degrees"]["max"].as_f64().unwrap() < 0.05);
        if let Ok(path) = std::env::var("SLIMEVR_CORE_METRICS") {
            std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        }
        println!("core_reference_metrics={report}");
    }
}
fn compare(a: &Value, e: &Value, path: &str) {
    match e {
        Value::Object(map) => {
            for (key, e) in map {
                compare(&a[key], e, &format!("{path}.{key}"));
            }
        }
        Value::Array(values) => {
            let actual = a.as_array().unwrap();
            assert_eq!(actual.len(), values.len(), "{path}");
            for (i, (a, e)) in actual.iter().zip(values).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        Value::Number(n) => {
            let actual = a.as_f64().unwrap_or_else(|| panic!("{path}: {a}"));
            let expected = n.as_f64().unwrap();
            assert!(
                (actual - expected).abs() <= 3e-6 * (1.0 + expected.abs()),
                "{path}: {actual} != {expected}"
            );
        }
        _ => assert_eq!(a, e, "{path}"),
    }
}
fn cases(kind: &str) -> Vec<Value> {
    let f: Value = serde_json::from_str(include_str!("fixtures/core-golden.json")).unwrap();
    assert_eq!(
        f["reference_commit"],
        "83941fd38e91cc91ca6b360deab5c2ae986dd1b6"
    );
    f["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["kind"] == kind)
        .cloned()
        .collect()
}
#[test]
fn ktmath_branches_signs_projection_and_extrapolation_match() {
    for c in cases("math") {
        let a = q(&c["a"]);
        let b = q(&c["b"]);
        let t = c["t"].as_f64().unwrap() as f32;
        let v: V = serde_json::from_value(c["vector"].clone()).unwrap();
        let output = json!({"mul":a*b,"inverse":a.inv(),"unit":a.unit(),"pow":a.pow(t),"interp_q":a.interp_q(b,t),"interp_r":a.interp_r(b,t),"twin_nearest":a.twin_nearest(b),"twin_extended_back":a.twin_extended_back(b),"project":a.project(V::UP),"yaw_yzx":a.yaw_yzx(),"euler_yxz":a.euler_yxz(),"rotate":a.rotate(v),"angle":a.angle_to_r(b)});
        compare(&output, &c["expected"], c["name"].as_str().unwrap());
    }
}
#[test]
fn actual_moving_average_variable_ticks_reset_and_six_delta_window_match() {
    for c in cases("filter") {
        let mode: FilterType = serde_json::from_value(c["mode"].clone()).unwrap();
        let mut filter = QuaternionFilter::new(
            FilterConfig {
                mode,
                amount: c["amount"].as_f64().unwrap() as f32,
            },
            q(&c["initial"]),
        );
        let outputs: Vec<_> = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| {
                match step["op"].as_str().unwrap() {
                    "sample" => filter.add(q(&step["q"])),
                    "tick" => filter.update(step["dt"].as_f64().unwrap() as f32),
                    "reset" => filter.reset(q(&step["q"]), q(&step["reference"])),
                    _ => panic!(),
                }
                json!({"rotation":filter.output(),"impact":filter.impact})
            })
            .collect();
        compare(&json!(outputs), &c["expected"], c["name"].as_str().unwrap());
    }
}
#[test]
fn actual_tracker_resets_and_yaw_interpolation_match_torso_and_legs() {
    for c in cases("calibration") {
        let mut calibration = Calibration::new(q(&c["mounting"]));
        let mut raw = Q::IDENTITY;
        let body: BodyPosition = serde_json::from_value(c["body"].clone()).unwrap();
        let mode =
            serde_json::from_value(c.get("arms_mode").cloned().unwrap_or(json!("back"))).unwrap();
        let outputs:Vec<_>=c["steps"].as_array().unwrap().iter().map(|step| {
            match step["op"].as_str().unwrap() {
                "sample"=>raw=q(&step["q"]),"full"=>if c["computed"].as_bool().unwrap_or(false){calibration.reset_full_computed(raw,q(&step["reference"]),body,mode)}else{calibration.reset_full_body(raw,q(&step["reference"]),body,mode)},
                "yaw"=>calibration.reset_yaw(raw,q(&step["reference"]),c["smooth_seconds"].as_f64().unwrap() as f32),
                "mounting"=>calibration.reset_mounting_body(raw,q(&step["reference"]),body,mode),
                "tick"=>calibration.tick(step["dt"].as_f64().unwrap() as f32),_=>panic!(),
            }json!({"adjusted":calibration.adjust(raw),"mount_rot_fix":calibration.mount_rot_fix,"transition":calibration.yaw_transition})
        }).collect();
        compare(&json!(outputs), &c["expected"], c["name"].as_str().unwrap());
    }
}
#[test]
fn extracted_human_skeleton_and_actual_bone_fk_match_six_point_and_missing_inputs() {
    let mut errors = SkeletonErrors::default();
    for c in cases("skeleton") {
        let config: SkeletonConfig = serde_json::from_value(c["config"].clone()).unwrap();
        let inputs: BTreeMap<BodyPosition, Q> =
            serde_json::from_value(c["inputs"].clone()).unwrap();
        let head: Option<HeadPose> = serde_json::from_value(c["head"].clone()).unwrap();
        let positions =
            serde_json::from_value(c.get("positions").cloned().unwrap_or(json!({}))).unwrap();
        let out = Skeleton::new(config)
            .solve_with_positions(&inputs, &positions, head, false)
            .unwrap();
        errors.observe(&out.bones, &c["expected"]);
        compare(
            &serde_json::to_value(&out.bones).unwrap(),
            &c["expected"],
            c["name"].as_str().unwrap(),
        );
        // Computed trackers expose bone tails with rotationOffset removed.
        for (name, pose) in &out.computed {
            let expected = &c["expected"][format!("{name}_tracker")];
            if expected.is_null() {
                continue;
            }
            compare(
                &serde_json::to_value(pose.position).unwrap(),
                &expected["tail"],
                name,
            );
            let rotation = q(&expected["rotation"]) * q(&expected["rotation_offset"]).inv();
            assert!(pose.rotation.angle_to_r(rotation) < 2e-5);
        }
    }
    for c in cases("skeleton_sequence") {
        let config: SkeletonConfig =
            serde_json::from_value(c["frames"][0]["config"].clone()).unwrap();
        let mut skeleton = Skeleton::new(config);
        let outputs: Vec<_> = c["frames"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(index, frame)| {
                let inputs = serde_json::from_value(frame["inputs"].clone()).unwrap();
                let head = serde_json::from_value(frame["head"].clone()).unwrap();
                let out = skeleton.solve(&inputs, head, false).unwrap();
                errors.observe(&out.bones, &c["expected"][index]);
                serde_json::to_value(out.bones).unwrap()
            })
            .collect();
        compare(&json!(outputs), &c["expected"], c["name"].as_str().unwrap());
    }
    errors.report();
}

#[test]
fn actual_leg_tweaks_clip_skating_plant_toe_and_contact_history_match() {
    use slimevr_core::legs::{FootState, LegConfig, LegTweaks};
    for c in cases("legs") {
        let config: LegConfig = serde_json::from_value(c["legs"].clone()).unwrap();
        let mut legs = LegTweaks::new(config);
        let localized = c
            .get("localizer")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        legs.set_localizer(localized);
        let mut localizer = slimevr_core::localizer::Localizer::new(localized);
        if c["reset_floor"].as_bool().unwrap() {
            legs.reset(true);
        }
        let mut skeleton =
            Skeleton::new(serde_json::from_value(c["frames"][0]["config"].clone()).unwrap());
        let mut outputs = Vec::new();
        for frame in c["frames"].as_array().unwrap() {
            let inputs = serde_json::from_value(frame["inputs"].clone()).unwrap();
            let head = serde_json::from_value(frame["head"].clone()).unwrap();
            let accel = serde_json::from_value(frame["accelerations"].clone()).unwrap();
            if localized {
                skeleton.set_root_override(Some(localizer.root));
            }
            let mut p = skeleton.solve(&inputs, head, false).unwrap();
            legs.update(frame["at_ms"].as_u64().unwrap(), &mut p, &inputs, &accel)
                .unwrap();
            localizer.update(&legs, &p, &accel);
            let f = legs.frame().unwrap();
            let state = f.state.map(|s| match s {
                FootState::Unknown => 0,
                FootState::Locked => 1,
                FootState::Unlocked => 2,
            });
            outputs.push(json!({"root":p.bones["head"].head,"floor":legs.floor,"feet":f.feet,"knees":f.knees,"hip":f.hip,"corrected_feet":f.corrected_feet,"corrected_knees":f.corrected_knees,"corrected_hip":f.corrected_hip,"corrected_rotations":f.corrected_rotations,"state":state,"numerical":f.numerical,"com":f.com,"standing":f.standing}));
        }
        let mut expected = c["expected"].as_array().unwrap().clone();
        let mut max_score_error = 0.0_f64;
        let constrained_lift = c["name"].as_str().unwrap().starts_with("legs_single_lift_")
            && c["name"].as_str().unwrap().ends_with("_constraints_True");
        for (actual, target) in outputs.iter_mut().zip(expected.iter_mut()) {
            let actual_scores = actual.as_object_mut().unwrap().remove("numerical").unwrap();
            let target_scores = target.as_object_mut().unwrap().remove("numerical").unwrap();
            for (a, e) in actual_scores
                .as_array()
                .unwrap()
                .iter()
                .zip(target_scores.as_array().unwrap())
            {
                let a = a.as_f64().unwrap();
                let e = e.as_f64().unwrap();
                max_score_error = max_score_error.max((a - e).abs());
                // Constraint normalization differs by sub-micrometer roundoff;
                // differentiating positions at 4ms amplifies this in the contact score.
                // Keep positions, rotations and discrete lock states at their strict tolerance.
                let score_roundoff = if constrained_lift { 1e-4 } else { 0.0 };
                assert!(
                    (a - e).abs() <= score_roundoff + 3e-6 * (1.0 + e.abs()),
                    "{} contact score: {a} != {e}",
                    c["name"]
                );
            }
        }
        compare(
            &json!(outputs),
            &json!(expected),
            c["name"].as_str().unwrap(),
        );
        println!("{} max_contact_score_error={max_score_error}", c["name"]);
    }
}

#[test]
fn actual_stay_aligned_rest_and_moving_gradients_match() {
    use slimevr_core::alignment::{AlignmentConfig, RelaxedPose, StayAligned};
    for c in cases("alignment") {
        let pose = if c["relaxed"].is_object() {
            let mut p: RelaxedPose = serde_json::from_value(c["relaxed"].clone()).unwrap();
            p.enabled = true;
            p
        } else {
            RelaxedPose {
                enabled: true,
                ..Default::default()
            }
        };
        let mut alignment = StayAligned::new(AlignmentConfig {
            enabled: true,
            standing: pose,
            sitting: pose,
            flat: pose,
            ..Default::default()
        });
        let mut last = 0;
        let mut outputs = Vec::new();
        for frame in c["frames"].as_array().unwrap() {
            let at = frame["at_ms"].as_u64().unwrap();
            let base: BTreeMap<BodyPosition, Q> =
                serde_json::from_value(frame["rotations"].clone()).unwrap();
            for (b, q) in &base {
                alignment.update_rest(at, *b, *q, *q);
            }
            let rates = base.keys().map(|b| (*b, 0.15)).collect();
            alignment.adjust((at - last) as f32 / 1000.0, &base, &rates);
            last = at;
            let states: BTreeMap<_, _> = alignment
                .states
                .iter()
                .map(|(b, s)| {
                    (
                        *b,
                        json!({"correction":s.yaw_correction,"rest":s.rest.state}),
                    )
                })
                .collect();
            outputs.push(json!(states));
        }
        compare(&json!(outputs), &c["expected"], c["name"].as_str().unwrap());
    }
}

#[test]
fn actual_autobone_kotlin_seeded_frame_orders_match() {
    for c in cases("frame_orders") {
        let actual = slimevr_core::autobone::frame_orders(
            c["count"].as_u64().unwrap() as usize,
            c["epochs"].as_u64().unwrap() as usize,
            c["seed"].as_u64().unwrap(),
        )
        .unwrap();
        assert_eq!(json!(actual), c["expected"]);
    }
}

#[test]
fn actual_tracker_derived_velocity_boundaries_and_lifecycle_match() {
    let cases = cases("velocity");
    assert_eq!(cases.len(), 2);
    for c in cases {
        let mut estimator = slimevr_core::velocity::DerivedVelocity::default();
        let actual: Vec<_> = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| {
                if step["reset"].as_bool() == Some(true) {
                    estimator.reset();
                }
                let position =
                    if step["enabled"].as_bool() == Some(false) || step["position"].is_null() {
                        None
                    } else {
                        Some(serde_json::from_value::<V>(step["position"].clone()).unwrap())
                    };
                estimator.update(step["at_us"].as_u64().unwrap(), position)
            })
            .collect();
        compare(
            &serde_json::to_value(actual).unwrap(),
            &c["expected"],
            c["name"].as_str().unwrap(),
        );
    }
}

#[test]
fn actual_hmd_full_reset_preserves_heading_and_removes_pitch_match() {
    let cases = cases("hmd_calibration");
    assert_eq!(cases.len(), 3);
    for c in cases {
        let mut calibration = slimevr_core::calibration::HmdCalibration::default();
        let mut raw = Q::IDENTITY;
        let actual: Vec<_> = c["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| {
                if step["rotation"].is_object() {
                    raw = q(&step["rotation"]);
                }
                if step["full"].as_bool() == Some(true) {
                    calibration
                        .reset_full(raw, step["enabled"].as_bool().unwrap_or(false))
                        .unwrap();
                }
                calibration.adjust(raw)
            })
            .collect();
        compare(
            &serde_json::to_value(actual).unwrap(),
            &c["expected"],
            c["name"].as_str().unwrap(),
        );
    }
}

#[test]
fn original_flex_axes_ranges_and_resistance_reversal_match() {
    for case in cases("flex") {
        let body = serde_json::from_value(case["body"].clone()).unwrap();
        let mut flex = slimevr_core::flex::FlexSensor::default();
        let outputs = case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|step| {
                let value = step["value"].as_f64().unwrap_or(0.0) as f32;
                let q = match step["op"].as_str().unwrap() {
                    "resistance" => flex.resistance(body, value).unwrap(),
                    "angle" => slimevr_core::flex::FlexSensor::angle(body, value),
                    "min" => flex.reset_min(body),
                    "max" => flex.reset_max(body),
                    _ => panic!(),
                };
                serde_json::to_value(q).unwrap()
            })
            .collect::<Vec<_>>();
        compare(
            &json!(outputs),
            &case["expected"],
            case["name"].as_str().unwrap(),
        );
    }
}
