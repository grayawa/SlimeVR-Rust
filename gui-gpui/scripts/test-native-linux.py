#!/usr/bin/env python3
"""Verify a mapped native GPUI window and capture its rendered X11 contents."""
import argparse
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--components", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    environment = dict(os.environ, XDG_SESSION_TYPE="x11")
    environment.pop("WAYLAND_DISPLAY", None)
    with (args.output / "native.log").open("w") as log:
        process = subprocess.Popen([str(args.components.resolve())], env=environment, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 45
            screenshot = args.output / "components.png"
            last_status = "waiting for native window"
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"GPUI exited with {process.returncode}; see native.log")
                tree = subprocess.check_output(["xwininfo", "-root", "-tree"], text=True)
                (args.output / "window-tree.txt").write_text(tree, encoding="utf-8")
                for line in tree.splitlines():
                    if '"SlimeVR' in line and "Components" in line:
                        window = line.split()[0]
                        state = subprocess.run(["xwininfo", "-id", window], capture_output=True, text=True)
                        if state.returncode or "Map State: IsViewable" not in state.stdout:
                            last_status = f"{window} awaiting window mapping"
                            continue
                        capture = subprocess.run(["import", "-window", window, str(screenshot)], capture_output=True, text=True)
                        if capture.returncode:
                            last_status = capture.stderr.strip()
                            continue
                        colors = int(subprocess.check_output(["identify", "-format", "%k", str(screenshot)], text=True))
                        if colors >= 32:
                            print(f"Native X11 rendering passed: {colors} colors; {screenshot}")
                            return
                        last_status = f"{window} awaiting rendered controls ({colors} colors)"
                time.sleep(0.2)
            raise RuntimeError(f"Native GPUI rendering timed out: {last_status}; see native.log and window-tree.txt")
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == "__main__":
    main()
