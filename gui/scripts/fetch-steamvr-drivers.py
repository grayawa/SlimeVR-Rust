#!/usr/bin/env python3
"""Stage the official SlimeVR OpenVR driver release for Tauri packaging."""
import hashlib
import io
import json
import shutil
import urllib.request
import zipfile
from pathlib import Path

VERSION = "v6.0.0"
ROOT = Path(__file__).resolve().parents[1] / "src-tauri/resources/drivers"
ARCHIVE_HASHES = {
    "win64": "6b86598f1f9285014c89447e6b82c3ef59dae428127437fd06d5b8e1054d632b",
    "x64-linux": "97e7b2210b5046a7fb4524fb4288ed8a0046dc7930ac8b4aff1143a870760dd7",
    "aarch64-linux": "f29f5a45bcaadc0d583c71a732c5abe9f80e1c93386890f9915ca94846756445",
}


def main():
    ROOT.mkdir(parents=True, exist_ok=True)
    metadata = []
    for platform, expected_hash in ARCHIVE_HASHES.items():
        name = f"slimevr-openvr-driver-{platform}"
        url = ("https://github.com/SlimeVR/SlimeVR-OpenVR-Driver/releases/download/"
               f"{VERSION}/{name}.zip")
        target = ROOT / name
        manifest = target / "driver.vrdrivermanifest"
        release = target / "release.json"
        # Flatten a nested driver staging directory to the target root.
        nested = target / "slimevr"
        if (nested / "driver.vrdrivermanifest").is_file():
            for child in nested.iterdir():
                shutil.move(str(child), str(target / child.name))
            nested.rmdir()
        cached = json.loads(release.read_text()) if release.is_file() else {}
        if not manifest.is_file() or cached.get("archive_sha256") != expected_hash:
            with urllib.request.urlopen(url, timeout=45) as response:
                data = response.read()
            digest = hashlib.sha256(data).hexdigest()
            if digest != expected_hash:
                raise ValueError(f"Driver archive hash mismatch: {name}")
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                for member in archive.infolist():
                    path = Path(member.filename)
                    if path.is_absolute() or ".." in path.parts:
                        raise ValueError("Invalid archive path")
                    if path.parts and path.parts[0] == "slimevr":
                        path = Path(*path.parts[1:])
                    output = target / path
                    if member.is_dir():
                        output.mkdir(parents=True, exist_ok=True)
                    else:
                        output.parent.mkdir(parents=True, exist_ok=True)
                        output.write_bytes(archive.read(member))
            release.write_text(json.dumps(dict(version=VERSION, url=url,
                                               archive_sha256=digest), indent=2) + "\n")
        if not manifest.is_file() or json.loads(manifest.read_text()).get("name") != "slimevr":
            raise ValueError(f"Invalid driver manifest: {manifest}")
        metadata.append(json.loads(release.read_text()))
    for name in ["LICENSE-APACHE", "LICENSE-MIT"]:
        if not (ROOT / name).is_file():
            url = f"https://raw.githubusercontent.com/SlimeVR/SlimeVR-OpenVR-Driver/{VERSION}/{name}"
            with urllib.request.urlopen(url, timeout=45) as response:
                (ROOT / name).write_bytes(response.read())
    (ROOT / "releases.json").write_text(json.dumps(metadata, indent=2) + "\n")
    print(f"Official SteamVR driver {VERSION} is staged for Tauri packaging")


if __name__ == "__main__":
    main()
