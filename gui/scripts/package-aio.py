#!/usr/bin/env python3
"""Collect verified, independently usable Windows bundles from one AIO build."""
import argparse
import hashlib
import json
import zipfile
from pathlib import Path

BUNDLES = ('SlimeVR-GPUI-Windows-x64', 'SlimeVR-Tauri-Windows-x64', 'SlimeVR-Overlay-Windows-x64')


def package(directory: Path, revision: str) -> Path:
    files = []
    for name in BUNDLES:
        path = directory / f'{name}.zip'
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        if (directory / f'{name}.zip.sha256').read_text().split()[0] != digest:
            raise ValueError(f'Checksum mismatch: {name}')
        with zipfile.ZipFile(path) as archive:
            if archive.testzip():
                raise ValueError(f'Corrupt bundle: {name}')
        files.append({'name': path.name, 'sha256': digest, 'bytes': path.stat().st_size})
    output = directory / 'SlimeVR-AIO-Windows-x64.zip'
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_STORED) as archive:
        for row in files:
            archive.write(directory / row['name'], row['name'])
            checksum = row['name'] + '.sha256'
            archive.write(directory / checksum, checksum)
        archive.writestr('BUILD-MANIFEST.json', json.dumps({'revision': revision, 'bundles': files}, indent=2) + '\n')
        archive.writestr('使用说明.txt', '\ufeffSlimeVR Windows 构建合集\n\n选择 GPUI 或 Tauri 压缩包解压，运行其中的 SlimeVR.exe。\nOverlay 是附加面板：另行解压，先启动桌面程序和 SteamVR，再运行 Start-Overlay.cmd。\nGPUI 不需要 WebView2；Tauri 使用系统 WebView2。两种桌面程序共用配置，切换前请退出当前程序。\n')
    digest = hashlib.sha256(output.read_bytes()).hexdigest()
    output.with_suffix('.zip.sha256').write_text(f'{digest}  {output.name}\n')
    return output


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    args = parser.parse_args()
    print(package(args.directory, args.revision))
