#!/usr/bin/env python3
"""Compile the repository's actual Kotlin parser and math, then generate synthetic golden data.

Only TrackerPosition lookup and unused UDPDevice dependencies use compile stubs.
This does NOT run the full Java server or claim hardware compatibility.
Requires java and Maven Central access on first run; cargo test needs neither.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
from reference_sources import reference_checkout, source_path
import struct
import subprocess
import tempfile
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
REFERENCE_ROOT, REFERENCE_COMMIT = reference_checkout(ROOT)
SOURCE = REFERENCE_ROOT / "server/core/src/main/java"
ARTIFACTS = [
    ("org.jetbrains.kotlin", "kotlin-compiler-embeddable", "2.2.0"),
    ("org.jetbrains.kotlin", "kotlin-stdlib", "2.2.0"),
    ("org.jetbrains.kotlin", "kotlin-script-runtime", "2.2.0"),
    ("org.jetbrains.kotlin", "kotlin-reflect", "1.6.10"),
    ("org.jetbrains.kotlin", "kotlin-daemon-embeddable", "2.2.0"),
    ("org.jetbrains.kotlinx", "kotlinx-coroutines-core-jvm", "1.8.0"),
    ("org.jetbrains.kotlinx", "kotlinx-serialization-core-jvm", "1.9.0"),
    ("org.jetbrains", "annotations", "13.0"),
]


def packet(id, payload=b"", seq=1):
    return struct.pack(">Iq", id, seq) + payload


def cases():
    def rot(sensor=0, values=(0.1, -0.2, 0.3, 0.9), kind=1):
        return bytes([sensor, kind]) + struct.pack(">4f", *values) + b"\x03"
    handshake = struct.pack(">7I", 9, 13, 1, 0, 0, 0, 22) + b"\x05good\0\x02\x00\x00\x00\x00\x01"
    packed = bytes([1]) + struct.pack(">7h", -8192, 16384, 4096, 27000, -128, 256, -384)
    body = struct.pack(">I", 17) + rot(0)
    body2 = struct.pack(">I", 20) + b"\x01" + struct.pack(">f", 36.5)
    compact1 = bytes([23]) + packed
    compact2 = bytes([17]) + rot(0)
    return [
        ("vendor_good_protocol22", packet(3, handshake)),
        ("legacy_empty_handshake", packet(3)),
        ("sensor_info_full", packet(15, bytes([1, 1, 13, 0, 3, 1, 7, 0]))),
        ("sensor_info_optional", packet(15, bytes([0, 1]))),
        ("rotation_float_17", packet(17, rot())),
        ("rotation_correction_17", packet(17, rot(kind=2))),
        ("rotation_legacy_1", packet(1, struct.pack(">4f", 0.1, -0.2, 0.3, 0.9))),
        ("rotation_extension_16", packet(16, struct.pack(">4f", 0, 0, 0, 1))),
        ("rotation_compact_q15_accel_q7", packet(23, packed)),
        ("zero_float_identity_fallback", packet(17, rot(values=(0, 0, 0, 0)))),
        ("nan_float_identity_fallback", packet(17, rot(values=(float('nan'), 0, 0, 1)))),
        ("acceleration_sensor_1", packet(4, struct.pack(">3fB", 1, -2, 3, 1))),
        ("acceleration_legacy_optional_id", packet(4, struct.pack(">3f", 1, -2, 3))),
        ("nan_acceleration_zero_fallback", packet(4, struct.pack(">3f", float('nan'), 1, 2))),
        ("battery_voltage_and_fraction", packet(12, struct.pack(">2f", 3.85, 0.75))),
        ("battery_fraction_only", packet(12, struct.pack(">f", 0.5))),
        ("rssi_signed", packet(19, bytes([0, 185]))),
        ("temperature", packet(20, b"\0" + struct.pack(">f", 36.5))),
        ("ping_signed", packet(10, struct.pack(">i", -1234))),
        ("user_action", packet(21, b"\x03")),
        ("tap", packet(13, b"\x01\x40")),
        ("sensor_error", packet(14, b"\x01\x02")),
        ("config_ack", packet(24, struct.pack(">BH", 1, 300))),
        ("flex", packet(26, b"\x01" + struct.pack(">f", 0.6))),
        ("position", packet(27, b"\0" + struct.pack(">3f", 1, 2, 3))),
        ("protocol_change", packet(200, b"\x01\x02")),
        ("heartbeat", packet(0)),
        ("bundle_normal", packet(100, struct.pack(">H", len(body)) + body + struct.pack(">H", len(body2)) + body2)),
        ("bundle_compact", packet(101, b"\0" + bytes([len(compact1)]) + compact1 + bytes([len(compact2)]) + compact2)),
    ]


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("--cache", type=Path, default=Path(tempfile.gettempdir()) / "slimevr-udp-oracle")
    args.add_argument("--output", type=Path, default=HERE.parent / "crates/slimevr-server/tests/fixtures/udp-golden.json")
    options = args.parse_args()
    lib = options.cache / "lib"
    lib.mkdir(parents=True, exist_ok=True)

    def fetch(item):
        group, artifact, version = item
        path = lib / f"{artifact}-{version}.jar"
        if not path.exists():
            urllib.request.urlretrieve(f"https://repo.maven.apache.org/maven2/{group.replace('.', '/')}/{artifact}/{version}/{path.name}", path)
        return path

    with ThreadPoolExecutor(max_workers=4) as pool:
        list(pool.map(fetch, ARTIFACTS))
    sources = [SOURCE / f"dev/slimevr/tracking/trackers/udp/{name}.kt" for name in ("UDPProtocolParser", "UDPPacket", "FirmwareConstants", "FeatureFlags", "SensorTap")]
    sources += [SOURCE / "dev/slimevr/tracking/trackers/TrackerStatus.kt"]
    sources += sorted((SOURCE / "io/github/axisangles/ktmath").glob("*.kt"))
    sources += sorted((HERE / "reference").glob("*.kt"))
    classes = options.cache / "classes"
    import os
    dependencies = os.pathsep.join(str(p) for p in sorted(lib.glob("*.jar")))
    subprocess.run(["java", "-cp", dependencies, "org.jetbrains.kotlin.cli.jvm.K2JVMCompiler", "-Xvalue-classes", "-no-stdlib", "-no-reflect", "-jvm-target", "17", "-classpath", dependencies, "-d", str(classes), *map(str, sources)], check=True)
    selected = cases()
    result = subprocess.run(["java", "-cp", str(classes) + os.pathsep + dependencies, "OracleKt"], input="".join(data.hex() + "\n" for _, data in selected), text=True, capture_output=True, check=True)
    lines = [json.loads(line) for line in result.stdout.splitlines()]
    assert len(lines) == len(selected) + 1
    fixture = {
        "reference_commit": REFERENCE_COMMIT,
        "scope": "Actual Kotlin UDPProtocolParser, UDPPacket and ktmath; TrackerPosition lookup and unused UDPDevice compile stubs. Synthetic inputs, not hardware recordings.",
        "source_sha256": {source_path(p, ROOT, REFERENCE_ROOT): hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
        "replies": lines[0],
        "cases": [{"name": name, "hex": data.hex(), **expected} for (name, data), expected in zip(selected, lines[1:])],
    }
    options.output.parent.mkdir(parents=True, exist_ok=True)
    options.output.write_text(json.dumps(fixture, indent=2, ensure_ascii=False) + "\n")
    print(f"Generated {len(selected)} cases: {options.output}")


if __name__ == "__main__":
    main()
