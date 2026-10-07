use slimevr_server::serial::{command, redact, wifi};
#[test]
fn wifi_and_command_wire_and_redaction() {
    assert_eq!(command("GET INFO").unwrap(), b"GET INFO\n");
    for text in ["", "REBOOT\nFRST", "bad\0"] {
        assert!(command(text).is_err());
    }
    let (bytes, log) = wifi("network", "secret").unwrap();
    assert_eq!(bytes, b"SET WIFI \"network\" \"secret\"\n");
    assert!(!log.contains("secret"));
    assert_eq!(redact("password secret\n", "secret"), "password ******\n");
    for (ssid, pwd) in [("", ""), ("ssid\n", "secret"), ("ssid", "pass\"word")] {
        assert!(wifi(ssid, pwd).is_err());
    }
}
#[cfg(unix)]
#[tokio::test]
async fn real_pty_commands_logs_reservation_and_shutdown() {
    use serialport::SerialPort;
    use slimevr_server::{
        api::Request,
        serial::{Action, Controller, Event},
    };
    use std::{
        io::{Read, Write},
        time::Duration,
    };
    let (mut master, mut slave) = serialport::TTYPort::pair().unwrap();
    master.set_timeout(Duration::from_secs(2)).unwrap();
    slave.set_exclusive(false).unwrap();
    let path = slave.name().unwrap();
    drop(slave);
    let (tx, mut rx) = tokio::sync::mpsc::channel(128);
    let controller = Controller::start(tx, vec![path.clone()]).unwrap();
    async fn next(rx: &mut tokio::sync::mpsc::Receiver<Request>) -> Event {
        match tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Request::Serial(event) => event,
            _ => panic!("unexpected request"),
        }
    }
    assert!(matches!(next(&mut rx).await,Event::Ports(ports) if ports.iter().any(|p|p.path==path)));
    controller
        .send(Action::Open {
            port: Some(path),
            auto: false,
            include_hid: true,
        })
        .unwrap();
    assert!(matches!(next(&mut rx).await, Event::Connected(_)));
    controller
        .send(Action::Wifi {
            ssid: "ssid".into(),
            password: "secret".into(),
        })
        .unwrap();
    assert!(matches!(next(&mut rx).await,Event::Log{log,server:true} if !log.contains("secret")));
    let mut data = vec![0; b"SET WIFI \"ssid\" \"secret\"\n".len()];
    master.read_exact(&mut data).unwrap();
    assert_eq!(data, b"SET WIFI \"ssid\" \"secret\"\n");
    master
        .write_all(b"echo secret\nmac: aa:bb:cc:dd:ee:ff, \n")
        .unwrap();
    assert!(matches!(next(&mut rx).await,Event::Log{log,server:false} if log=="echo ******\n"));
    assert!(matches!(next(&mut rx).await,Event::Log{log,server:false} if log.starts_with("mac:")));
    controller.send(Action::Suspend { token: 41 }).unwrap();
    assert!(matches!(next(&mut rx).await, Event::Suspended(41)));
    controller.send(Action::Command("FRST".into())).unwrap();
    assert!(matches!(next(&mut rx).await, Event::Error(_)));
    controller.send(Action::Resume).unwrap();
    controller.send(Action::Close).unwrap();
    assert!(matches!(next(&mut rx).await, Event::Closed));
    let start = std::time::Instant::now();
    drop(controller);
    assert!(start.elapsed() < Duration::from_secs(1));
}
