//! Opt-in release microbenchmark; never assert wall-clock speed in CI.
use super::{
    tests::{bundle, handshake, peer, rotation, wire},
    *,
};
use crate::receiver::{Receiver, ReceiverConfig};
use std::hint::black_box;

fn distribution(mut values: Vec<Duration>) -> serde_json::Value {
    values.sort_unstable();
    let quantile =
        |p: usize| values[(values.len() * p).div_ceil(1000).saturating_sub(1)].as_secs_f64() * 1e6;
    serde_json::json!({"p50":quantile(500), "p95":quantile(950), "p99":quantile(990),
        "p999":quantile(999), "max":values.last().unwrap().as_secs_f64() * 1e6})
}

fn receiver() -> Receiver {
    let mut receiver = Receiver::new(ReceiverConfig {
        accept_new_devices: true,
        ..Default::default()
    })
    .unwrap();
    for device in 0..6 {
        let mut hello = handshake();
        *hello.last_mut().unwrap() = device + 1;
        receiver.receive(peer(100 + device as u16), &hello, 0);
        receiver.receive(peer(100 + device as u16), &wire(15, 1, &[0, 1, 13]), 0);
        receiver.receive(peer(100 + device as u16), &wire(15, 2, &[1, 1, 13]), 0);
    }
    receiver
}

fn run(
    packets: &[(SocketAddr, Vec<u8>)],
    stalled: bool,
    reparse: bool,
    case: &str,
) -> serde_json::Value {
    let queue = Mutex::new(Queue::default());
    let stats = Stats::default();
    let mut receiver = receiver();
    let received = Instant::now();
    let drain_at = received
        + if stalled {
            Duration::from_millis(100)
        } else {
            Duration::ZERO
        };
    let mut enqueue = Vec::with_capacity(packets.len());
    let mut dispatch = Vec::with_capacity(packets.len());
    let mut processed = 0;
    let start = Instant::now();
    for (index, (from, bytes)) in packets.iter().enumerate() {
        let phase = Instant::now();
        let packet = Pending::new(*from, bytes.clone(), received);
        queue.lock().unwrap().push(packet, &stats);
        enqueue.push(phase.elapsed());
        if !stalled || (index + 1) % 256 == 0 || index + 1 == packets.len() {
            queue
                .lock()
                .unwrap()
                .prepare(drain_at, Duration::from_millis(4), &stats);
            loop {
                let phase = Instant::now();
                let Some(packet) = queue.lock().unwrap().pop() else {
                    break;
                };
                processed += 1;
                let effects = if reparse {
                    receiver.receive(packet.from, &packet.bytes, processed)
                } else {
                    receiver.receive_parsed(packet.from, packet.parsed, processed, None)
                };
                black_box(effects);
                dispatch.push(phase.elapsed());
            }
        }
    }
    let elapsed = start.elapsed();
    let result = serde_json::json!({"type":"udp_ingress_benchmark", "case":case,
        "receive_path":if reparse {"raw_reparse_reference"} else {"cached_parse"},
        "stalled":stalled, "input_datagrams":packets.len(), "selected_datagrams":processed,
        "coalesced":stats.coalesced.load(Ordering::Relaxed),
        "dropped_poses":stats.dropped_poses.load(Ordering::Relaxed),
        "dropped_controls":stats.dropped_controls.load(Ordering::Relaxed),
        "elapsed_ms":elapsed.as_secs_f64()*1000., "input_datagrams_per_second":packets.len() as f64 / elapsed.as_secs_f64(),
        "enqueue_us":distribution(enqueue), "dispatch_us":distribution(dispatch)});
    println!("{result}");
    receiver.snapshot(processed)
}

#[test]
#[ignore = "Run in release with --ignored --nocapture to measure queue, parsing and receiver costs"]
fn benchmark_udp_queue_and_cached_parse() {
    for case in [
        "single_rotation",
        "dual_sensor_bundle",
        "control_heavy_bundle",
    ] {
        let packets: Vec<_> = (0..30_720)
            .map(|index| {
                let sequence = index / 6 + 3;
                let sensor = (index / 6 % 2) as u8;
                let bytes = if case == "single_rotation" {
                    rotation(sensor, sequence)
                } else if case == "control_heavy_bundle" && sequence % 4 == 0 {
                    wire(24, sequence, &[sensor, 0, 1])
                } else {
                    bundle(
                        sequence,
                        sequence % 2 == 0,
                        &[rotation(0, 0), rotation(1, 0), wire(4, 0, &[0; 13])],
                    )
                };
                (peer(100 + (index % 6) as u16), bytes)
            })
            .collect();
        // Alternate paths per repeat to reduce warmup/order bias; retain distributions per run.
        for repeat in 0..3 {
            for stalled in [false, true] {
                let a = run(&packets, stalled, repeat % 2 == 0, case);
                let b = run(&packets, stalled, repeat % 2 != 0, case);
                assert_eq!(a, b); // Same controls, accepted samples and final cached poses.
            }
        }
    }
}
