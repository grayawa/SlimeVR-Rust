//! Driver-compatible local IPC, with bounded queues and session-scoped output.
use super::{messages::ProtobufMessage, output_stats::WriteBatch, Batch, Event, OutputStats};
use prost::Message;
use std::{
    io,
    path::Path,
    sync::{atomic::Ordering, Arc},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
    time::{timeout, Duration},
};

pub const MAX_FRAME: usize = 1024;

pub fn encode(message: &ProtobufMessage) -> io::Result<Vec<u8>> {
    let size = message.encoded_len() + 4;
    if size > MAX_FRAME {
        return Err(io::Error::other("SteamVR frame exceeds 1024 bytes"));
    }
    let mut bytes = Vec::with_capacity(size);
    bytes.extend_from_slice(&(size as u32).to_le_bytes());
    message.encode(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes)
}

pub async fn read<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<ProtobufMessage> {
    ProtobufMessage::decode(read_bytes(reader, MAX_FRAME).await?.as_slice())
        .map_err(io::Error::other)
}
pub async fn read_bytes<R: AsyncRead + Unpin>(reader: &mut R, limit: usize) -> io::Result<Vec<u8>> {
    let size = reader.read_u32_le().await? as usize;
    if !(4..=limit).contains(&size) {
        return Err(io::Error::other("invalid SteamVR frame length"));
    }
    let mut bytes = vec![0; size - 4];
    reader.read_exact(&mut bytes).await?;
    Ok(bytes)
}

async fn rpc_write<W: AsyncWrite + Unpin>(writer: &mut W, bytes: &[u8]) -> io::Result<()> {
    let mut framed = Vec::with_capacity(bytes.len() + 4);
    framed.extend_from_slice(&((bytes.len() + 4) as u32).to_le_bytes());
    framed.extend_from_slice(bytes);
    timeout(Duration::from_millis(250), writer.write_all(&framed)).await??;
    Ok(())
}
async fn rpc_connection<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    commands: &mpsc::Sender<crate::api::Request>,
    publisher: tokio::sync::broadcast::Sender<crate::api::Wire>,
    hub: crate::api::pubsub::Hub,
) -> io::Result<()> {
    use crate::api::{Request, Wire};
    use solarxr_protocol::{self as sx, flatbuffers as fb};
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut events = publisher.subscribe();
    let client = hub.client_id();
    let mut subscriptions = std::collections::BTreeSet::new();
    loop {
        let incoming = read_bytes(&mut reader, 8 * 1024 * 1024);
        tokio::pin!(incoming);
        loop {
            tokio::select! {
                bytes=&mut incoming=>{let bytes=bytes?;let bundle=fb::root::<sx::MessageBundle>(&bytes).map_err(io::Error::other)?;
                    if let Some(headers)=bundle.pub_sub_msgs(){let dispatched=hub.broker.lock().map_err(|_|io::Error::other("topic broker unavailable"))?.dispatch(headers,&mut subscriptions).map_err(io::Error::other)?;for bytes in dispatched.replies{rpc_write(&mut writer,&bytes).await?;}for(handle,bytes)in dispatched.messages{let _=publisher.send(Wire::PubSub{origin:client,handle,bytes});}}
                    if bundle.rpc_msgs().is_some(){let(reply,response)=tokio::sync::oneshot::channel();commands.send(Request::Client{data:tokio_tungstenite::tungstenite::Message::Binary(bytes.into()),reply}).await.map_err(io::Error::other)?;for response in timeout(Duration::from_secs(5),response).await?.map_err(io::Error::other)?{if let Wire::Binary(bytes)=response{rpc_write(&mut writer,&bytes).await?;}}}
                    break;
                },event=events.recv()=>{match event{Ok(Wire::PubSub{origin,handle,bytes})if origin!=client&&subscriptions.contains(&handle)=>rpc_write(&mut writer,&bytes).await?,Ok(Wire::Binary(bytes))=>rpc_write(&mut writer,&bytes).await?,Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>return Err(io::Error::other("native RPC client cannot keep up")),Err(tokio::sync::broadcast::error::RecvError::Closed)=>return Ok(()),_=>{}}}
            }
        }
    }
}

async fn connection<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    session: u64,
    events: &mpsc::Sender<Event>,
    output: &mut mpsc::Receiver<Batch>,
    stats: &OutputStats,
) -> io::Result<()> {
    events
        .send(Event::Connected(session))
        .await
        .map_err(io::Error::other)?;
    let (mut reader, mut writer) = tokio::io::split(stream);
    let receive = async {
        loop {
            // Keep partial reads in this future; never cancel/restart a frame on a tick.
            let message = read(&mut reader).await?;
            events
                .send(Event::Message(session, message))
                .await
                .map_err(io::Error::other)?;
        }
        #[allow(unreachable_code)]
        Ok::<(), io::Error>(())
    };
    let send = async {
        while let Some(batch) = output.recv().await {
            if batch.session != session {
                stats.stale_batches.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            write_batch(&mut writer, batch, stats).await?;
        }
        Ok::<(), io::Error>(())
    };
    let result = tokio::select! {
        result = receive => result,
        result = send => result,
    };
    events
        .send(Event::Disconnected(session))
        .await
        .map_err(io::Error::other)?;
    result
}

async fn write_batch<W: AsyncWrite + Unpin>(
    writer: &mut W,
    batch: Batch,
    stats: &OutputStats,
) -> io::Result<()> {
    let measured = WriteBatch::new(stats);
    let result = async {
        for message in batch.messages {
            let bytes = encode(&message)?;
            timeout(Duration::from_millis(250), writer.write_all(&bytes)).await??;
        }
        Ok(())
    }
    .await;
    measured.finish(result.is_ok());
    result
}

#[cfg(unix)]
pub struct Listener {
    listener: tokio::net::UnixListener,
    path: std::path::PathBuf,
    identity: (u64, u64),
}

#[cfg(unix)]
impl Listener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            if !metadata.file_type().is_socket() {
                return Err(io::Error::other("SteamVR endpoint is not a socket"));
            }
            match std::os::unix::net::UnixStream::connect(path) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "SteamVR socket is in use",
                    ))
                }
                Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(path)?
                }
                Err(e) => return Err(e),
            }
        }
        let listener = tokio::net::UnixListener::bind(path)?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        let metadata = std::fs::symlink_metadata(path)?;
        Ok(Self {
            listener,
            path: path.into(),
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    pub async fn run(self, events: mpsc::Sender<Event>, output: mpsc::Receiver<Batch>) {
        self.run_with_stats(events, output, Arc::new(OutputStats::default()))
            .await;
    }
    pub async fn run_with_stats(
        self,
        events: mpsc::Sender<Event>,
        mut output: mpsc::Receiver<Batch>,
        stats: Arc<OutputStats>,
    ) {
        let mut session = 0;
        loop {
            let stream = match self.listener.accept().await {
                Ok((stream, _)) => stream,
                Err(_) => break,
            };
            session += 1;
            let _ = connection(stream, session, &events, &mut output, &stats).await;
            if events.is_closed() {
                break;
            }
        }
    }
    pub async fn run_rpc(
        self,
        commands: mpsc::Sender<crate::api::Request>,
        publisher: tokio::sync::broadcast::Sender<crate::api::Wire>,
        hub: crate::api::pubsub::Hub,
    ) {
        let mut clients = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = self.listener.accept() => {
                    let Ok((stream,_)) = accepted else { break; };
                    if clients.len() >= 8 { continue; }
                    let commands = commands.clone();let publisher=publisher.clone();let hub=hub.clone();
                    clients.spawn(async move { let _ = rpc_connection(stream,&commands,publisher,hub).await; });
                },
                _ = clients.join_next(), if !clients.is_empty() => {},
            }
        }
    }
}
#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        use std::os::unix::fs::MetadataExt;
        if std::fs::symlink_metadata(&self.path).is_ok_and(|m| (m.dev(), m.ino()) == self.identity)
        {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
pub struct Listener {
    server: tokio::net::windows::named_pipe::NamedPipeServer,
}
#[cfg(windows)]
impl Listener {
    pub fn bind(path: &Path) -> io::Result<Self> {
        let server = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(path)?;
        Ok(Self { server })
    }
    pub async fn run(self, events: mpsc::Sender<Event>, output: mpsc::Receiver<Batch>) {
        self.run_with_stats(events, output, Arc::new(OutputStats::default()))
            .await;
    }
    pub async fn run_with_stats(
        mut self,
        events: mpsc::Sender<Event>,
        mut output: mpsc::Receiver<Batch>,
        stats: Arc<OutputStats>,
    ) {
        let mut session = 0;
        loop {
            if self.server.connect().await.is_err() {
                break;
            }
            session += 1;
            let _ = connection(&mut self.server, session, &events, &mut output, &stats).await;
            let _ = self.server.disconnect();
            if events.is_closed() {
                break;
            }
            // Reuse the same first-instance server handle so another server cannot
            // replace our pipe between connections.
        }
    }
    pub async fn run_rpc(
        mut self,
        commands: mpsc::Sender<crate::api::Request>,
        publisher: tokio::sync::broadcast::Sender<crate::api::Wire>,
        hub: crate::api::pubsub::Hub,
    ) {
        loop {
            if self.server.connect().await.is_err() {
                break;
            }
            let _ =
                rpc_connection(&mut self.server, &commands, publisher.clone(), hub.clone()).await;
            let _ = self.server.disconnect();
            if commands.is_closed() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn batch(messages: usize) -> Batch {
        Batch {
            session: 1,
            messages: (0..messages)
                .map(|_| {
                    super::super::envelope(
                        super::super::messages::protobuf_message::Message::Version(
                            super::super::messages::Version {
                                protocol_version: 2,
                            },
                        ),
                    )
                })
                .collect(),
        }
    }
    #[tokio::test]
    async fn completed_partial_failed_and_cancelled_writes_have_distinct_counts() {
        let stats = OutputStats::default();
        let (mut writer, mut reader) = tokio::io::duplex(16);
        let (written, ()) = tokio::join!(write_batch(&mut writer, batch(2), &stats), async {
            for _ in 0..2 {
                read(&mut reader).await.unwrap();
            }
        });
        written.unwrap();
        assert_eq!(stats.snapshot().steamvr_output_batches_written, 1);
        drop(reader);
        assert!(write_batch(&mut writer, batch(1), &stats).await.is_err());
        assert_eq!(stats.snapshot().steamvr_output_write_failed, 1);
        let (mut writer, _blocked_reader) = tokio::io::duplex(1);
        let mut writing = Box::pin(write_batch(&mut writer, batch(2), &stats));
        assert!(matches!(
            futures_util::poll!(&mut writing),
            std::task::Poll::Pending
        ));
        drop(writing);
        assert_eq!(stats.snapshot().steamvr_output_write_cancelled, 1);
        assert_eq!(stats.snapshot().steamvr_output_batches_written, 1);
    }
    #[tokio::test(start_paused = true)]
    async fn timeout_of_a_partially_written_batch_is_a_failure_not_a_completed_batch() {
        let stats = OutputStats::default();
        let (mut writer, _blocked_reader) = tokio::io::duplex(1);
        assert!(write_batch(&mut writer, batch(2), &stats).await.is_err());
        assert_eq!(stats.snapshot().steamvr_output_write_failed, 1);
        assert_eq!(stats.snapshot().steamvr_output_batches_written, 0);
        assert_eq!(stats.snapshot().steamvr_output_write_cancelled, 0);
    }
}
