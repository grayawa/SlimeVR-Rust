//! Discord IPC uses the same application ID and activity fields as the original SlimeVR desktop host.
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::watch,
};
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub enable: bool,
    pub activity: Option<String>,
    pub icon_text: Option<String>,
}
pub struct Presence {
    sender: watch::Sender<Settings>,
    stop: watch::Sender<bool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
type Ipc = Box<dyn Stream>;
impl Presence {
    pub fn start() -> Self {
        let (sender, receiver) = watch::channel(Settings::default());
        let (stop, mut stopping) = watch::channel(false);
        let worker = std::thread::spawn(move || {
            if let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                runtime.block_on(async move {
                    tokio::select! {_=run(receiver)=>(),_=stopping.changed()=>()}
                });
            }
        });
        Self {
            sender,
            stop,
            worker: Some(worker),
        }
    }
    pub fn set(&self, settings: Settings) {
        if self.sender.borrow().enable != settings.enable
            || self.sender.borrow().activity != settings.activity
            || self.sender.borrow().icon_text != settings.icon_text
        {
            self.sender.send_replace(settings);
        }
    }
}
impl Drop for Presence {
    fn drop(&mut self) {
        self.sender.send_replace(Settings::default());
        let _ = self.stop.send(true);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
async fn connect() -> std::io::Result<Ipc> {
    #[cfg(windows)]
    {
        for index in 0..10 {
            if let Ok(pipe) = tokio::net::windows::named_pipe::ClientOptions::new()
                .open(format!(r"\\.\pipe\discord-ipc-{index}"))
            {
                return Ok(Box::new(pipe));
            }
        }
    }
    #[cfg(unix)]
    {
        let mut roots = vec![std::env::temp_dir()];
        for key in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
            if let Some(path) = std::env::var_os(key) {
                roots.push(path.into());
            }
        }
        let mut candidates = vec![];
        for root in roots {
            for index in 0..10 {
                for suffix in ["", "app/com.discordapp.Discord", "snap.discord"] {
                    candidates.push(root.join(suffix).join(format!("discord-ipc-{index}")));
                }
            }
        }
        for path in candidates {
            if let Ok(Ok(stream)) = tokio::time::timeout(
                std::time::Duration::from_millis(200),
                tokio::net::UnixStream::connect(path),
            )
            .await
            {
                return Ok(Box::new(stream));
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "Discord IPC is unavailable",
    ))
}
async fn send<W: AsyncWrite + Unpin>(
    writer: &mut W,
    opcode: u32,
    value: &Value,
) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(value)?;
    let mut frame = vec![];
    frame.extend(opcode.to_le_bytes());
    frame.extend((bytes.len() as u32).to_le_bytes());
    frame.extend(bytes);
    tokio::time::timeout(std::time::Duration::from_secs(2), writer.write_all(&frame)).await??;
    Ok(())
}
async fn receive<R: AsyncRead + Unpin>(reader: &mut R) -> std::io::Result<(u32, Value)> {
    let opcode = reader.read_u32_le().await?;
    let len = reader.read_u32_le().await? as usize;
    if len > 65536 {
        return Err(std::io::Error::other("Discord frame exceeds 64 KiB"));
    }
    let mut bytes = vec![0; len];
    reader.read_exact(&mut bytes).await?;
    Ok((opcode, serde_json::from_slice(&bytes)?))
}
fn activity(s: &Settings, start: u64, nonce: u64) -> Value {
    json!({"cmd":"SET_ACTIVITY","nonce":nonce.to_string(),"args":{"pid":std::process::id(),"activity":if s.enable{json!({"state":s.activity,"assets":{"large_image":"icon","large_text":s.icon_text},"timestamps":{"start":start}})}else{Value::Null}}})
}
async fn session(
    mut stream: Ipc,
    settings: &mut watch::Receiver<Settings>,
    start: u64,
) -> std::io::Result<()> {
    send(
        &mut stream,
        0,
        &json!({"v":1,"client_id":"1237970689009647639"}),
    )
    .await?;
    let (opcode, ready) =
        tokio::time::timeout(std::time::Duration::from_secs(3), receive(&mut stream)).await??;
    if opcode != 1 || ready["evt"] != "READY" {
        return Err(std::io::Error::other("Discord handshake failed"));
    }
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut nonce = 1;
    let initial = settings.borrow_and_update().clone();
    send(&mut writer, 1, &activity(&initial, start, nonce)).await?;
    if !initial.enable {
        return Ok(());
    }
    loop {
        let next = receive(&mut reader);
        tokio::pin!(next);
        loop {
            tokio::select! {frame=&mut next=>{let(opcode,value)=frame?;if opcode==3{send(&mut writer,4,&value).await?;}else if opcode==2{return Err(std::io::Error::other("Discord closed connection"));}break;},changed=settings.changed()=>{if changed.is_err(){return Ok(());}let s=settings.borrow_and_update().clone();nonce+=1;send(&mut writer,1,&activity(&s,start,nonce)).await?;if !s.enable{return Ok(());}}}
        }
    }
}
async fn run(mut settings: watch::Receiver<Settings>) {
    let start = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    loop {
        while !settings.borrow().enable {
            if settings.changed().await.is_err() {
                return;
            }
        }
        if let Ok(stream) = connect().await {
            let _ = session(stream, &mut settings, start).await;
        }
        tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(3))=>{},changed=settings.changed()=>{if changed.is_err(){return;}}}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn framing_handshake_activity_and_ping_use_original_fields() {
        let (mut a, mut b) = tokio::io::duplex(8192);
        let expected = json!({"evt":"READY"});
        send(&mut a, 1, &expected).await.unwrap();
        assert_eq!(receive(&mut b).await.unwrap(), (1, expected));
        let s = Settings {
            enable: true,
            activity: Some("6 trackers".into()),
            icon_text: Some("0.1".into()),
        };
        let value = activity(&s, 123, 4);
        assert_eq!(value["args"]["activity"]["assets"]["large_image"], "icon");
        assert_eq!(value["args"]["activity"]["timestamps"]["start"], 123);
        assert!(activity(&Settings::default(), 0, 0)["args"]["activity"].is_null());
    }
}

#[cfg(test)]
mod session_tests {
    use super::*;
    #[tokio::test]
    async fn live_session_handshake_updates_ping_and_disable() {
        let (client, mut discord) = tokio::io::duplex(8192);
        let (tx, mut settings) = watch::channel(Settings {
            enable: true,
            activity: Some("Starting".into()),
            icon_text: None,
        });
        let task = tokio::spawn(async move { session(Box::new(client), &mut settings, 123).await });
        let (opcode, hello) = receive(&mut discord).await.unwrap();
        assert_eq!(opcode, 0);
        assert_eq!(hello["client_id"], "1237970689009647639");
        send(&mut discord, 1, &json!({"evt":"READY"}))
            .await
            .unwrap();
        assert_eq!(
            receive(&mut discord).await.unwrap().1["args"]["activity"]["state"],
            "Starting"
        );
        send(&mut discord, 3, &json!({"ping":7})).await.unwrap();
        assert_eq!(receive(&mut discord).await.unwrap(), (4, json!({"ping":7})));
        tx.send_replace(Settings {
            enable: true,
            activity: Some("6 trackers".into()),
            icon_text: None,
        });
        assert_eq!(
            receive(&mut discord).await.unwrap().1["args"]["activity"]["state"],
            "6 trackers"
        );
        tx.send_replace(Settings::default());
        assert!(receive(&mut discord).await.unwrap().1["args"]["activity"].is_null());
        task.await.unwrap().unwrap();
    }
}
