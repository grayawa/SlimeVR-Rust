#!/usr/bin/env python3
"""Stage installer licenses and the checked-out build's source references."""
import hashlib
import importlib.util
import json
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    'distribution_notices', Path(__file__).with_name('distribution-notices.py'))
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)


def prepare(root=ROOT):
    root = Path(root)
    revision = subprocess.check_output(
        ['git', 'rev-parse', '--verify', 'HEAD'], cwd=root, text=True).strip()
    changed = subprocess.check_output(
        ['git', 'status', '--porcelain', '--untracked-files=normal'],
        cwd=root, text=True)
    submodules = subprocess.check_output(
        ['git', 'submodule', 'status', '--recursive'], cwd=root, text=True)
    lockfiles = {}
    for name in ('server-rust/Cargo.lock', 'gui/src-tauri/Cargo.lock', 'pnpm-lock.yaml'):
        path = root / name
        lockfiles[name] = hashlib.sha256(path.read_bytes()).hexdigest()
    destination = root / 'gui/src-tauri/resources/notices'
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as work:
        staged = Path(work) / 'notices'
        notices.copy_notices(root, staged, revision)
        (staged / 'BUILD-SOURCE.json').write_text(json.dumps({
            'commit': revision,
            'local_changes': bool(changed.strip()),
            'submodule_status': submodules.splitlines(),
            'lockfile_sha256': lockfiles,
        }, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        if destination.exists():
            shutil.rmtree(destination)
        shutil.move(str(staged), destination)
    return destination


if __name__ == '__main__':
    print(prepare())
