#!/usr/bin/env python3
"""Measure real loopback UDP + pose ticks, optionally competing with CPU workers."""
import argparse
import json
import math
import multiprocessing
import os
from pathlib import Path
import queue
import socket
import struct
import subprocess
import tempfile
import threading
import time

ROLES = (
    "chest", "hip", "left_upper_leg", "left_lower_leg", "right_upper_leg", "right_lower_leg",
    "waist", "left_foot", "right_foot", "upper_chest", "left_upper_arm", "right_upper_arm",
)


def cpu_load(stop):
    value = 1
    while not stop.is_set():
        for _ in range(10000):
            value = (value * 1664525 + 1013904223) & 0xFFFFFFFF


def wire(packet_id, sequence, body=b""):
    return struct.pack(">Iq", packet_id, sequence) + body


def cgroup_cpu():
    try:
        quota, period = Path("/sys/fs/cgroup/cpu.max").read_text().split()
        usage = dict(line.split() for line in Path("/sys/fs/cgroup/cpu.stat").read_text().splitlines())
        return {"quota_cpus": None if quota == "max" else int(quota) / int(period),
                "usage_usec": int(usage["usage_usec"])}
    except (OSError, KeyError, ValueError):
        return None


def configuration(devices, sensors):
    lines = ["trackers:"]
    for device in range(devices):
        for sensor in range(sensors):
            key = f"udp://02:00:00:00:00:{device + 1:02X}/{sensor}"
            lines += [f"  {json.dumps(key)}:", f"    designation: {ROLES[device * sensors + sensor]}"]
    lines += ["keybindings:"]
    for action in ("fullReset", "yawReset", "mountingReset", "pauseTracking", "feetMountingReset"):
        lines.append(f'  {action}Binding: ""')
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", type=Path, required=True)
    parser.add_argument("--duration", type=int, default=10, help="Seconds; 10 gives >=1000 tick samples at 4ms under normal load")
    parser.add_argument("--devices", type=int, choices=range(1, 7), default=6)
    parser.add_argument("--sensors", type=int, choices=(1, 2), default=2)
    parser.add_argument("--rate", type=int, default=700, help="Pose datagrams per device per second")
    parser.add_argument("--cpu-workers", type=int, default=0, help="Additional busy processes, usually the logical CPU count")
    parser.add_argument("--output", type=Path, help="Write summary JSON to this file")
    args = parser.parse_args()
    if not args.backend.is_file() or args.duration < 1 or args.rate < 1 or args.cpu_workers < 0:
        parser.error("Use an existing backend, positive duration/rate and nonnegative CPU workers")
    context = multiprocessing.get_context("spawn")
    stop = context.Event()
    workers = []
    reports, backpressure, errors = [], [], []
    ready = queue.Queue()
    readers = []
    sockets = []
    process = None
    with tempfile.TemporaryDirectory(prefix="slimevr-runtime-benchmark-") as directory:
        config = Path(directory) / "vrconfig.yml"
        config.write_text(configuration(args.devices, args.sensors), encoding="utf-8")
        command = [str(args.backend.resolve()), "listen", "--bind", "127.0.0.1:0",
                   "--api-bind", "127.0.0.1:0", "--config", str(config), "--accept-new-devices",
                   "--no-discovery", "--no-steamvr", "--no-bindings-provider", "--shutdown-on-stdin-eof",
                   "--run-for", str(args.duration), "--pose-ms", "4", "--log-level", "info",
                   "--timing-window-ms", str(args.duration * 1000)]

        def collect(stream):
            for line in stream:
                try:
                    message = json.loads(line)
                except json.JSONDecodeError:
                    errors.append(line.strip()[:500])
                    continue
                kind = message.get("type")
                if kind == "listening":
                    ready.put(message["bind"])
                elif kind == "runtime_timing":
                    reports.append(message)
                elif kind == "udp_ingress_backpressure":
                    backpressure.append(message)
                elif message.get("level") == "error":
                    errors.append(message)

        try:
            process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                       stderr=subprocess.PIPE, text=True, encoding="utf-8")
            for stream in (process.stdout, process.stderr):
                reader = threading.Thread(target=collect, args=(stream,), daemon=True)
                reader.start()
                readers.append(reader)
            deadline = time.monotonic() + 30
            while True:
                try:
                    address = ready.get(timeout=0.1)
                    break
                except queue.Empty:
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(f"Backend did not start: {errors[-5:]}")
            host, port = address.rsplit(":", 1)
            destination = (host, int(port))
            sequences = [args.sensors] * args.devices
            for device in range(args.devices):
                sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                sock.bind(("127.0.0.1", 0))
                sockets.append(sock)
                hello = struct.pack(">7I", 9, 13, 1, 0, 0, 0, 22) + b"\x05good\0" + bytes((2, 0, 0, 0, 0, device + 1))
                sock.sendto(wire(3, 0, hello), destination)
                for sensor in range(args.sensors):
                    sock.sendto(wire(15, sensor + 1, bytes((sensor, 1, 13))), destination)
            for _ in range(args.cpu_workers):
                worker = context.Process(target=cpu_load, args=(stop,))
                worker.start()
                workers.append(worker)
            start = time.monotonic()
            cpu_start = cgroup_cpu()
            sent = 0
            target_rate = args.rate * args.devices
            while process.poll() is None and time.monotonic() - start < args.duration:
                # Pace the average rate without an unlimited catch-up burst after a host stall.
                due = int((time.monotonic() - start) * target_rate)
                for _ in range(min(32, max(0, due - sent))):
                    device = sent % args.devices
                    sensor = (sent // args.devices) % args.sensors
                    angle = math.sin(sent / target_rate) * 0.25
                    body = struct.pack(">B7h", sensor, int(math.sin(angle / 2) * 32767),
                                       0, 0, int(math.cos(angle / 2) * 32767), 0, 1255, 0)
                    sequences[device] += 1
                    sockets[device].sendto(wire(23, sequences[device], body), destination)
                    sent += 1
                time.sleep(0.0005)
            feed_seconds = time.monotonic() - start
            cpu_end = cgroup_cpu()
            stop.set()
            process.wait(timeout=15)
            for reader in readers:
                reader.join(timeout=5)
            if process.returncode != 0 or not reports or errors:
                raise RuntimeError(f"Backend failed ({process.returncode}): {errors[-5:]}")
            summary = {"type": "runtime_benchmark", "backend": str(args.backend.resolve()),
                       "devices": args.devices, "sensors_per_device": args.sensors,
                       "logical_cpus": os.cpu_count(), "cpu_workers": args.cpu_workers,
                       "target_datagrams_per_second": target_rate, "sent_datagrams": sent,
                       "feed_seconds": feed_seconds, "actual_datagrams_per_second": sent / feed_seconds,
                       "reports": reports, "capacity_backpressure": backpressure}
            if cpu_start and cpu_end:
                used_cpus = (cpu_end["usage_usec"] - cpu_start["usage_usec"]) / 1e6 / feed_seconds
                summary["cgroup_cpu"] = {"quota_cpus": cpu_end["quota_cpus"], "average_used_cpus": used_cpus,
                                         "quota_utilization": used_cpus / cpu_end["quota_cpus"] if cpu_end["quota_cpus"] else None}
            text = json.dumps(summary, ensure_ascii=False, indent=2) + "\n"
            if args.output:
                args.output.parent.mkdir(parents=True, exist_ok=True)
                args.output.write_text(text, encoding="utf-8")
            print(text, end="")
        finally:
            stop.set()
            for worker in workers:
                worker.join(timeout=5)
                if worker.is_alive():
                    worker.terminate()
                    worker.join()
            for sock in sockets:
                sock.close()
            if process is not None:
                if process.stdin:
                    process.stdin.close()
                if process.poll() is None:
                    try:
                        process.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                for reader in readers:
                    reader.join(timeout=5)
                for stream in (process.stdout, process.stderr):
                    if stream:
                        stream.close()


if __name__ == "__main__":
    main()
