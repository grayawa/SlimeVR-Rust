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
        readme = '''SlimeVR Rust / Tauri — Windows 11 x64 解压运行包

1. 把整个文件夹解压到固定位置，例如 D:\\SlimeVR-Rust-Tauri。
2. 完全退出旧 SlimeVR 界面和后端，再双击 SlimeVR.exe 或 Start-SlimeVR.cmd。
3. 连接追踪器；使用 SteamVR 时按界面步骤注册 / 启用驱动。
切换后端后设备暂未出现时，等待约 10 秒；仍未出现则重启对应追踪器，让它重新握手。

本包包含 Tauri 界面、Rust 后端、官方 SteamVR 驱动、Bindings Provider 和驱动需要的 VC++ DLL。
本包由 SlimeVR AIO 从同一提交构建，具体提交与文件校验值见 BUILD-MANIFEST.json。
使用 Windows 11 系统已有的 WebView2；未附 WebView2 Runtime。无需安装 Java、Node 或 Rust。
若包中附带 WebView2Loader.dll，请保留它；MSVC 构建将加载库静态链接。

配置沿用 %APPDATA%\\dev.slimevr.SlimeVR\\vrconfig.yml / vrconfig.yaml。
已有配置会被读取；保存保留原有字段并生成 .bak。GUI 偏好和日志也在相应应用目录。
日志路径：%APPDATA%\\dev.slimevr.SlimeVR\\logs\\gui-tauri.log。
默认记录 info / warn / error。排查时先完全退出，再双击 Start-SlimeVR-Debug.cmd。
调试启动会额外记录姿态和设备快照；再次正常启动恢复默认级别。
单个日志约 10 MiB，保留当前文件和 4 个历史文件。提交日志时请压缩整个 logs 文件夹。
详见 日志说明.md。
这是免安装包，配置仍按原 SlimeVR 路径保存，不要求放在解压目录内。

SteamVR 会登记解压目录里的驱动路径，注册后请保持该目录位置不变。
Windows 防火墙提示出现时允许私有网络访问，以便接收追踪器 UDP 数据。

本包已验证：Windows x64 交叉编译、PE 架构 / DLL 依赖、生产网页资源嵌入及 ZIP 完整性。
本环境未执行 Windows 11 实机运行。真实追踪器 / SteamVR 验收请按随包测试清单执行。
代码来源 SlimeVR-Server 83941fd38e91cc91ca6b360deab5c2ae986dd1b6 加当前 Rust / Tauri 改动。
构建与第三方来源、文件校验值见 BUILD-MANIFEST.json 和 licenses/。
'''
        (base / '使用说明.txt').write_text(readme, encoding='utf-8-sig')
        shutil.copy2(ROOT / 'docs/rust-unified-hardware-test.zh-CN.md', base / '实机测试清单.md')
        shutil.copy2(ROOT / 'docs/rust-windows-fix1.zh-CN.md', base / '修复说明-fix1.md')
        shutil.copy2(ROOT / 'docs/rust-windows-fix2.zh-CN.md', base / '修复说明-fix2.md')
        shutil.copy2(ROOT / 'docs/rust-windows-fix3.zh-CN.md', base / '修复说明-fix3.md')
        shutil.copy2(ROOT / 'docs/rust-windows-fix4.zh-CN.md', base / '修复说明-fix4.md')
        shutil.copy2(ROOT / 'docs/rust-logging.zh-CN.md', base / '日志说明.md')
        shutil.copy2(ROOT / 'docs/rust-windows-fix5.zh-CN.md', base / '更新说明-fix5.md')
        shutil.copy2(ROOT / 'docs/rust-steamvr-hand-handover.zh-CN.md', base / '手部切换修复说明.md')
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
