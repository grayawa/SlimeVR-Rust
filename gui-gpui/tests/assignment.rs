use slimevr_gpui::assignment::{allowed, targets};
use solarxr_protocol::datatypes::BodyPart as B;
use std::collections::HashSet;

#[test]
fn assignment_modes_match_original_physical_roles_and_common_slots() {
    for (mode, count) in [
        ("lower-body", 5),
        ("core", 6),
        ("enhanced-core", 8),
        ("full-body", 10),
        ("all", 20),
    ] {
        let slots = targets(mode, true);
        assert_eq!(
            slots.len(),
            if mode == "all" { count } else { count + 3 },
            "{mode}"
        );
        assert_eq!(
            slots.iter().map(|s| s.body).collect::<HashSet<_>>().len(),
            slots.len()
        );
        assert!(slots.iter().any(|s| s.body == B::CHEST.0), "{mode}");
        for s in slots {
            assert!((0.0..=0.94).contains(&s.label_y));
        }
    }
    assert!(!allowed("lower-body", "HIP"));
    assert!(allowed("lower-body", "CHEST"));
}
#[test]
fn mirror_changes_visual_side_without_changing_assignment_identity() {
    let a = targets("all", true);
    let b = targets("all", false);
    for t in &a {
        let other = b.iter().find(|s| s.body == t.body).unwrap();
        assert_eq!(t.y, other.y);
        let name = B(t.body).variant_name().unwrap();
        if name.starts_with("LEFT_") || name.starts_with("RIGHT_") {
            assert_ne!(t.left, other.left);
            assert!((t.x + other.x - 164.).abs() < 0.01);
        } else {
            assert_eq!(t.x, other.x);
        }
    }
    assert!(
        a.iter()
            .find(|s| s.body == B::LEFT_UPPER_LEG.0)
            .unwrap()
            .left
    );
    assert!(
        !b.iter()
            .find(|s| s.body == B::LEFT_UPPER_LEG.0)
            .unwrap()
            .left
    );
}
#[test]
fn every_mode_keeps_labels_apart_on_the_smallest_assignment_canvas() {
    for mode in ["lower-body", "core", "enhanced-core", "full-body", "all"] {
        for mirror in [true, false] {
            let points = targets(mode, mirror);
            for left in [true, false] {
                let mut ys: Vec<_> = points
                    .iter()
                    .filter(|p| p.left == left)
                    .map(|p| p.label_y)
                    .collect();
                ys.sort_by(f32::total_cmp);
                for pair in ys.windows(2) {
                    assert!((pair[1] - pair[0]) * 475. >= 44., "{mode}: {pair:?}");
                }
            }
        }
    }
}

fn physical(device: u8, body: B) -> slimevr_gpui::protocol::Tracker {
    slimevr_gpui::protocol::Tracker {
        key: slimevr_gpui::protocol::TrackerKey { device, sensor: 0 },
        body: body.0,
        status: 2,
        editable: true,
        mounting: Some([0., 1., 0., 0.]),
        ..Default::default()
    }
}
#[test]
fn selector_replaces_all_occupants_then_assigns_with_the_selected_mounting() {
    use slimevr_gpui::assignment::selection_requests;
    let a = physical(1, B::CHEST);
    let mut b = physical(2, B::HIP);
    b.mounting = Some([0., 0., 0., 1.]);
    let c = physical(3, B::CHEST);
    let requests = selection_requests(&[a, b.clone(), c], B::CHEST.0, Some(b.key));
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0].1["tracker_id"]["device_id"]["id"], 1);
    assert_eq!(requests[1].1["tracker_id"]["device_id"]["id"], 3);
    assert_eq!(requests[0].1["body_position"], 0);
    assert_eq!(requests[1].1["body_position"], 0);
    assert_eq!(requests[2].1["body_position"], B::CHEST.0);
    assert_eq!(requests[2].1["mounting_orientation"]["w"], 1.);
    assert_eq!(requests[2].1["allow_drift_compensation"], false);
    slimevr_gpui::rpc_generated::encode_rpc_batch(&requests, 10).unwrap();
}
#[test]
fn invalid_taps_do_not_clear_existing_assignments_and_dont_assign_only_clears_its_role() {
    use slimevr_gpui::assignment::selection_requests;
    let a = physical(1, B::CHEST);
    let mut virtual_tracker = physical(2, B::HIP);
    virtual_tracker.computed = true;
    assert!(
        selection_requests(
            &[a.clone(), virtual_tracker.clone()],
            B::CHEST.0,
            Some(virtual_tracker.key)
        )
        .is_empty()
    );
    let requests = selection_requests(&[a, physical(3, B::HIP)], B::CHEST.0, None);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].1["tracker_id"]["device_id"]["id"], 1);
    assert_eq!(requests[0].1["body_position"], 0);
    assert_eq!(requests[0].1["mounting_orientation"]["y"], 1.);
}
#[test]
fn dependency_warnings_follow_original_spine_alternatives_and_hip_exception() {
    use slimevr_gpui::assignment::warnings;
    assert!(warnings(&[physical(1, B::HIP)]).is_empty());
    let warnings = warnings(&[
        physical(1, B::LEFT_FOOT),
        physical(2, B::LEFT_UPPER_LEG),
        physical(3, B::UPPER_CHEST),
    ]);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].body, B::LEFT_FOOT.0);
    assert_eq!(warnings[0].satisfied, 6);
    assert_eq!(warnings[0].affected, vec![B::LEFT_LOWER_LEG.0]);
}
#[test]
fn original_label_groups_and_default_set_are_preserved() {
    let points = targets("all", true);
    assert!(points.iter().find(|p| p.body == B::NECK.0).unwrap().left);
    assert!(!points.iter().find(|p| p.body == B::WAIST.0).unwrap().left);
    for side in [true, false] {
        let mut legs: Vec<_> = points
            .iter()
            .filter(|p| p.left == side && B(p.body).variant_name().unwrap().ends_with("LEG"))
            .collect();
        legs.sort_by(|a, b| a.label_y.total_cmp(&b.label_y));
        assert!((legs[1].label_y - legs[0].label_y - 52. / 550.).abs() < 0.001);
    }
    assert_eq!(slimevr_gpui::assignment::preferred_mode(5), "lower-body");
    assert_eq!(slimevr_gpui::assignment::preferred_mode(6), "core");
    assert_eq!(slimevr_gpui::assignment::preferred_mode(9), "full-body");
    assert_eq!(slimevr_gpui::assignment::preferred_mode(21), "all");
}

#[test]
fn movement_highlight_is_sign_invariant_and_expires_after_half_a_second() {
    use slimevr_gpui::assignment::Motion;
    use std::time::{Duration, Instant};
    let mut motion = Motion::default();
    let now = Instant::now();
    let mut t = physical(1, B::CHEST);
    t.raw_rotation = Some([0., 0., 0., 1.]);
    motion.update(&[t.clone()], now);
    assert_eq!(motion.level(t.key), 0.);
    t.raw_rotation = Some([0., 0., 0., -1.]);
    motion.update(&[t.clone()], now + Duration::from_millis(100));
    assert_eq!(motion.level(t.key), 0.);
    t.raw_rotation = Some([0., 0.70710677, 0., 0.70710677]);
    motion.update(&[t.clone()], now + Duration::from_millis(200));
    assert_eq!(motion.level(t.key), 1.);
    motion.update(&[t.clone()], now + Duration::from_millis(800));
    assert_eq!(motion.level(t.key), 0.);
    motion.update(&[], now + Duration::from_millis(900));
    assert_eq!(motion.level(t.key), 0.);
}

#[test]
fn slow_movement_accumulates_per_tracker_and_expires_without_new_packets() {
    use slimevr_gpui::assignment::Motion;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut motion = Motion::default();
    let mut moving = physical(1, B::CHEST);
    let mut still = physical(2, B::HIP);
    moving.raw_rotation = Some([0., 0., 0., 1.]);
    still.raw_rotation = moving.raw_rotation;
    motion.update(&[moving.clone(), still.clone()], now);
    for i in 1..=4 {
        let half_angle = (i as f32 * 5.).to_radians() / 2.;
        moving.raw_rotation = Some([0., half_angle.sin(), 0., half_angle.cos()]);
        motion.update(
            &[moving.clone(), still.clone()],
            now + Duration::from_millis(i * 100),
        );
    }
    let expected = 4. * 2.5_f32.to_radians().sin().powi(2) * 50.;
    assert!((motion.level(moving.key) - expected).abs() < 0.001);
    assert_eq!(motion.level(still.key), 0.);
    assert!(motion.expire(now + Duration::from_millis(1000)));
    assert_eq!(motion.level(moving.key), 0.);
}

#[test]
fn movement_uses_original_linear_acceleration_and_supports_computed_cards() {
    use slimevr_gpui::assignment::Motion;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut motion = Motion::default();
    let mut t = physical(1, B::CHEST);
    t.computed = true;
    t.raw_rotation = Some([0., 0., 0., 1.]);
    t.linear_acceleration = Some([0., 0., 0.]);
    motion.update(&[t.clone()], now);
    t.acceleration = Some([100., 0., 0.]);
    motion.update(&[t.clone()], now + Duration::from_millis(100));
    assert_eq!(
        motion.level(t.key),
        0.,
        "raw acceleration must not replace world linear acceleration"
    );
    t.linear_acceleration = Some([20., 0., 0.]);
    motion.update(&[t.clone()], now + Duration::from_millis(200));
    assert!((motion.level(t.key) - 0.4).abs() < 0.001);
}

#[test]
fn disconnect_and_invalid_samples_cannot_leave_or_restart_a_glow() {
    use slimevr_gpui::assignment::Motion;
    use std::time::{Duration, Instant};
    let now = Instant::now();
    let mut motion = Motion::default();
    let mut t = physical(1, B::CHEST);
    t.raw_rotation = Some([0., 0., 0., 1.]);
    motion.update(&[t.clone()], now);
    t.raw_rotation = Some([0., 1., 0., 0.]);
    motion.update(&[t.clone()], now + Duration::from_millis(100));
    assert_eq!(motion.level(t.key), 1.);
    t.status = 1;
    motion.update(&[t.clone()], now + Duration::from_millis(200));
    assert_eq!(motion.level(t.key), 0.);
    t.status = 2;
    t.raw_rotation = Some([f32::NAN; 4]);
    t.linear_acceleration = Some([f32::INFINITY; 3]);
    motion.update(&[t.clone()], now + Duration::from_millis(300));
    t.raw_rotation = Some([0., 0., 0., 1.]);
    t.linear_acceleration = Some([20., 0., 0.]);
    motion.update(&[t.clone()], now + Duration::from_millis(400));
    assert_eq!(motion.level(t.key), 0.);
}
