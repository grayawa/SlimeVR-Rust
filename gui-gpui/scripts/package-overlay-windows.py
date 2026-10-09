#!/usr/bin/env python3
"""Package the dashboard addon; no backend, driver installer or personal data."""
import argparse
import datetime
import hashlib
import importlib.util
import json
import shutil
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('gui_package', Path(__file__).with_name('package-windows.py'))
gui = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gui)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--overlay-exe', type=Path, required=True)
    parser.add_argument('--openvr-dll', type=Path, default=ROOT / 'bindings-provider/openvr/bin/win64/openvr_api.dll')
    parser.add_argument('--baseline', type=Path, help='Audited prior package; copy only runtime DLLs and their licenses')
    parser.add_argument('--vc-runtime-dir', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--build-revision', default='dashboard')
    args = parser.parse_args()
    shaders = gui.shader_audit.audit(args.overlay_exe)
    with tempfile.TemporaryDirectory(prefix='slimevr-dashboard-') as work:
        base = Path(work) / 'SlimeVR-Overlay'
        licenses = base / 'licenses'
        licenses.mkdir(parents=True)
        if args.baseline:
            with zipfile.ZipFile(args.baseline) as archive:
                if archive.testzip():
                    raise ValueError('Baseline ZIP failed CRC check')
                for member in archive.infolist():
                    parts = PurePosixPath(member.filename).parts
                    if len(parts) < 2 or '..' in parts or member.is_dir():
                        continue
                    name = PurePosixPath(*parts[1:])
                    if (len(name.parts) == 1 and name.name in gui.portable.VC_DLLS) or name.as_posix() in {
                        'licenses/VC-Runtime-LICENSE.rtf', 'licenses/VC-Runtime-SOURCE.json'
                    }:
                        target = base / str(name)
                        target.write_bytes(archive.read(member))
        elif args.vc_runtime_dir:
            for name in gui.portable.VC_DLLS:
                source = args.vc_runtime_dir / name
                if not source.is_file():
                    source = args.vc_runtime_dir / (name + '_amd64')
                shutil.copy2(source, base / name)
            shutil.copy2(args.vc_runtime_dir / 'VC-Runtime-LICENSE.rtf', licenses)
            shutil.copy2(args.vc_runtime_dir / 'SOURCE.json', licenses / 'VC-Runtime-SOURCE.json')
        else:
            raise ValueError('Provide --baseline or --vc-runtime-dir with licensed runtime DLLs')
        for name in ['VC-Runtime-LICENSE.rtf', 'VC-Runtime-SOURCE.json']:
            if not (licenses / name).is_file():
                raise ValueError(f'Missing runtime license/provenance: {name}')
        shutil.copy2(args.overlay_exe, base / 'SlimeVR-Overlay.exe')
        shutil.copy2(args.openvr_dll, base / 'openvr_api.dll')
        gui.portable.notices.copy_notices(ROOT, base, args.build_revision)
        shutil.copytree(ROOT / 'server-rust/licenses', licenses / 'rust-backend', dirs_exist_ok=True)
        shutil.copy2(ROOT / 'bindings-provider/openvr/LICENSE', licenses / 'OpenVR-LICENSE')
        shutil.copy2(ROOT / 'gui-gpui/assets/GPUI-Kit-LICENSE-APACHE', licenses)
        vendor = ROOT / 'gui-gpui/vendor/gpui-pre-windows'
        shutil.copy2(vendor / 'LICENSE-APACHE', licenses / 'GPUI-Windows-LICENSE-APACHE')
        shutil.copy2(vendor / 'SLIMEVR-PATCH.md', licenses / 'GPUI-Windows-PATCH.md')
        shutil.copytree(ROOT / 'gui-gpui/assets/fonts', licenses / 'fonts', ignore=shutil.ignore_patterns('*.ttf'), dirs_exist_ok=True)
        shutil.copy2(ROOT / 'docs/rust-steamvr-dashboard.zh-CN.md', base / '面板说明与测试.md')
        for name, flags in [('Start-Overlay.cmd', ''), ('Preview-Overlay.cmd', '--preview'), ('Demo-Overlay.cmd', '--demo')]:
            (base / name).write_bytes((
                '@echo off\r\nsetlocal\r\ncd /d "%~dp0"\r\n'
                f'start "" "%~dp0SlimeVR-Overlay.exe" {flags} %*\r\n'
            ).encode('ascii'))
        (base / '使用说明.txt').write_text(
            'SlimeVR SteamVR 仪表盘附加程序\n\n'
            '1. 解压整个文件夹；先启动现有 SlimeVR-Rust 程序并连接追踪器。\n'
            '2. 启动 SteamVR，再双击 Start-Overlay.cmd。\n'
            '3. 按手柄系统键打开 SteamVR 仪表盘，选择 SlimeVR。\n'
            '4. 可操作完整 / 航向 / 安装方向重置，查看骨架及节点信息。\n\n'
            '面板连接已运行的后端，驱动与追踪生命周期由后端管理。\n'
            'Demo-Overlay.cmd 只显示示例数据；Preview-Overlay.cmd 同时显示桌面预览。\n'
            '界面使用 GPUI 原生渲染器，沿用桌面语言与主题；--locale zh-Hans 指定中文。\n'
            '日志：%APPDATA%\\dev.slimevr.SlimeVR\\logs\\overlay\\gui-gpui.log。\n'
            '功能和实机验证范围详见面板说明与测试.md。\n',
            encoding='utf-8-sig',
        )
        files = []
        for path in sorted(base.rglob('*')):
            if not path.is_file():
                continue
            entry = {'path': path.relative_to(base).as_posix(), 'bytes': path.stat().st_size,
                     'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
            if path.suffix.lower() in {'.exe', '.dll'}:
                entry['pe'] = gui.portable.pe_info(path)
                for dll in entry['pe']['imports']:
                    if dll in gui.SYSTEM or dll.startswith(('api-ms-win-', 'ext-ms-win-')):
                        continue
                    if dll not in {f.name.lower() for f in base.iterdir() if f.is_file()}:
                        raise ValueError(f'Missing DLL {dll}: {entry["path"]}')
            files.append(entry)
        if gui.portable.pe_info(base / 'SlimeVR-Overlay.exe')['subsystem'] != 2:
            raise ValueError('Expected a Windows GUI executable')
        manifest = {'build_revision': args.build_revision, 'target': 'Windows x64',
                    'built_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    'shaders': shaders, 'backend_included': False, 'webview2_required': False,
                    'windows_native_execution_tested': False, 'steamvr_headset_execution_tested': False,
                    'baseline_sha256': hashlib.sha256(args.baseline.read_bytes()).hexdigest() if args.baseline else None,
                    'files': files}
        (base / 'BUILD-MANIFEST.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.output, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path in sorted(base.rglob('*')):
                if path.is_file():
                    archive.write(path, path.relative_to(base.parent).as_posix())
    with zipfile.ZipFile(args.output) as archive:
        if archive.testzip():
            raise ValueError('Generated ZIP failed CRC check')
    digest = hashlib.sha256(args.output.read_bytes()).hexdigest()
    args.output.with_suffix('.zip.sha256').write_text(f'{digest}  {args.output.name}\n')
    print(json.dumps({'path': str(args.output), 'bytes': args.output.stat().st_size, 'sha256': digest}))


if __name__ == '__main__':
    main()
