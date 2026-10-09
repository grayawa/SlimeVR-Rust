//! Verify real UDP + machine journal behavior under live diagnostic filtering.
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
fn handshake() -> Vec<u8> {
    let mut body: Vec<_> = [9u32, 13, 1, 0, 0, 0, 22]
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    body.extend(b"\x05good\0");
    body.extend([2, 0, 0, 0, 0, 1]);
    wire(3, 0, &body)
}
fn capture(level: Option<&str>, events: bool) -> Vec<Value> {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("pose.json");
    std::fs::write(&config, "{}").unwrap();
    let journal = dir.path().join("receiver.jsonl");
    let mut command = Command::new(env!("CARGO_BIN_EXE_slimevr-server"));
    command
        .env_remove("SLIMEVR_LOG_LEVEL")
        .args([
            "listen",
            "--bind",
            "127.0.0.1:0",
            "--no-discovery",
            "--accept-new-devices",
            "--run-for",
            "1",
            "--summary-ms",
            "100",
            "--pose-output-ms",
            "100",
            "--pose-config",
        ])
        .arg(&config)
        .arg("--record")
        .arg(&journal);
    if let Some(level) = level {
        command.args(["--log-level", level]);
    }
    if events {
        command.arg("--events");
    }
    let mut child = Running(
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    let listening: Value = serde_json::from_str(&first).unwrap();
    assert_eq!(listening["type"], "listening");
    let reader = std::thread::spawn(move || {
        let mut out = first;
        stdout.read_to_string(&mut out).unwrap();
        out
    });
    let mut stderr = child.0.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut out = String::new();
        stderr.read_to_string(&mut out).unwrap();
        out
    });
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect(listening["bind"].as_str().unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    socket.send(&handshake()).unwrap();
    let mut buffer = [0; 1500];
    socket.recv(&mut buffer).unwrap();
    socket.recv(&mut buffer).unwrap();
    socket.send(&wire(15, 1, &[0, 1, 13])).unwrap();
    socket.recv(&mut buffer).unwrap();
    let mut rotation = vec![0, 1];
    for n in [0f32, 0., 0., 1.] {
        rotation.extend(n.to_be_bytes());
    }
    rotation.push(3);
    socket.send(&wire(17, 2, &rotation)).unwrap();
    socket.send(&[0, 0, 0]).unwrap(); // malformed datagram remains a warning at default verbosity
    assert!(child.0.wait().unwrap().success());
    let out = reader.join().unwrap();
    let err = errors.join().unwrap();
    let warnings: Vec<Value> = err
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(warnings
        .iter()
        .any(|v| v["type"] == "rejected" && v["level"] == "warn"));
    let replay = slimevr_server::recording::replay(&journal, |_| {}).unwrap();
    assert_eq!(
        replay.receiver.snapshot(replay.at_ms)["counters"]["samples"],
        1
    );
    assert_eq!(
        replay.receiver.snapshot(replay.at_ms)["counters"]["malformed"],
        1
    );
    assert!(replay.verified_replies > 0);
    out.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn default_info_preserves_connections_and_recordings_without_full_snapshots() {
    let records = capture(None, false);
    assert!(records.iter().any(|v| v["type"] == "device_connected"));
    assert!(records.iter().any(|v| v["type"] == "stopped"));
    assert!(records.iter().all(|v| v["level"] == "info"));
    assert!(!records.iter().any(|v| matches!(
        v["type"].as_str(),
        Some("snapshot" | "pose_snapshot" | "sample")
    )));
}
#[test]
fn debug_restores_full_snapshots_while_trace_and_legacy_events_restore_samples() {
    let debug = capture(Some("debug"), false);
    assert!(debug
        .iter()
        .any(|v| v["type"] == "pose_snapshot" && v["level"] == "debug"));
    assert!(debug
        .iter()
        .any(|v| v["type"] == "snapshot" && v["level"] == "debug"));
    assert!(!debug.iter().any(|v| v["type"] == "sample"));
    for (level, events) in [(Some("trace"), false), (None, true)] {
        let trace = capture(level, events);
        assert!(trace
            .iter()
            .any(|v| v["type"] == "sample" && v["level"] == "trace"));
    }
}
#[test]
fn environment_overrides_default_cli_overrides_environment_and_invalid_values_fail() {
    let run = |env: &str, args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .env("SLIMEVR_LOG_LEVEL", env)
            .args([
                "listen",
                "--bind",
                "127.0.0.1:0",
                "--no-discovery",
                "--run-for",
                "1",
            ])
            .args(args)
            .output()
            .unwrap()
    };
    let quiet = run("error", &[]);
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty());
    let explicit = run("invalid", &["--log-level", "info"]);
    assert!(explicit.status.success());
    assert!(!explicit.stdout.is_empty());
    let invalid = run("invalid", &[]);
    assert!(!invalid.status.success());
    let failure: Value = serde_json::from_slice(&invalid.stderr).unwrap();
    assert_eq!(failure["level"], "error");
    assert_eq!(failure["type"], "backend_fatal_error");
}

#[test]
fn unread_diagnostic_pipe_does_not_stop_tracker_handshakes() {
    let dir = tempfile::tempdir().unwrap();
    let pose = dir.path().join("pose.json");
    std::fs::write(&pose, "{}").unwrap();
    let mut child = Running(
        Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
            .env_remove("SLIMEVR_LOG_LEVEL")
            .args([
                "listen",
                "--bind",
                "127.0.0.1:0",
                "--no-discovery",
                "--accept-new-devices",
                "--run-for",
                "2",
                "--log-level",
                "trace",
                "--pose-ms",
                "1",
                "--pose-output-ms",
                "1",
                "--pose-config",
            ])
            .arg(pose)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut stdout = BufReader::new(child.0.stdout.take().unwrap());
    let stderr = child.0.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink()).unwrap()
    });
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    let listening: Value = serde_json::from_str(&first).unwrap();
    // No stdout consumer: rapid full pose snapshots fill the child pipe.
    std::thread::sleep(Duration::from_millis(300));
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.connect(listening["bind"].as_str().unwrap()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    socket.send(&handshake()).unwrap();
    let mut buffer = [0; 1500];
    socket
        .recv(&mut buffer)
        .expect("blocked logs must not stall handshake");
    socket.recv(&mut buffer).unwrap();
    socket.send(&wire(15, 1, &[0, 1, 13])).unwrap();
    socket
        .recv(&mut buffer)
        .expect("sensor registration must remain responsive");
    let reader =
        std::thread::spawn(move || std::io::copy(&mut stdout, &mut std::io::sink()).unwrap());
    assert!(child.0.wait().unwrap().success());
    reader.join().unwrap();
    errors.join().unwrap();
}

#[test]
fn timing_windows_emit_percentiles_cumulative_stalls_and_final_partial_window() {
    let dir = tempfile::tempdir().unwrap();
    let pose = dir.path().join("pose.json");
    std::fs::write(&pose, "{}").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_slimevr-server"))
        .env_remove("SLIMEVR_LOG_LEVEL")
        .args([
            "listen",
            "--bind",
            "127.0.0.1:0",
            "--no-discovery",
            "--run-for",
            "1",
            "--log-level",
            "info",
            "--pose-ms",
            "4",
            "--timing-window-ms",
            "200",
            "--pose-config",
        ])
        .arg(pose)
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let reports: Vec<Value> = stdout
        .lines()
        .chain(stderr.lines())
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|value| value["type"] == "runtime_timing")
        .collect();
    assert!(!reports.is_empty());
    let mut ticks = 0;
    let mut samples = 0;
    for report in &reports {
        assert_eq!(report["tick_kind"], "pose");
        assert_eq!(report["expected_tick_ms"], 4.);
        assert_eq!(report["api_live_snapshots"], 0);
        assert!(report["api_live_snapshot_ms"].is_null());
        assert!(report["window_ms"].as_f64().unwrap() > 0.);
        ticks += report["ticks"].as_u64().unwrap();
        let count = report["interval_samples"].as_u64().unwrap();
        samples += count;
        for key in ["tick_work_ms", "tick_jitter_ms"] {
            if key == "tick_jitter_ms" && count == 0 {
                assert!(report[key].is_null());
                continue;
            }
            let p: Vec<_> = ["p50", "p95", "p99", "p999", "max"]
                .into_iter()
                .map(|p| report[key][p].as_f64().unwrap())
                .collect();
            assert!(p.windows(2).all(|pair| pair[0] <= pair[1]));
        }
        let stalls: Vec<_> = ["gt_2ms", "gt_5ms", "gt_10ms", "gt_50ms", "gt_100ms"]
            .into_iter()
            .map(|key| report["runtime_stall"][key].as_u64().unwrap())
            .collect();
        assert!(stalls.windows(2).all(|pair| pair[0] >= pair[1]));
        assert!(stalls[0] <= count);
    }
    // Every observed interval is represented once, including across window boundaries.
    assert_eq!(samples + 1, ticks);
    assert!(!stdout.contains("\"type\":\"runtime_stall\""));
}
