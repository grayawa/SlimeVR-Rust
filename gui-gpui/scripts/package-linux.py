#!/usr/bin/env python3
"""Package a Linux GPUI desktop, backend and pinned SteamVR resources."""
import argparse
import datetime
import hashlib
import importlib.util
import json
import platform
from pathlib import Path
import re
import shutil
import struct
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "notices", ROOT / "gui/scripts/distribution-notices.py"
)
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)
TARGETS = {"x64": (62, "x64-linux", "linux64"), "ARM64": (183, "aarch64-linux", "linuxarm64")}


def elf_machine(path):
    header = Path(path).read_bytes()[:20]
    if len(header) < 20 or header[:6] != b"\x7fELF\x02\x01":
        raise ValueError(f"Expected a 64-bit little-endian ELF: {path}")
    return struct.unpack_from("<H", header, 18)[0]


def copy_resources(source, destination):
    source = Path(source)
    if not source.is_dir():
        raise ValueError(f"Missing resources: {source}")
    # Resource trees stay within their recorded source directory.
    for path in source.rglob("*"):
        if path.is_symlink():
            raise ValueError(f"Resource symlink requires an explicit packaging rule: {path}")
    shutil.copytree(source, destination)


def build_package(args):
    machine, driver_platform, bindings_platform = TARGETS[args.arch]
    driver_dir = args.driver_dir or ROOT / f"gui/src-tauri/resources/drivers/slimevr-openvr-driver-{driver_platform}"
    bindings_dir = args.bindings_dir or ROOT / f"gui/src-tauri/resources/bindings/{bindings_platform}"
    with tempfile.TemporaryDirectory(prefix="slimevr-linux-package-") as temporary:
        base = Path(temporary) / "SlimeVR-Rust-GPUI"
        base.mkdir()
        for source, name in [(args.gui, "slimevr-gpui"), (args.server, "slimevr-server"),
                             (args.components, "slimevr-gpui-components"), (args.probe, "slimevr-gpui-probe")]:
            if elf_machine(source) != machine:
                raise ValueError(f"ELF architecture mismatch: {source}")
            shutil.copy2(source, base / name)
            (base / name).chmod(0o755)
        driver = base / f"drivers/slimevr-openvr-driver-{driver_platform}"
        bindings = base / f"bindings/{bindings_platform}"
        copy_resources(driver_dir, driver)
        copy_resources(bindings_dir, bindings)
        if json.loads((driver / "driver.vrdrivermanifest").read_text())["name"] != "slimevr":
            raise ValueError("Expected the SlimeVR driver manifest")
        helper = bindings / "slimevr-bindings-provider"
        helper.chmod(0o755)
        for path in [helper, bindings / "libopenvr_api.so", driver / f"bin/{bindings_platform}/driver_slimevr.so"]:
            if elf_machine(path) != machine:
                raise ValueError(f"ELF architecture mismatch: {path}")
        notices.copy_notices(ROOT, base, args.build_revision)
        for name in ["LICENSE-MIT", "LICENSE-APACHE"]:
            shutil.copy2(driver_dir.parent / name, base / "drivers" / name)
        shutil.copy2(bindings_dir.parent / "OPENVR-LICENSE", base / "licenses/OpenVR-LICENSE")
        shutil.copy2(ROOT / "gui-gpui/assets/GPUI-Kit-LICENSE-APACHE", base / "licenses/GPUI-Kit-LICENSE-APACHE")
        shutil.copy2(ROOT / "gui-gpui/README.zh-CN.md", base / "原生前端说明.md")
        shutil.copy2(ROOT / "gui/src-tauri/resources/69-slimevr-devices.rules", base / "69-slimevr-devices.rules")
        (base / "Start-SlimeVR.sh").write_text(
            '#!/bin/sh\nset -eu\ncd -- "$(dirname -- "$0")"\nexec ./slimevr-gpui "$@"\n', encoding="utf-8"
        )
        (base / "Start-SlimeVR.sh").chmod(0o755)
        (base / "使用说明.txt").write_text(
            "SlimeVR-Rust / GPUI — Linux 独立开发预览\n\n"
            "使用 Ubuntu 24.04+ 或提供等价动态库的发行版，解压到固定目录后执行 ./Start-SlimeVR.sh。\n"
            "程序会启动随包 Rust 后端；本机已有后端时连接该服务。\n"
            "系统需要 Vulkan 驱动、ALSA、Fontconfig、X11/Wayland、桌面 D-Bus 会话。\n"
            "托盘使用 StatusNotifier；GNOME 用户可以启用 AppIndicator 扩展。\n"
            "USB/HID 设备需要权限规则时，执行 sudo install -m 644 69-slimevr-devices.rules /etc/udev/rules.d/，\n"
            "再执行 sudo udevadm control --reload-rules，随后重新插入设备。\n"
            "SteamVR 配合 Linux SlimeVR 驱动输出姿态。真实头显跟踪按实机清单验收。\n"
            "配置和日志位于 ${XDG_CONFIG_HOME:-$HOME/.config}/dev.slimevr.SlimeVR/。\n"
            "slimevr-gpui-components 提供内存示例组件预览；slimevr-gpui-probe 提供诊断。\n"
            "可使用 ./Start-SlimeVR.sh --attach 或 --log-level debug。\n"
            "源码提交、许可证与文件校验见 SOURCE-CODE.txt、licenses/ 和 BUILD-MANIFEST.json。\n", encoding="utf-8"
        )
        files = []
        for path in sorted(base.rglob("*")):
            if not path.is_file():
                continue
            content = path.read_bytes()
            entry = {"path": path.relative_to(base).as_posix(), "bytes": len(content),
                     "sha256": hashlib.sha256(content).hexdigest(), "mode": oct(path.stat().st_mode & 0o777)}
            if content.startswith(b"\x7fELF"):
                entry["elf_machine"] = elf_machine(path)
                if entry["elf_machine"] != machine:
                    raise ValueError(f"Mixed ELF architectures: {path}")
                if args.audit_system_libs:
                    output = subprocess.run(["ldd", str(path)], capture_output=True, text=True)
                    if output.returncode or "not found" in output.stdout:
                        raise ValueError(f"Unresolved libraries for {entry['path']}: {output.stdout}{output.stderr}")
                    entry["needed"] = sorted(set(re.findall(r"^\s*(\S+)\s+=>", output.stdout, re.M)))
            files.append(entry)
        manifest = {"app_version": "0.1.0", "build_revision": args.build_revision,
                    "target": f"Linux {args.arch}",
                    "build_host": platform.freedesktop_os_release().get("PRETTY_NAME", "Linux"),
                    "build_libc": dict(zip(["name", "version"], platform.libc_ver())),
                    "built_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    "system_libraries_audited": args.audit_system_libs, "files": files}
        (base / "BUILD-MANIFEST.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with tarfile.open(args.output, "w:gz") as archive:
            archive.add(base, arcname=base.name)
    with tarfile.open(args.output, "r:gz") as archive:
        for member in archive.getmembers():
            if member.isfile():
                archive.extractfile(member).read()
    digest = hashlib.sha256(args.output.read_bytes()).hexdigest()
    args.output.with_suffix(args.output.suffix + ".sha256").write_text(f"{digest}  {args.output.name}\n")
    return {"path": str(args.output), "bytes": args.output.stat().st_size, "sha256": digest}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["gui", "server", "components", "probe", "output"]:
        parser.add_argument(f"--{name}", type=Path, required=True)
    parser.add_argument("--driver-dir", type=Path)
    parser.add_argument("--bindings-dir", type=Path)
    parser.add_argument("--arch", choices=TARGETS, default="x64")
    parser.add_argument("--build-revision", required=True)
    parser.add_argument("--audit-system-libs", action="store_true")
    print(json.dumps(build_package(parser.parse_args())))


if __name__ == "__main__":
    main()
