use slimevr_server::steamvr::manager::Manager;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
fn manager(url: String) -> Manager {
    Manager::new(None, None, Some(url), false, false).unwrap()
}
#[tokio::test]
async fn original_http_routes_referer_and_json_enable_the_driver() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let expected = url.clone();
    let worker = tokio::spawn(async move {
        for (route, body) in [
            (
                "POST /drivers/unblock",
                Some(serde_json::json!({"driver":"slimevr"})),
            ),
            (
                "POST /drivers/setenable",
                Some(serde_json::json!({"driver":"slimevr","enable":true})),
            ),
            ("GET /drivers/list.json", None),
        ] {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 2048];
            loop {
                let n = socket.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]).to_lowercase();
                    let length = header
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|l| l.parse::<usize>().unwrap())
                        .unwrap_or(0);
                    if bytes.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(bytes).unwrap();
            assert!(request.starts_with(route));
            assert!(request
                .to_lowercase()
                .contains(&format!("referer: {expected}/dashboard/index.html")));
            if let Some(body) = body {
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(
                        request.split("\r\n\r\n").nth(1).unwrap()
                    )
                    .unwrap(),
                    body
                );
            }
            let reply = r#"{"jsonid":"vr_driver_list","drivers":[{"enabled":true,"blocked_by_safe_mode":false,"manifest":{"name":"slimevr"}}]}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        reply.len(),
                        reply
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        }
    });
    let status = manager(url).enable().await.unwrap();
    assert!(status.known && status.installed && status.enabled && !status.blocked);
    worker.await.unwrap();
}
#[tokio::test]
async fn malformed_driver_lists_and_failed_http_are_reported() {
    for (code, body) in [
        ("200 OK", r#"{"jsonid":"wrong","drivers":[]}"#),
        ("503 Unavailable", "failure"),
        ("200 OK", r#"{"jsonid":"vr_driver_list","drivers":[]}"#),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 2048];
            assert!(socket.read(&mut buffer).await.unwrap() > 0);
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let status = manager(url).status().await;
        if body.contains("vr_driver_list") {
            assert!(!status.unwrap().installed);
        } else {
            assert!(status.is_err());
        }
        worker.await.unwrap();
    }
    assert!(Manager::new(None, None, Some("https://example.com".into()), false, false).is_err());
}
#[cfg(target_os = "linux")]
#[tokio::test]
async fn vrpathreg_preserves_existing_and_manual_installs_and_registers_valid_bundle() {
    use slimevr_server::steamvr::manager::RegistrationOutcome;
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let runtime = dir.path().join("SteamVR");
    let source = dir.path().join("bundle with spaces");
    std::fs::create_dir_all(runtime.join("bin")).unwrap();
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(
        source.join("driver.vrdrivermanifest"),
        r#"{"name":"slimevr"}"#,
    )
    .unwrap();
    let script = runtime.join("bin/vrpathreg.sh");
    std::fs::write(&script,"#!/bin/sh\ncd \"$(dirname \"$0\")\"\nif [ \"$1\" = finddriver ]; then exit \"$(cat code)\"; fi\nprintf '%s\\n' \"$1\" \"$2\" > called\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let m = Manager::new(
        Some(source.clone()),
        Some(runtime.clone()),
        None,
        true,
        false,
    )
    .unwrap();
    std::fs::write(runtime.join("bin/code"), "0").unwrap();
    assert_eq!(m.register().await.unwrap(), RegistrationOutcome::Existing);
    assert!(!runtime.join("bin/called").exists());
    std::fs::write(runtime.join("bin/code"), "2").unwrap();
    assert!(m.register().await.unwrap_err().contains("exited with 2"));
    std::fs::write(runtime.join("bin/code"), "1").unwrap();
    std::fs::create_dir_all(runtime.join("drivers/slimevr")).unwrap();
    assert_eq!(
        m.register().await.unwrap(),
        RegistrationOutcome::ExistingManualDriver
    );
    assert!(!runtime.join("bin/called").exists());
    std::fs::remove_dir(runtime.join("drivers/slimevr")).unwrap();
    assert_eq!(m.register().await.unwrap(), RegistrationOutcome::Installed);
    assert_eq!(
        std::fs::read_to_string(runtime.join("bin/called")).unwrap(),
        format!("adddriver\n{}\n", source.canonicalize().unwrap().display())
    );
}
