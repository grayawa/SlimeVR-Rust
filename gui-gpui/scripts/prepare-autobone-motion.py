#!/usr/bin/env python3
"""Prepare GPUI's animated WebP from the upstream AutoBone demonstration."""
from pathlib import Path
import subprocess


def main():
    root = Path(__file__).resolve().parents[2]
    output = root / "gui-gpui/assets/autobone-motion.webp"
    subprocess.run([
        "ffmpeg", "-hide_banner", "-loglevel", "error",
        "-i", str(root / "gui/public/videos/autobone.webm"),
        "-vf", "fps=8,scale=144:-2", "-c:v", "libwebp_anim",
        "-lossless", "0", "-quality", "65", "-loop", "0", "-y", str(output),
    ], check=True)
    output.chmod(0o644)


if __name__ == "__main__":
    main()
