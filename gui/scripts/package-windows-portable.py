#!/usr/bin/env python3
"""Package a production Windows x64 Tauri build with the Rust backend and driver."""
import argparse
import datetime
import hashlib
import importlib.util
import json
import platform
import shutil
import struct
import tempfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
notice_spec = importlib.util.spec_from_file_location('distribution_notices', ROOT / 'gui/scripts/distribution-notices.py')
notices = importlib.util.module_from_spec(notice_spec)
notice_spec.loader.exec_module(notices)
SYSTEM_DLLS = {
    'advapi32.dll', 'bcrypt.dll', 'bcryptprimitives.dll', 'cfgmgr32.dll',
    'combase.dll', 'comctl32.dll', 'crypt32.dll', 'dbghelp.dll', 'dwmapi.dll',
    'gdi32.dll', 'hid.dll', 'imm32.dll', 'iphlpapi.dll', 'kernel32.dll',
    'msvcrt.dll', 'ntdll.dll', 'ole32.dll', 'oleaut32.dll', 'secur32.dll',
    'setupapi.dll', 'shell32.dll', 'shlwapi.dll', 'user32.dll', 'userenv.dll',
    'version.dll', 'winmm.dll', 'winspool.drv', 'ws2_32.dll', 'wtsapi32.dll',
}
VC_DLLS = ['msvcp140.dll', 'msvcp140_atomic_wait.dll',
           'vcruntime140.dll', 'vcruntime140_1.dll']


def pe_info(path):
    data = path.read_bytes()
    if data[:2] != b'MZ':
        raise ValueError(f'Not a Windows PE: {path}')
    start = struct.unpack_from('<I', data, 0x3C)[0]
    if data[start:start + 4] != b'PE\0\0':
        raise ValueError(f'Invalid Windows PE: {path}')
    machine, sections = struct.unpack_from('<HH', data, start + 4)
    optional_size = struct.unpack_from('<H', data, start + 20)[0]
    optional = start + 24
    if machine != 0x8664 or struct.unpack_from('<H', data, optional)[0] != 0x20B:
        raise ValueError(f'Expected Windows x64 image: {path}')
    section_table = optional + optional_size
    ranges = []
    for index in range(sections):
        offset = section_table + index * 40
        virtual_size, address, raw_size, raw_offset = struct.unpack_from('<IIII', data, offset + 8)
        ranges.append((address, max(virtual_size, raw_size), raw_offset))

    def resolve(rva):
        for address, size, raw_offset in ranges:
            if address <= rva < address + size:
                return raw_offset + rva - address
        raise ValueError(f'Invalid PE address {rva:x}: {path}')

    import_rva = struct.unpack_from('<I', data, optional + 120)[0]
    imports = []
    if import_rva:
        descriptor = resolve(import_rva)
        while any(data[descriptor:descriptor + 20]):
            name_rva = struct.unpack_from('<I', data, descriptor + 12)[0]
            name_offset = resolve(name_rva)
            end = data.index(b'\0', name_offset)
            imports.append(data[name_offset:end].decode('ascii').lower())
            descriptor += 20
    return {'architecture': 'x86_64',
            'subsystem': struct.unpack_from('<H', data, optional + 68)[0],
            'imports': sorted(set(imports))}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gui-exe', type=Path, required=True,
                        help='Production build; cargo cross builds must enable tauri/custom-protocol')
    parser.add_argument('--server-exe', type=Path, required=True)
    parser.add_argument('--bindings-dir', type=Path, required=True)
    parser.add_argument('--vc-runtime-dir', type=Path, required=True,
                        help='Official x64 runtime DLLs (or extracted *.dll_amd64), license and SOURCE.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--build-revision', default='Tauri-fix6-hand1')
    args = parser.parse_args()
    source = ROOT / 'gui/src-tauri/resources/drivers'
    with tempfile.TemporaryDirectory(prefix='slimevr-windows-portable-') as work:
        base = Path(work) / 'SlimeVR-Rust-Tauri'
        base.mkdir()
        shutil.copy2(args.gui_exe, base / 'SlimeVR.exe')
        shutil.copy2(args.server_exe, base / 'slimevr-server.exe')
        # MSVC embeds the loader statically; GNU builds may import its DLL.
        if 'webview2loader.dll' in pe_info(args.gui_exe)['imports']:
            shutil.copy2(args.gui_exe.parent / 'WebView2Loader.dll', base / 'WebView2Loader.dll')
        notices.copy_notices(ROOT, base, args.build_revision)
        driver = base / 'drivers/slimevr-openvr-driver-win64'
        shutil.copytree(source / 'slimevr-openvr-driver-win64', driver)
        for name in ['LICENSE-MIT', 'LICENSE-APACHE']:
            shutil.copy2(source / name, driver.parent / name)
        bindings = base / 'bindings/win64'
        bindings.mkdir(parents=True)
        for name in ['SlimeVR-Bindings-Provider.exe', 'openvr_api.dll']:
            shutil.copy2(args.bindings_dir / name, bindings / name)
        shutil.copy2(ROOT / 'bindings-provider/openvr/LICENSE', bindings.parent / 'OPENVR-LICENSE')
        licenses = base / 'licenses'
        licenses.mkdir(exist_ok=True)
        shutil.copytree(ROOT / 'server-rust/licenses', licenses / 'rust-backend', dirs_exist_ok=True)
        shutil.copy2(args.vc_runtime_dir / 'VC-Runtime-LICENSE.rtf', licenses / 'VC-Runtime-LICENSE.rtf')
        shutil.copy2(args.vc_runtime_dir / 'SOURCE.json', licenses / 'VC-Runtime-SOURCE.json')
        for name in VC_DLLS:
            candidates = [args.vc_runtime_dir / name, args.vc_runtime_dir / (name + '_amd64')]
            library = next((p for p in candidates if p.is_file()), None)
            if library is None:
                raise FileNotFoundError(f'Missing official VC runtime: {name}')
            shutil.copy2(library, base / name)
            shutil.copy2(library, driver / 'bin/win64' / name)
        (base / 'Start-SlimeVR.cmd').write_bytes(
            b'@echo off\r\nsetlocal\r\ncd /d "%~dp0"\r\n'
            b'start "" "%~dp0SlimeVR.exe" --backend rust %*\r\n')
        (base / 'Start-SlimeVR-Debug.cmd').write_bytes(
            b'@echo off\r\nsetlocal\r\ncd /d "%~dp0"\r\n'
            b'start "" "%~dp0SlimeVR.exe" --backend rust --log-level debug %*\r\n')
        readme = '''SlimeVR-Rust / Tauri — Windows x64 解压运行包

1. 把整个文件夹解压到固定位置，例如 D:\\SlimeVR-Rust-Tauri。
2. 完全退出已有界面和占用同一端口的后端，再双击 SlimeVR.exe 或 Start-SlimeVR.cmd。
3. 连接追踪器；使用 SteamVR 时按界面步骤注册 / 启用驱动。
切换后端时设备暂未出现，等待约十秒；仍未出现则重启对应设备以重新握手。

本包包含 Tauri 界面、Rust 后端、固定版本 SteamVR 驱动、Bindings Provider 和运行库。
Tauri 使用系统 WebView2 Runtime。Windows 10 用户可安装微软 Evergreen WebView2 Runtime：
https://developer.microsoft.com/microsoft-edge/webview2/
包中的 WebView2Loader.dll 是加载库，请保留；MSVC 构建采用静态加载库。

配置：%APPDATA%\\dev.slimevr.SlimeVR\\vrconfig.yml / vrconfig.yaml。
GUI 偏好和日志保存到应用数据目录。配置保存保留未知字段并生成 .bak。
日志：%APPDATA%\\dev.slimevr.SlimeVR\\logs\\gui-tauri.log，默认 info。
排查时完全退出，再运行 Start-SlimeVR-Debug.cmd。正常启动恢复默认级别。
日志按约十 MiB 轮转，保留当前文件和四个历史文件；收集时压缩整个 logs 文件夹。
详细步骤见 docs/rust-logging.zh-CN.md。

SteamVR 注册使用解压目录中的驱动路径，注册后保持目录位置。
防火墙提示出现时允许私有网络，以便接收追踪器 UDP 数据。
构建提交、文件哈希与资源来源见 BUILD-MANIFEST.json、SOURCE-CODE.txt 和 licenses/。
打包检查覆盖 PE / DLL、生产网页资源、许可与 ZIP 完整性；实机按随包清单验收。
参考源码 SlimeVR-Server：83941fd38e91cc91ca6b360deab5c2ae986dd1b6。
'''
        (base / '使用说明.txt').write_text(readme, encoding='utf-8-sig')
        notices.copy_documentation(ROOT, base, args.build_revision)
        files = []
        for path in sorted(base.rglob('*')):
            if not path.is_file():
                continue
            item = {'path': path.relative_to(base).as_posix(), 'bytes': path.stat().st_size,
                    'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
            if path.suffix.lower() in ['.exe', '.dll']:
                item['pe'] = pe_info(path)
                for dll in item['pe']['imports']:
                    if dll in SYSTEM_DLLS or dll.startswith(('api-ms-win-', 'ext-ms-win-')):
                        continue
                    available = {p.name.lower() for directory in [path.parent, base]
                                 for p in directory.iterdir() if p.is_file()}
                    if dll not in available:
                        raise ValueError(f'Unbundled DLL {dll} required by {item["path"]}')
            files.append(item)
        manifest = {'app_version': '0.1.0', 'target': 'Windows 11 x64',
                    'build_revision': args.build_revision,
                    'built_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    'build_host': platform.system(), 'webview2_runtime_bundled': False,
                    'windows_native_execution_tested': False,
                    'reference_commit': '83941fd38e91cc91ca6b360deab5c2ae986dd1b6',
                    'vc_runtime_source': json.loads((licenses / 'VC-Runtime-SOURCE.json').read_text()),
                    'files': files}
        (base / 'BUILD-MANIFEST.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        args.output.parent.mkdir(parents=True, exist_ok=True)
        with zipfile.ZipFile(args.output, 'w', zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
            for path in sorted(base.rglob('*')):
                if path.is_file():
                    archive.write(path, path.relative_to(base.parent).as_posix())
        with zipfile.ZipFile(args.output) as archive:
            if archive.testzip() is not None:
                raise ValueError('ZIP integrity check failed')
        digest = hashlib.sha256(args.output.read_bytes()).hexdigest()
        args.output.with_suffix('.zip.sha256').write_text(f'{digest}  {args.output.name}\n', encoding='ascii')
        print(json.dumps({'zip': str(args.output), 'bytes': args.output.stat().st_size,
                          'files': len(files) + 1, 'sha256': digest}, indent=2))


if __name__ == '__main__':
    main()
