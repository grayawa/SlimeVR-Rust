//! Run against an explicitly selected backend; all data and ports are isolated.
use slimevr_gpui::{
    client::{Client, Snapshot},
    log_level::LogLevel,
    protocol::{Command as Rpc, ResetKind},
};
use solarxr_protocol::datatypes::BodyPart;
use std::{
    io::{BufRead, BufReader},
    net::UdpSocket,
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(3);
        while matches!(self.0.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn wait(client: &Client, predicate: impl Fn(&Snapshot) -> bool) -> Snapshot {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let s = client.snapshot();
        if predicate(&s) {
            return s;
        }
        assert!(
            Instant::now() < deadline,
            "Backend state did not arrive: connection={:?}, error={:?}, records={:?}",
            s.connection,
            s.last_error,
            s.rpc.keys().collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
fn packet(id: u32, seq: u64, payload: &[u8]) -> Vec<u8> {
    let mut out = id.to_be_bytes().to_vec();
    out.extend(seq.to_be_bytes());
    out.extend(payload);
    out
}
fn handshake(device: u8) -> Vec<u8> {
    let mut payload = Vec::new();
    for n in [9u32, 13, 1, 0, 0, 0, 22] {
        payload.extend(n.to_be_bytes());
    }
    payload.extend([5, b'g', b'o', b'o', b'd', 0, 2, 0, 0, 0, 0, device]);
    packet(3, 0, &payload)
}

#[test]
#[ignore = "Set SLIMEVR_TEST_BACKEND to an existing slimevr-server executable"]
fn six_udp_trackers_assignment_yaml_resets_and_pause_use_real_backend() {
    let executable =
        std::env::var_os("SLIMEVR_TEST_BACKEND").expect("SLIMEVR_TEST_BACKEND must be set");
    let temp = tempfile::tempdir().unwrap();
    let config = temp.path().join("vrconfig.yml");
    let mut server = Server(
        Command::new(executable)
            .args([
                "listen",
                "--bind",
                "127.0.0.1:0",
                "--api-bind",
                "127.0.0.1:0",
                "--accept-new-devices",
                "--no-steamvr",
                "--no-bindings-provider",
                "--shutdown-on-stdin-eof",
                "--run-for",
                "45000",
            ])
            .arg("--config")
            .arg(&config)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let output = server.0.stdout.take().unwrap();
    let mut reader = BufReader::new(output);
    let listening = loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "Backend exited before listening"
        );
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line)
            && value["type"] == "listening"
        {
            break value;
        }
    };
    // Drain diagnostics continuously to keep the child's output pipe writable.
    let drain = std::thread::spawn(move || {
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            line.clear();
        }
    });
    let client = Client::connect(
        format!("ws://{}", listening["api_bind"].as_str().unwrap()),
        LogLevel::Error,
    )
    .unwrap();
    let live_audio = Arc::new(Mutex::new((
        slimevr_gpui::sounds::Sequencer::default(),
        Vec::new(),
    )));
    let observed_audio = live_audio.clone();
    client.on_reset(move |session, reset| {
        let mut audio = observed_audio.lock().unwrap();
        let cues = audio
            .0
            .reset(session, reset.tx, reset.kind, reset.done, reset.progress_ms);
        audio.1.extend(cues);
    });

    wait(&client, |s| {
        s.feed.is_some() && s.settings.is_some() && s.paused == Some(false) && s.vrchat.is_some()
    });
    let target = listening["bind"].as_str().unwrap().to_owned();
    let sockets: Vec<_> = (1..=6)
        .map(|device| {
            let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
            socket.send_to(&handshake(device), &target).unwrap();
            socket
                .send_to(&packet(15, 1, &[0, 1, 13]), &target)
                .unwrap();
            socket
        })
        .collect();
    let stopping = Arc::new(AtomicBool::new(false));
    let stop = stopping.clone();
    let samples = std::thread::spawn(move || {
        let mut seq = 2;
        while !stop.load(Ordering::Relaxed) {
            let mut rotation = [0; 19];
            rotation[1] = 1;
            rotation[14..18].copy_from_slice(&1.0f32.to_be_bytes());
            rotation[18] = 3;
            for socket in &sockets {
                let _ = socket.send_to(&packet(17, seq, &rotation), &target);
            }
            seq += 1;
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    let six = wait(&client, |s| {
        s.feed
            .as_ref()
            .is_some_and(|f| f.trackers.iter().filter(|t| !t.computed).count() == 6)
    });
    assert!(!six.feed.as_ref().unwrap().bones.is_empty());
    let body_parts = [
        BodyPart::CHEST,
        BodyPart::HIP,
        BodyPart::LEFT_UPPER_LEG,
        BodyPart::RIGHT_UPPER_LEG,
        BodyPart::LEFT_LOWER_LEG,
        BodyPart::RIGHT_LOWER_LEG,
    ];
    for (tracker, body) in six
        .feed
        .unwrap()
        .trackers
        .iter()
        .filter(|t| !t.computed)
        .zip(body_parts)
    {
        client
            .send(Rpc::Assign {
                key: tracker.key,
                body: body.0,
            })
            .unwrap();
        wait(&client, |s| {
            s.pending.is_none()
                && s.feed.as_ref().is_some_and(|f| {
                    f.trackers
                        .iter()
                        .any(|t| t.key == tracker.key && t.body == body.0)
                })
        });
    }
    let saved = std::fs::read_to_string(&config).unwrap();
    assert!(
        saved.contains("LEFT_UPPER_LEG"),
        "Assignment was not saved to original YAML: {saved}"
    );
    for kind in [ResetKind::Full, ResetKind::Yaw, ResetKind::Mounting] {
        let tx = client.send(Rpc::Reset(kind)).unwrap();
        wait(&client, |s| {
            s.pending.is_none() && s.reset.as_ref().is_some_and(|r| r.tx == tx && r.done)
        });
        wait(&client, |s| {
            s.feed.as_ref().is_some_and(|f| f.can_yaw && f.can_mount)
        });
    }
    for paused in [true, false] {
        client.send(Rpc::Pause(paused)).unwrap();
        wait(&client, |s| s.pending.is_none() && s.paused == Some(paused));
    }
    let revision = client.snapshot().revision;
    for _ in 0..12 {
        client.send(Rpc::ReadSettings).unwrap_or_default();
    }
    let read_tx = client.send(Rpc::ReadSettings).unwrap();
    wait(&client, |s| {
        s.revision > revision
            && s.rpc
                .get("SettingsResponse")
                .is_some_and(|r| r.tx == read_tx)
    });
    assert!(client.snapshot().diagnostics.len() <= 64);

    // All settings use the same upstream builders, original YAML and readback.
    let mut filter = client.snapshot().rpc["SettingsResponse"].value["filtering"].clone();
    filter["amount"] = serde_json::json!(0.65);
    client
        .batch(vec![
            (
                "ChangeSettingsRequest".into(),
                serde_json::json!({"filtering":filter}),
            ),
            ("SettingsRequest".into(), serde_json::json!({})),
        ])
        .unwrap();
    wait(&client, |s| {
        s.rpc.get("SettingsResponse").is_some_and(|r| {
            (r.value["filtering"]["amount"].as_f64().unwrap_or_default() - 0.65).abs() < 1e-5
        })
    });
    let skeleton = client.snapshot().rpc["SkeletonConfigResponse"]
        .value
        .clone();
    let exported = slimevr_gpui::proportions::export(&skeleton);
    client
        .batch(slimevr_gpui::proportions::import(&exported).unwrap())
        .unwrap();
    let bvh = temp.path().join("native.bvh");
    client
        .rpc(
            "RecordBVHRequest",
            serde_json::json!({"path":bvh.to_string_lossy(),"stop":false}),
        )
        .unwrap();
    wait(&client, |s| {
        s.rpc
            .get("RecordBVHStatus")
            .is_some_and(|r| r.value["recording"] == true)
    });
    std::thread::sleep(Duration::from_millis(100));
    client
        .rpc("RecordBVHRequest", serde_json::json!({"stop":true}))
        .unwrap();
    wait(&client, |s| {
        s.rpc
            .get("RecordBVHStatus")
            .is_some_and(|r| r.value["recording"] == false)
    });
    let recording = std::fs::read_to_string(&bvh).unwrap();
    assert!(
        recording.contains("HIERARCHY")
            && recording.contains("MOTION")
            && recording.contains("Frames:")
    );
    let hmd_url = format!("ws://{}", listening["api_bind"].as_str().unwrap());
    let hmd_stop = stopping.clone();
    let hmd = std::thread::spawn(move || {
        let (mut socket, _) = tokio_tungstenite::tungstenite::connect(hmd_url).unwrap();
        while !hmd_stop.load(Ordering::Relaxed) {
            let message = serde_json::json!({"type":"pos","tracker_id":0,"x":0.0,"y":1.4,"z":0.0,"qx":0.0,"qy":0.0,"qz":0.0,"qw":1.0});
            socket
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    message.to_string().into(),
                ))
                .unwrap();
            std::thread::sleep(Duration::from_millis(30));
        }
    });
    std::thread::sleep(Duration::from_millis(100));
    let mut auto = client.snapshot().rpc["SettingsResponse"].value["auto_bone_settings"].clone();
    auto["sample_count"] = serde_json::json!(12);
    auto["sample_rate_ms"] = serde_json::json!(20);
    client
        .batch(vec![
            (
                "ChangeSettingsRequest".into(),
                serde_json::json!({"auto_bone_settings":auto}),
            ),
            ("SettingsRequest".into(), serde_json::json!({})),
        ])
        .unwrap();
    wait(&client, |s| {
        s.rpc
            .get("SettingsResponse")
            .is_some_and(|r| r.value["auto_bone_settings"]["sample_count"] == 12)
    });
    client
        .rpc(
            "AutoBoneProcessRequest",
            serde_json::json!({"process_type":1}),
        )
        .unwrap();
    wait(&client, |s| {
        s.rpc.get("AutoBoneProcessStatusResponse").is_some_and(|r| {
            r.value["process_type"] == 1
                && r.value["completed"] == true
                && r.value["success"] == true
        })
    });

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let finished = live_audio
            .lock()
            .unwrap()
            .1
            .iter()
            .filter(|cue| matches!(cue, slimevr_gpui::sounds::Cue::Finished(_)))
            .count();
        if finished == 3 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Live reset feedback did not reach playback independently of the undrained UI events"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let events = client.drain_events();
    let mut audio = slimevr_gpui::sounds::Sequencer::default();
    let mut finishes = 0;
    for event in &events {
        if event.name == "ResetResponse" {
            finishes += audio
                .reset(
                    event.session,
                    event.tx,
                    event.value["reset_type"].as_u64().unwrap() as u8,
                    event.value["status"] == 1,
                    event.value["progress"].as_i64().unwrap() as i32,
                )
                .iter()
                .filter(|cue| matches!(cue, slimevr_gpui::sounds::Cue::Finished(_)))
                .count();
        }
    }
    assert_eq!(
        finishes, 3,
        "Each real backend reset must finish audibly exactly once"
    );
    assert!(
        events
            .iter()
            .any(|e| e.name == "AutoBoneProcessStatusResponse" && e.value["completed"] == true),
        "Completion must survive pose updates"
    );
    assert!(
        client.snapshot().last_error.is_none(),
        "Startup/read requests must not produce backend errors: {:?}",
        client.snapshot().last_error
    );
    stopping.store(true, Ordering::Relaxed);
    samples.join().unwrap();
    hmd.join().unwrap();
    drop(client);
    drop(server);
    drain.join().unwrap();
}
