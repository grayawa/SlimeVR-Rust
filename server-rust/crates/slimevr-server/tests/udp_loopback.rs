//! Exercise the real CLI/socket/journal boundary with six synthetic devices.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    net::UdpSocket,
    process::{Child, Command, Stdio},
    time::Duration,
};

struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wire(id: u32, seq: i64, payload: &[u8]) -> Vec<u8> {
    [
        id.to_be_bytes().as_slice(),
        seq.to_be_bytes().as_slice(),
        payload,
    ]
    .concat()
}
fn handshake(device: u8) -> Vec<u8> {
    let mut body: Vec<_> = [9u32, 13, 1, 0, 0, 0, 22]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    body.extend(b"\x05good\0");
    body.extend([2, 0, 0, 0, 0, device]);
    wire(3, 0, &body)
}

#[test]
fn six_devices_over_real_udp_and_cli_replay_produce_identical_state() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("loopback.jsonl");
    let config = dir.path().join("pose.json");
    let bodies = [
        "chest",
        "hip",
        "left_upper_leg",
        "right_upper_leg",
        "left_lower_leg",
        "right_lower_leg",
    ];
    let bindings:Vec<_>=bodies.iter().enumerate().map(|(i,body)|serde_json::json!({"device_key":format!("02:00:00:00:00:{:02X}",i+1),"sensor_id":0,"body":body})).collect();
    std::fs::write(
        &config,
        serde_json::to_vec(&serde_json::json!({"bindings":bindings})).unwrap(),
    )
    .unwrap();
    let mut child = Running(
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .args([
                "listen",
                "--bind",
                "127.0.0.1:0",
                "--accept-new-devices",
                "--no-discovery",
                "--run-for",
                "3",
                "--record",
            ])
            .arg(&journal)
            .arg("--pose-config")
            .arg(&config)
            .args(["--pose-ms", "20", "--log-level", "debug"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    assert!(
        output.read_line(&mut line).unwrap() > 0,
        "receiver did not start"
    );
    let started: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(started["type"], "listening");
    let address = started["bind"].as_str().unwrap();
    let mut devices = Vec::new();
    for number in 1..=6 {
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        socket.connect(address).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        socket.send(&handshake(number)).unwrap();
        let mut buf = [0; 1500];
        let count = socket.recv(&mut buf).unwrap();
        assert_eq!(count, 64);
        assert_eq!(&buf[..13], b"\x03Hey OVR =D 5");
        // Drain the capability advertisement before SensorInfo ACK.
        let count = socket.recv(&mut buf).unwrap();
        assert_eq!(&buf[..count], &wire(22, 0, &[3]));
        socket.send(&wire(15, 1, &[0, 1, 13])).unwrap();
        let count = socket.recv(&mut buf).unwrap();
        assert_eq!(&buf[..count], &[0, 0, 0, 15, 0, 1]);
        socket.send(&wire(22, 2, &[7])).unwrap();
        let count = socket.recv(&mut buf).unwrap();
        assert_eq!(&buf[..count], &wire(22, 0, &[3]));
        devices.push(socket);
    }
    let mut rotation = vec![0, 1];
    for n in [0f32, 0.0, 0.0, 1.0] {
        rotation.extend(n.to_be_bytes());
    }
    rotation.push(3);
    for socket in &devices {
        socket.send(&wire(17, 3, &rotation)).unwrap();
        socket.send(&wire(17, 3, &rotation)).unwrap(); // rejected duplicate
        socket.send(&wire(17, 2, &rotation)).unwrap(); // rejected reordering
    }
    devices[0].send(&wire(15, 4, &[1, 1, 13])).unwrap();
    let mut packed = vec![23, 1];
    for n in [0i16, 0, 0, 32767, 128, 256, 384] {
        packed.extend(n.to_be_bytes());
    }
    let compact = [vec![packed.len() as u8], packed].concat();
    devices[0].send(&wire(101, 5, &compact)).unwrap();
    devices[0].send(&[0, 0, 0, 17]).unwrap(); // malformed header
                                              // Collect diagnostics while waiting for the receiver's scheduled shutdown.
    let mut rest = String::new();
    output.read_to_string(&mut rest).unwrap();
    let status = child.0.wait().unwrap();
    if !status.success() {
        let mut err = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut err)
            .unwrap();
        panic!("{err}");
    }
    let received: Vec<Value> = rest
        .lines()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).unwrap();
            value.as_object_mut().unwrap().remove("level");
            value
        })
        .collect();
    let final_state = received
        .iter()
        .rev()
        .find(|v| v["type"] == "snapshot")
        .unwrap();
    assert_eq!(final_state["devices"].as_object().unwrap().len(), 6);
    assert_eq!(final_state["counters"]["samples"], 7);
    assert_eq!(final_state["counters"]["sequence_rejected"], 12);
    assert_eq!(final_state["counters"]["malformed"], 1);
    let replay = Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .arg("replay")
        .arg(&journal)
        .output()
        .unwrap();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    let replayed: Vec<Value> = String::from_utf8(replay.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        replayed.iter().find(|v| v["type"] == "snapshot").unwrap(),
        final_state
    );
    assert!(
        replayed.last().unwrap()["verified_replies"]
            .as_u64()
            .unwrap()
            >= 25
    );
    let solve = || {
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .arg("solve-recording")
            .arg(&journal)
            .arg("--config")
            .arg(&config)
            .output()
            .unwrap()
    };
    let solved = solve();
    assert!(
        solved.status.success(),
        "{}",
        String::from_utf8_lossy(&solved.stderr)
    );
    let poses: Vec<Value> = String::from_utf8(solved.stdout.clone())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(poses[0]["type"], "pose_snapshot");
    let live = received
        .iter()
        .rev()
        .find(|v| v["type"] == "pose_snapshot")
        .unwrap();
    assert!(
        received
            .iter()
            .filter(|v| v["type"] == "pose_snapshot")
            .count()
            > 2
    );
    assert!(live["at_ms"].as_u64().unwrap() >= 3000);
    assert!(received
        .iter()
        .filter(|v| v["type"] == "pose_snapshot")
        .collect::<Vec<_>>()
        .windows(2)
        .all(|p| p[0]["at_ms"].as_u64().unwrap() <= p[1]["at_ms"].as_u64().unwrap()));
    assert_eq!(
        &poses[0], live,
        "live and journal replay must solve every event and tick identically"
    );
    assert_eq!(poses[0]["trackers"].as_array().unwrap().len(), 6);
    assert!(!poses[0]["skeleton"]["world_anchor_present"]
        .as_bool()
        .unwrap());
    assert!(
        poses[0]["skeleton"]["computed"]["left_foot"]["position"]["y"]
            .as_f64()
            .unwrap()
            .is_finite()
    );
    assert_eq!(
        solved.stdout,
        solve().stdout,
        "same UDP journal must solve identically on repeated runs"
    );
}

#[test]
fn backend_replacement_allows_firmware_timeout_and_rehandshake_without_reboot() {
    use std::time::Instant;
    let tracker = UdpSocket::bind("127.0.0.1:0").unwrap();
    tracker
        .set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    let target = tracker.local_addr().unwrap().to_string();
    let mut child = Running(
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .args([
                "listen",
                "--bind",
                "127.0.0.1:0",
                "--accept-new-devices",
                "--discovery-target",
                &target,
                "--run-for",
                "6",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut output = BufReader::new(child.0.stdout.take().unwrap());
    let mut line = String::new();
    output.read_line(&mut line).unwrap();
    let started: Value = serde_json::from_str(&line).unwrap();
    let address = started["bind"].as_str().unwrap().to_owned();
    // Drain stdout so snapshots cannot block the server clock.
    let reader = std::thread::spawn(move || {
        let mut remainder = String::new();
        output.read_to_string(&mut remainder).unwrap();
        remainder
    });
    let start = Instant::now();
    let mut last_server_packet = start;
    let mut connected_to_old_server = true;
    let mut discovered = false;
    let mut buf = [0u8; 1500];
    while start.elapsed() < Duration::from_secs(5) {
        // Model the official firmware: any server datagram refreshes the 3s
        // connection timeout. It sends its identity only after timing out.
        if connected_to_old_server && last_server_packet.elapsed() > Duration::from_secs(3) {
            connected_to_old_server = false;
            tracker.send_to(&handshake(1), &address).unwrap();
        }
        if connected_to_old_server {
            tracker.send_to(&wire(0, 1, &[]), &address).unwrap();
        }
        match tracker.recv_from(&mut buf) {
            Ok((size, _)) => {
                last_server_packet = Instant::now();
                if size == 64 && &buf[..13] == b"\x03Hey OVR =D 5" {
                    discovered = true;
                    tracker
                        .send_to(&wire(15, 1, &[0, 1, 13]), &address)
                        .unwrap();
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(error) => panic!("tracker socket failed: {error}"),
        }
    }
    assert!(child.0.wait().unwrap().success());
    let remainder = reader.join().unwrap();
    assert!(
        discovered,
        "discovery broadcasts kept the old firmware connection alive"
    );
    assert!(
        remainder.contains("sensor_registered"),
        "new handshake did not register the tracker"
    );
}
