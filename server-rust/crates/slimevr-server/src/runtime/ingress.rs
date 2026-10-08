//! Socket draining runs independently of the single-owner receiver/pose loop.
use std::{
    io,
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tokio::{net::UdpSocket, sync::mpsc, task::JoinHandle};

// About 30 ms at six trackers * 700 packets/s. Never accumulate an unbounded
// backlog of old rotations during a stall; overload is visible in diagnostics.
const CAPACITY: usize = 128;
pub(super) struct Datagram {
    pub from: SocketAddr,
    pub bytes: Vec<u8>,
}
pub(super) struct Ingress {
    pub packets: mpsc::Receiver<io::Result<Datagram>>,
    pub dropped: Arc<AtomicU64>,
    task: JoinHandle<()>,
}
impl Ingress {
    pub fn start(socket: Arc<UdpSocket>) -> Self {
        let (sender, packets) = mpsc::channel(CAPACITY);
        let dropped = Arc::new(AtomicU64::new(0));
        let counter = dropped.clone();
        let task = tokio::spawn(async move {
            let mut buffer = vec![0u8; 65536];
            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((length, from)) => {
                        // Reserve before copying: overflow neither allocates nor waits.
                        match sender.try_reserve() {
                            Ok(permit) => permit.send(Ok(Datagram {
                                from,
                                bytes: buffer[..length].to_vec(),
                            })),
                            Err(mpsc::error::TrySendError::Full(_)) => {
                                counter.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(mpsc::error::TrySendError::Closed(_)) => break,
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error)).await;
                        break;
                    }
                }
            }
        });
        Self {
            packets,
            dropped,
            task,
        }
    }
}
impl Drop for Ingress {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn socket_drains_while_owner_is_blocked_and_bounds_backlog() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let mut ingress = Ingress::start(socket);
        // This synchronous sender also prevents the owner future from yielding.
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        for n in 0..(CAPACITY * 3) {
            sender.send_to(&(n as u32).to_be_bytes(), address).unwrap();
            std::thread::sleep(std::time::Duration::from_micros(500));
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert_eq!(ingress.packets.len(), CAPACITY);
        assert!(ingress.dropped.load(Ordering::Relaxed) > 0);
        let packet = ingress.packets.recv().await.unwrap().unwrap();
        assert_eq!(packet.bytes, 0u32.to_be_bytes());
        assert_eq!(packet.from, sender.local_addr().unwrap());
        drop(ingress);
        // Receiver termination releases the socket despite outstanding datagrams.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let _rebound = UdpSocket::bind(address).await.unwrap();
    }
}
