use serde_json::Value;
use slimevr_server::{
    protocol::{self, Packet},
    recording::decode_hex,
};

fn compare(actual: &Value, expected: &Value, path: &str) {
    match expected {
        Value::Object(map) => {
            for (key, value) in map {
                compare(&actual[key], value, &format!("{path}.{key}"));
            }
        }
        Value::Array(values) => {
            let actual = actual
                .as_array()
                .unwrap_or_else(|| panic!("{path}: expected array"));
            assert_eq!(actual.len(), values.len(), "{path}");
            for (i, (a, e)) in actual.iter().zip(values).enumerate() {
                compare(a, e, &format!("{path}[{i}]"));
            }
        }
        Value::Number(n) => {
            let a = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path}: expected numeric value, got {actual}"));
            let e = n.as_f64().unwrap();
            assert!(
                (a - e).abs() <= 1e-6 * (1.0 + e.abs()),
                "{path}: {a} != {e}"
            );
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn actual_kotlin_parser_math_and_reply_goldens_match() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/udp-golden.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "83941fd38e91cc91ca6b360deab5c2ae986dd1b6"
    );
    let replies = &fixture["replies"];
    assert_eq!(
        protocol::handshake_response(),
        decode_hex(replies["handshake"].as_str().unwrap()).unwrap()
    );
    assert_eq!(
        protocol::sensor_info_response(1, 1),
        decode_hex(replies["sensor_info"].as_str().unwrap()).unwrap()
    );
    assert_eq!(
        protocol::header(22, &[3]),
        decode_hex(replies["features"].as_str().unwrap()).unwrap()
    );
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let datagram =
            protocol::parse(&decode_hex(case["hex"].as_str().unwrap()).unwrap()).unwrap();
        compare(
            &serde_json::to_value(&datagram.packets).unwrap(),
            &case["packets"],
            name,
        );
        let rotations: Vec<_> = datagram
            .packets
            .iter()
            .filter_map(|p| match p {
                Packet::Rotation { rotation, .. } => Some(rotation.udp_to_server()),
                _ => None,
            })
            .collect();
        compare(
            &serde_json::to_value(rotations).unwrap(),
            &case["server_rotations"],
            &format!("{name}.axes"),
        );
        let legacy_accel: Vec<_> = datagram
            .packets
            .iter()
            .filter_map(|p| match p {
                Packet::Acceleration { acceleration, .. }
                | Packet::Rotation {
                    acceleration: Some(acceleration),
                    ..
                } => Some(acceleration.legacy_acceleration_to_server()),
                _ => None,
            })
            .collect();
        compare(
            &serde_json::to_value(legacy_accel).unwrap(),
            &case["legacy_accelerations"],
            &format!("{name}.legacy_accel"),
        );
    }
}
