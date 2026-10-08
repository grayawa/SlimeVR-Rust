//! Preserve normal samples, but compact stale pose backlogs without crossing controls.
use crate::protocol::{self, Packet};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::{net::UdpSocket, sync::Notify, task::JoinHandle};

const POSE_CAPACITY: usize = 256;
const CONTROL_CAPACITY: usize = 128;
type Field = (u8, u8); // Sensor ID + rotation / acceleration / position / flex.

pub(super) struct Datagram {
    pub from: SocketAddr,
    pub bytes: Vec<u8>,
    pub received: Instant,
    pub parsed: Result<protocol::Datagram, protocol::ParseError>,
}
struct Pending {
    datagram: Datagram,
    fields: Option<BTreeSet<Field>>,
    sequence: i64,
}
impl Pending {
    fn new(from: SocketAddr, bytes: Vec<u8>, received: Instant) -> Self {
        let parsed = protocol::parse(&bytes);
        let sequence = parsed.as_ref().map_or(0, |p| p.sequence);
        let fields = parsed.as_ref().ok().and_then(|p| {
            if !p.warnings.is_empty() || p.packets.is_empty() {
                return None;
            }
            let mut fields = BTreeSet::new();
            for packet in &p.packets {
                match packet {
                    Packet::Rotation {
                        sensor_id,
                        data_type: 1,
                        acceleration,
                        ..
                    } => {
                        fields.insert((*sensor_id, 0));
                        if acceleration.is_some() {
                            fields.insert((*sensor_id, 1));
                        }
                    }
                    Packet::Acceleration { sensor_id, .. } => {
                        fields.insert((*sensor_id, 1));
                    }
                    Packet::Position { sensor_id, .. } => {
                        fields.insert((*sensor_id, 2));
                    }
                    Packet::Flex { sensor_id, .. } => {
                        fields.insert((*sensor_id, 3));
                    }
                    // Mixed bundles, errors, telemetry, handshakes and malformed data
                    // remain whole ordered messages; never partially rewrite packets.
                    _ => return None,
                }
            }
            Some(fields)
        });
        Self {
            datagram: Datagram {
                from,
                bytes,
                received,
                parsed,
            },
            fields,
            sequence,
        }
    }
}
#[derive(Default)]
pub(super) struct Stats {
    pub coalesced: AtomicU64,
    pub dropped_poses: AtomicU64,
    pub dropped_controls: AtomicU64,
}
#[derive(Default)]
struct Queue {
    pending: VecDeque<Pending>,
    poses: usize,
    controls: usize,
}
impl Queue {
    fn push(&mut self, packet: Pending, stats: &Stats) {
        if packet.fields.is_none() {
            if self.controls == CONTROL_CAPACITY {
                stats.dropped_controls.fetch_add(1, Ordering::Relaxed);
                return;
            }
            self.controls += 1;
        } else {
            self.poses += 1;
        }
        self.pending.push_back(packet);
        if self.poses > POSE_CAPACITY {
            self.compact(stats);
            if self.poses > POSE_CAPACITY {
                // Only cardinality/barriers can still fill the pose reservoir.
                // Evict an old pose, never an ordered control, and admit the new one.
                let oldest = self
                    .pending
                    .iter()
                    .position(|p| p.fields.is_some())
                    .unwrap();
                self.pending.remove(oldest);
                self.poses -= 1;
                stats.dropped_poses.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    fn compact(&mut self, stats: &Stats) {
        let mut covered: BTreeMap<SocketAddr, (i64, BTreeSet<Field>)> = BTreeMap::new();
        let mut keep = vec![true; self.pending.len()];
        for (index, packet) in self.pending.iter().enumerate().rev() {
            let peer = packet.datagram.from;
            let Some(fields) = &packet.fields else {
                covered.remove(&peer); // Per-device control/session boundary.
                continue;
            };
            let (sequence, newer_fields) = covered
                .entry(peer)
                .or_insert_with(|| (packet.sequence, BTreeSet::new()));
            // Zero sequences are accepted in arrival order; do not compact across
            // zero/nonzero transitions, duplicates, or reordered positive sequences.
            let ordered = (*sequence == 0 && packet.sequence == 0)
                || (packet.sequence > 0 && *sequence > packet.sequence);
            if !ordered {
                newer_fields.clear();
            }
            if !newer_fields.is_empty() && fields.is_subset(newer_fields) {
                keep[index] = false;
            }
            newer_fields.extend(fields.iter().copied());
            *sequence = packet.sequence;
        }
        let mut index = 0;
        self.pending.retain(|packet| {
            let retained = keep[index];
            index += 1;
            if !retained {
                debug_assert!(packet.fields.is_some());
                self.poses -= 1;
                stats.coalesced.fetch_add(1, Ordering::Relaxed);
            }
            retained
        });
    }
    fn prepare(&mut self, now: Instant, period: Duration, stats: &Stats) {
        // Under light load deliver every sample to the filter. Once delivery is
        // later than one pose tick, keep latest fields within each control boundary.
        if self.pending.iter().any(|p| {
            p.fields.is_some() && now.saturating_duration_since(p.datagram.received) > period
        }) {
            self.compact(stats);
        }
    }
    fn pop(&mut self) -> Option<Datagram> {
        let packet = self.pending.pop_front()?;
        if packet.fields.is_some() {
            self.poses -= 1;
        } else {
            self.controls -= 1;
        }
        Some(packet.datagram)
    }
    #[cfg(test)]
    fn drain(&mut self, now: Instant, period: Duration, stats: &Stats) -> Vec<Datagram> {
        self.prepare(now, period, stats);
        std::iter::from_fn(|| self.pop()).collect()
    }
}
#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    ready: Notify,
    closed: AtomicBool,
    error: Mutex<Option<io::Error>>,
    stats: Stats,
}
struct Exit(Arc<Shared>);
impl Drop for Exit {
    fn drop(&mut self) {
        self.0.closed.store(true, Ordering::Release);
        self.0.ready.notify_one();
    }
}
pub(super) struct Ingress {
    shared: Arc<Shared>,
    period: Duration,
    task: JoinHandle<()>,
}
impl Ingress {
    pub fn start(socket: Arc<UdpSocket>, period: Duration) -> Self {
        let shared = Arc::new(Shared::default());
        let incoming = shared.clone();
        let task = tokio::spawn(async move {
            let _exit = Exit(incoming.clone());
            let mut buffer = vec![0u8; 65536];
            loop {
                match socket.recv_from(&mut buffer).await {
                    Ok((length, from)) => {
                        let received = Instant::now();
                        let packet = Pending::new(from, buffer[..length].to_vec(), received);
                        let Ok(mut queue) = incoming.queue.lock() else {
                            break;
                        };
                        queue.push(packet, &incoming.stats);
                        drop(queue);
                        incoming.ready.notify_one();
                    }
                    Err(error) => {
                        if let Ok(mut saved) = incoming.error.lock() {
                            *saved = Some(error);
                        }
                        break;
                    }
                }
            }
        });
        Self {
            shared,
            period,
            task,
        }
    }
    pub fn stats(&self) -> &Stats {
        &self.shared.stats
    }
    pub fn try_recv(&self) -> io::Result<Option<Datagram>> {
        Ok(self
            .shared
            .queue
            .lock()
            .map_err(|_| io::Error::other("UDP ingress queue poisoned"))?
            .pop())
    }
    pub async fn recv(&self) -> io::Result<Datagram> {
        loop {
            let notified = self.shared.ready.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let packet = {
                let mut queue = self
                    .shared
                    .queue
                    .lock()
                    .map_err(|_| io::Error::other("UDP ingress queue poisoned"))?;
                queue.prepare(Instant::now(), self.period, &self.shared.stats);
                queue.pop()
            };
            if let Some(packet) = packet {
                return Ok(packet);
            }
            if self.shared.closed.load(Ordering::Acquire) {
                return Err(self
                    .shared
                    .error
                    .lock()
                    .map_err(|_| io::Error::other("UDP ingress error unavailable"))?
                    .take()
                    .unwrap_or_else(|| io::Error::other("UDP ingress stopped")));
            }
            notified.await;
        }
    }
}
impl Drop for Ingress {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
#[path = "ingress_bench.rs"]
mod bench;

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn peer(port: u16) -> SocketAddr {
        ([127, 0, 0, 1], port).into()
    }
    pub(super) fn wire(id: u32, sequence: i64, payload: &[u8]) -> Vec<u8> {
        [
            id.to_be_bytes().as_slice(),
            sequence.to_be_bytes().as_slice(),
            payload,
        ]
        .concat()
    }
    pub(super) fn rotation(sensor: u8, sequence: i64) -> Vec<u8> {
        let mut body = vec![sensor, 1];
        for value in [sequence as f32 / 1000., 0., 0., 1.] {
            body.extend(value.to_be_bytes());
        }
        body.push(3);
        wire(17, sequence, &body)
    }
    fn acceleration(sensor: u8, sequence: i64) -> Vec<u8> {
        let mut body = Vec::new();
        for value in [1f32, 2., 3.] {
            body.extend(value.to_be_bytes());
        }
        body.push(sensor);
        wire(4, sequence, &body)
    }
    pub(super) fn handshake() -> Vec<u8> {
        let mut body = Vec::new();
        for value in [9u32, 13, 1, 0, 0, 0, 22] {
            body.extend(value.to_be_bytes());
        }
        body.extend(b"\x05good\0");
        body.extend([2, 0, 0, 0, 0, 1]);
        wire(3, 0, &body)
    }
    pub(super) fn bundle(sequence: i64, compact: bool, packets: &[Vec<u8>]) -> Vec<u8> {
        let mut body = Vec::new();
        for packet in packets {
            let mut inner = if compact {
                vec![packet[3]]
            } else {
                packet[..4].to_vec()
            };
            inner.extend_from_slice(&packet[12..]);
            if compact {
                body.push(inner.len() as u8);
            } else {
                body.extend((inner.len() as u16).to_be_bytes());
            }
            body.extend(inner);
        }
        wire(if compact { 101 } else { 100 }, sequence, &body)
    }
    fn sequences(batch: &[Datagram]) -> Vec<i64> {
        batch
            .iter()
            .map(|p| protocol::parse(&p.bytes).unwrap().sequence)
            .collect()
    }
    fn push(queue: &mut Queue, stats: &Stats, start: Instant, from: SocketAddr, bytes: Vec<u8>) {
        queue.push(Pending::new(from, bytes, start), stats);
    }
    #[test]
    fn normal_delivery_keeps_all_filter_samples_but_stalled_delivery_keeps_latest_per_sensor_and_field(
    ) {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        let packets = [
            rotation(0, 1),
            rotation(1, 2),
            acceleration(0, 3),
            rotation(0, 4),
            rotation(1, 5),
        ];
        for bytes in &packets {
            push(&mut queue, &stats, start, peer(1), bytes.clone());
        }
        assert_eq!(
            sequences(&queue.drain(start, Duration::from_millis(4), &stats)),
            [1, 2, 3, 4, 5]
        );
        for bytes in packets {
            push(&mut queue, &stats, start, peer(1), bytes);
        }
        let batch = queue.drain(
            start + Duration::from_millis(20),
            Duration::from_millis(4),
            &stats,
        );
        assert_eq!(sequences(&batch), [3, 4, 5]); // Keep acceleration independently of rotation.
        assert_eq!(stats.coalesced.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn pure_bundles_preserve_unreplaced_sensors_and_combined_acceleration() {
        for compact in [false, true] {
            let start = Instant::now();
            let stats = Stats::default();
            let mut queue = Queue::default();
            let bytes = bundle(
                1,
                compact,
                &[rotation(0, 0), acceleration(0, 0), rotation(1, 0)],
            );
            push(&mut queue, &stats, start, peer(1), bytes.clone());
            push(&mut queue, &stats, start, peer(1), rotation(0, 2));
            let batch = queue.drain(
                start + Duration::from_millis(20),
                Duration::from_millis(4),
                &stats,
            );
            assert_eq!(sequences(&batch), [1, 2]);
            assert_eq!(batch[0].bytes, bytes); // Never remove sensor 1 or acceleration from a bundle.
            for bytes in [
                bundle(
                    3,
                    compact,
                    &[rotation(0, 0), acceleration(0, 0), rotation(1, 0)],
                ),
                rotation(0, 4),
                acceleration(0, 5),
                rotation(1, 6),
            ] {
                push(&mut queue, &stats, start, peer(1), bytes);
            }
            assert_eq!(
                sequences(&queue.drain(
                    start + Duration::from_millis(20),
                    Duration::from_millis(4),
                    &stats
                )),
                [4, 5, 6]
            );
        }
        // Packet 23 carries both fields; a newer rotation alone cannot replace it.
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        let mut body = vec![0];
        for value in [0i16, 0, 0, 32767, 128, 256, 384] {
            body.extend(value.to_be_bytes());
        }
        let combined = wire(23, 1, &body);
        for bytes in [combined.clone(), rotation(0, 2)] {
            push(&mut queue, &stats, start, peer(1), bytes);
        }
        let batch = queue.drain(
            start + Duration::from_millis(20),
            Duration::from_millis(4),
            &stats,
        );
        assert_eq!(sequences(&batch), [1, 2]);
        assert_eq!(batch[0].bytes, combined);
        for bytes in [combined, rotation(0, 2), acceleration(0, 3)] {
            push(&mut queue, &stats, start, peer(1), bytes);
        }
        assert_eq!(
            sequences(&queue.drain(
                start + Duration::from_millis(20),
                Duration::from_millis(4),
                &stats
            )),
            [2, 3]
        );
    }
    #[test]
    fn controls_reconnects_mixed_bundles_and_peers_form_independent_order_boundaries() {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for (from, bytes) in [
            (peer(1), rotation(0, 1)),
            (peer(2), rotation(0, 1)),
            (peer(1), wire(24, 2, &[0, 0, 1])), // Config ack protects the pre-control orientation.
            (peer(1), rotation(0, 3)),
            (peer(1), rotation(0, 4)),
            (peer(2), rotation(0, 2)),
            (
                peer(1),
                bundle(5, false, &[rotation(0, 0), wire(21, 0, &[3])]),
            ),
            (peer(1), rotation(0, 6)),
            (peer(1), handshake()), // Reconnect resets the sequence within the same peer.
            (peer(1), rotation(0, 1)),
            (peer(1), rotation(0, 2)),
        ] {
            push(&mut queue, &stats, start, from, bytes);
        }
        let batch = queue.drain(
            start + Duration::from_millis(20),
            Duration::from_millis(4),
            &stats,
        );
        assert_eq!(sequences(&batch), [1, 2, 4, 2, 5, 6, 0, 2]);
        assert_eq!(batch[0].from, peer(1));
        assert_eq!(batch[3].from, peer(2));
    }
    #[test]
    fn duplicates_reordering_and_zero_sequence_transitions_keep_receiver_validation() {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for sequence in [12, 12, 11, 0, 1] {
            push(&mut queue, &stats, start, peer(1), rotation(0, sequence));
        }
        assert_eq!(
            sequences(&queue.drain(
                start + Duration::from_millis(20),
                Duration::from_millis(4),
                &stats
            )),
            [12, 12, 11, 0, 1]
        );
        for _ in 0..3 {
            push(&mut queue, &stats, start, peer(1), rotation(0, 0));
        }
        assert_eq!(
            queue
                .drain(
                    start + Duration::from_millis(20),
                    Duration::from_millis(4),
                    &stats
                )
                .len(),
            1
        );
    }
    #[test]
    fn pose_flood_admits_latest_and_does_not_evict_or_consume_control_capacity() {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for sequence in 1..=CONTROL_CAPACITY as i64 {
            push(
                &mut queue,
                &stats,
                start,
                peer(1),
                wire(24, sequence, &[0, 0, 1]),
            );
        }
        for sequence in 1..=1000 {
            push(
                &mut queue,
                &stats,
                start,
                peer(2),
                rotation((sequence % 2) as u8, sequence),
            );
        }
        let batch = queue.drain(
            start + Duration::from_millis(20),
            Duration::from_millis(4),
            &stats,
        );
        assert_eq!(batch.len(), CONTROL_CAPACITY + 2);
        assert_eq!(sequences(&batch[CONTROL_CAPACITY..]), [999, 1000]);
        assert_eq!(stats.dropped_controls.load(Ordering::Relaxed), 0);
        assert_eq!(stats.dropped_poses.load(Ordering::Relaxed), 0);
        assert_eq!(stats.coalesced.load(Ordering::Relaxed), 998);
    }
    #[test]
    fn distinct_stream_overflow_evicts_old_pose_and_counts_control_overflow_separately() {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for sequence in 1..=CONTROL_CAPACITY as i64 + 1 {
            push(
                &mut queue,
                &stats,
                start,
                peer(1),
                wire(24, sequence, &[0, 0, 1]),
            );
        }
        for port in 1..=POSE_CAPACITY as u16 + 1 {
            push(&mut queue, &stats, start, peer(port + 100), rotation(0, 1));
        }
        assert_eq!(queue.poses, POSE_CAPACITY);
        assert_eq!(queue.controls, CONTROL_CAPACITY);
        assert_eq!(stats.dropped_controls.load(Ordering::Relaxed), 1);
        assert_eq!(stats.dropped_poses.load(Ordering::Relaxed), 1);
        assert_eq!(
            queue.pending.back().unwrap().datagram.from,
            peer(POSE_CAPACITY as u16 + 101)
        );
    }
    #[test]
    fn budget_remainder_stays_in_bounded_queue_and_new_poses_replace_it_without_overtaking_controls(
    ) {
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for sequence in 1..=128 {
            push(
                &mut queue,
                &stats,
                start,
                peer(1),
                wire(24, sequence, &[0, 0, 1]),
            );
        }
        for sequence in 1..=1000 {
            push(
                &mut queue,
                &stats,
                start,
                peer(2),
                rotation((sequence % 2) as u8, sequence),
            );
        }
        queue.prepare(
            start + Duration::from_millis(100),
            Duration::from_millis(4),
            &stats,
        );
        for sequence in 1..=16 {
            assert_eq!(queue.pop().unwrap().parsed.unwrap().sequence, sequence);
        }
        for sequence in 1001..=1002 {
            push(
                &mut queue,
                &stats,
                start,
                peer(2),
                rotation((sequence % 2) as u8, sequence),
            );
        }
        let batch = queue.drain(
            start + Duration::from_millis(101),
            Duration::from_millis(4),
            &stats,
        );
        let mut expected: Vec<_> = (17..=128).collect();
        expected.extend([1001, 1002]);
        assert_eq!(sequences(&batch), expected);
        assert_eq!(queue.controls, 0);
        assert_eq!(queue.poses, 0);
        assert_eq!(stats.coalesced.load(Ordering::Relaxed), 1000);
    }
    #[test]
    fn cached_parse_preserves_warnings_mixed_controls_and_malformed_rejection() {
        let config = crate::receiver::ReceiverConfig {
            accept_new_devices: true,
            ..Default::default()
        };
        let mut raw = crate::receiver::Receiver::new(config.clone()).unwrap();
        let mut cached = crate::receiver::Receiver::new(config).unwrap();
        let mut invalid_float = rotation(0, 2);
        invalid_float[14..18].copy_from_slice(&f32::NAN.to_be_bytes());
        let packets = [
            handshake(),
            wire(15, 1, &[0, 1, 13]),
            invalid_float,
            bundle(3, true, &[rotation(0, 0), wire(21, 0, &[3])]),
            wire(17, 4, &[0]),
            wire(99, 5, &[1, 2, 3]),
        ];
        for (at, bytes) in packets.into_iter().enumerate() {
            let packet = Pending::new(peer(1), bytes, Instant::now());
            assert!(packet.fields.is_none());
            let a = raw.receive_timed(peer(1), &packet.datagram.bytes, at as u64, Some(0));
            let b = cached.receive_parsed(peer(1), packet.datagram.parsed, at as u64, Some(0));
            assert_eq!(a.outbound, b.outbound);
            assert_eq!(a.events, b.events);
        }
        assert_eq!(raw.snapshot(10), cached.snapshot(10));
        assert_eq!(raw.counters.malformed, 1);
    }
    #[test]
    fn retained_original_datagrams_replay_with_same_receiver_state_and_control_replies() {
        use crate::{
            receiver::{Receiver, ReceiverConfig},
            recording::{Record, Recorder},
        };
        let start = Instant::now();
        let stats = Stats::default();
        let mut queue = Queue::default();
        for bytes in [
            handshake(),
            wire(15, 1, &[0, 1, 13]),
            rotation(0, 2),
            wire(15, 3, &[1, 1, 13]),
            rotation(0, 4),
            rotation(1, 5),
            rotation(0, 6),
        ] {
            push(&mut queue, &stats, start, peer(1), bytes);
        }
        let batch = queue.drain(
            start + Duration::from_millis(20),
            Duration::from_millis(4),
            &stats,
        );
        assert_eq!(sequences(&batch), [0, 1, 2, 3, 5, 6]);
        let config = ReceiverConfig {
            accept_new_devices: true,
            ..Default::default()
        };
        let mut receiver = Receiver::new(config.clone()).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let journal = directory.path().join("queue.jsonl");
        let mut recorder = Recorder::create(&journal, &config).unwrap();
        for (index, packet) in batch.into_iter().enumerate() {
            let at = index as u64;
            recorder
                .write(&Record::Receive {
                    received_at_ms: None,
                    at_ms: at,
                    from: packet.from,
                    hex: crate::recording::encode_hex(&packet.bytes),
                })
                .unwrap();
            for reply in receiver.receive(packet.from, &packet.bytes, at).outbound {
                recorder
                    .write(&Record::Send {
                        at_ms: at,
                        to: reply.to,
                        hex: crate::recording::encode_hex(&reply.bytes),
                    })
                    .unwrap();
            }
        }
        recorder.finish(6).unwrap();
        assert_eq!(receiver.counters.sequence_rejected, 0);
        assert_eq!(receiver.devices["02:00:00:00:00:01"].sensors[&0].samples, 2);
        assert_eq!(receiver.devices["02:00:00:00:00:01"].sensors[&1].samples, 1);
        let replay = crate::recording::replay(&journal, |_| {}).unwrap();
        assert!(replay.verified_replies >= 4);
        assert_eq!(receiver.snapshot(6), replay.receiver.snapshot(6));
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn real_udp_recovery_returns_latest_of_both_sensors_without_old_pose_replay() {
        let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let address = socket.local_addr().unwrap();
        let ingress = Ingress::start(socket, Duration::from_millis(4));
        let sender = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let send = std::thread::spawn(move || {
            for sequence in 1..=80 {
                sender
                    .send_to(&rotation((sequence % 2) as u8, sequence), address)
                    .unwrap();
                if sequence % 20 == 0 {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        send.join().unwrap(); // Owner blocked; UDP task must still drain the socket.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let arrived = ingress
                .shared
                .queue
                .lock()
                .unwrap()
                .pending
                .back()
                .is_some_and(|p| p.sequence == 80);
            if arrived {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "UDP worker did not receive the last sample"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(10));
        let first = ingress.recv().await.unwrap();
        let mut batch = vec![first];
        while let Some(packet) = ingress.try_recv().unwrap() {
            batch.push(packet);
        }
        assert_eq!(sequences(&batch), [79, 80]);
        assert_eq!(ingress.stats().coalesced.load(Ordering::Relaxed), 78);
        drop(ingress);
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match UdpSocket::bind(address).await {
                    Ok(_) => break,
                    Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
                        tokio::task::yield_now().await
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        })
        .await
        .unwrap();
    }
}
