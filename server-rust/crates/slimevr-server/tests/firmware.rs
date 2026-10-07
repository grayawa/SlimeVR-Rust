use md5::{Digest, Md5};
use slimevr_server::firmware::{ota, verify_digest};
use solarxr_protocol::rpc::FirmwareUpdateStatus as S;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UdpSocket},
};
#[test]
fn checksum_formats() {
    assert!(verify_digest(
        b"abc",
        "sha-256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    )
    .unwrap());
    assert!(!verify_digest(b"bad", "MD5:900150983cd24fb0d6963f7d28e17f72").unwrap());
    assert!(verify_digest(b"abc", "MD5:900150983CD24FB0D6963F7D28E17F72").unwrap());
    for digest in [
        "none",
        "sha256:bad",
        "unknown:00000000000000000000000000000000",
    ] {
        assert!(verify_digest(b"abc", digest).is_err());
    }
}
#[tokio::test]
async fn ota_challenge_chunk_acks_and_result() {
    for auth in [false, true] {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = socket.local_addr().unwrap().port();
        let bytes = vec![0x67; 4097];
        let expected = bytes.clone();
        let device = tokio::spawn(async move {
            let mut buf = [0; 512];
            let (n, from) = socket.recv_from(&mut buf).await.unwrap();
            let text = std::str::from_utf8(&buf[..n]).unwrap();
            let fields = text.split_whitespace().collect::<Vec<_>>();
            assert_eq!(fields[0], "0");
            assert_eq!(fields[2], "4097");
            assert_eq!(fields[3], format!("{:x}", Md5::digest(&expected)));
            let tcp_port = fields[1].parse::<u16>().unwrap();
            if auth {
                socket.send_to(b"AUTH testnonce", from).await.unwrap();
                let (n, source) = socket.recv_from(&mut buf).await.unwrap();
                assert_eq!(source, from);
                let text = std::str::from_utf8(&buf[..n]).unwrap();
                let fields = text.split_whitespace().collect::<Vec<_>>();
                assert_eq!(fields[0], "200");
                let password = format!("{:x}", Md5::digest(b"SlimeVR-OTA"));
                assert_eq!(
                    fields[2],
                    format!(
                        "{:x}",
                        Md5::digest(format!("{password}:testnonce:{}", fields[1]))
                    )
                );
            }
            socket.send_to(b"OK", from).await.unwrap();
            let mut tcp = TcpStream::connect(("127.0.0.1", tcp_port)).await.unwrap();
            let mut data = vec![0; expected.len()];
            for chunk in data.chunks_mut(2048) {
                tcp.read_exact(chunk).await.unwrap();
                tcp.write_all(&(chunk.len() as u32).to_be_bytes())
                    .await
                    .unwrap();
            }
            assert_eq!(data, expected);
            tcp.write_all(b"OK").await.unwrap();
            tcp.shutdown().await.unwrap();
        });
        let mut updates = Vec::new();
        ota(
            "127.0.0.1".parse().unwrap(),
            port,
            &bytes,
            |status, progress| {
                updates.push((status, progress));
                async { Ok(()) }
            },
        )
        .await
        .unwrap();
        device.await.unwrap();
        assert_eq!(updates[0], (S::AUTHENTICATING, 0));
        assert_eq!(updates.last(), Some(&(S::UPLOADING, 100)));
        assert_eq!(updates.len(), 4);
    }
}
#[tokio::test]
async fn ota_rejects_bad_auth() {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let port = socket.local_addr().unwrap().port();
    let device = tokio::spawn(async move {
        let mut buf = [0; 512];
        let (_, peer) = socket.recv_from(&mut buf).await.unwrap();
        socket.send_to(b"BAD", peer).await.unwrap();
    });
    let error = ota("127.0.0.1".parse().unwrap(), port, b"image", |_, _| async {
        Ok(())
    })
    .await
    .unwrap_err();
    assert_eq!(error.0, S::ERROR_AUTHENTICATION_FAILED);
    device.await.unwrap();
}
