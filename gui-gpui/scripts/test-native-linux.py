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
            window = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"GPUI exited with {process.returncode}; see native.log")
                tree = subprocess.check_output(["xwininfo", "-root", "-tree"], text=True)
                for line in tree.splitlines():
                    if '"SlimeVR' in line and "Components" in line:
                        window = line.split()[0]
                        break
                if window:
                    break
                time.sleep(0.2)
            if window is None:
                raise RuntimeError("Native GPUI window creation timed out")
            time.sleep(2)
            screenshot = args.output / "components.png"
            subprocess.run(["import", "-window", window, str(screenshot)], check=True)
            colors = int(subprocess.check_output(["identify", "-format", "%k", str(screenshot)], text=True))
            if colors < 32:
                raise RuntimeError(f"Expected rendered text and controls; captured {colors} colors")
            print(f"Native X11 rendering passed: {colors} colors; {screenshot}")
        finally:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()


if __name__ == "__main__":
    main()
