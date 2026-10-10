#!/usr/bin/env python3
"""Package the native Windows GUI with audited backend/SteamVR components."""
import argparse,datetime,hashlib,importlib.util,json,platform,shutil,tempfile,zipfile
from pathlib import Path,PurePosixPath
ROOT=Path(__file__).resolve().parents[2]
spec=importlib.util.spec_from_file_location('portable',ROOT/'gui/scripts/package-windows-portable.py');portable=importlib.util.module_from_spec(spec);spec.loader.exec_module(portable)
shader_spec=importlib.util.spec_from_file_location('shader_audit',ROOT/'gui-gpui/scripts/audit-windows-shaders.py');shader_audit=importlib.util.module_from_spec(shader_spec);shader_spec.loader.exec_module(shader_audit)
SYSTEM=portable.SYSTEM_DLLS|{'d3d11.dll','d3dcompiler_47.dll','dcomp.dll','dwrite.dll','dxgi.dll','gdiplus.dll','icuuc.dll','propsys.dll','uiautomationcore.dll','ktmw32.dll','rpcrt4.dll'}
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--gui-exe',type=Path,required=True);p.add_argument('--components-exe',type=Path);p.add_argument('--server-exe',type=Path);p.add_argument('--probe-exe',type=Path);p.add_argument('--baseline',type=Path);p.add_argument('--driver-dir',type=Path,default=ROOT/'gui/src-tauri/resources/drivers/slimevr-openvr-driver-win64');p.add_argument('--bindings-dir',type=Path,default=ROOT/'gui/src-tauri/resources/bindings/win64');p.add_argument('--vc-runtime-dir',type=Path);p.add_argument('--build-revision',default='GPUI-test14-components1');p.add_argument('--output',type=Path,required=True);args=p.parse_args()
 with tempfile.TemporaryDirectory(prefix='slimevr-gpui-package-')as work:
  base=Path(work)/'SlimeVR-Rust-GPUI';base.mkdir();licenses=base/'licenses';licenses.mkdir()
  if args.baseline:
   with zipfile.ZipFile(args.baseline)as archive:
    if archive.testzip():raise ValueError('Baseline ZIP failed CRC check')
    for member in archive.infolist():
     parts=PurePosixPath(member.filename).parts
     if len(parts)<2 or '..'in parts or member.is_dir():continue
     name=PurePosixPath(*parts[1:]);allowed=name.parts[0]in ['bindings','drivers','licenses']or(len(name.parts)==1 and name.suffix.lower()=='.dll'and name.name.lower()!='webview2loader.dll')or name.as_posix()=='slimevr-server.exe'
     if allowed:
      target=base/str(name);target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(archive.read(member))
  else:
   shutil.copytree(args.driver_dir,base/'drivers/slimevr-openvr-driver-win64')
   for name in ['LICENSE-MIT','LICENSE-APACHE']:
    source=args.driver_dir.parent/name
    if source.is_file():shutil.copy2(source,base/'drivers'/name)
   shutil.copytree(args.bindings_dir,base/'bindings/win64')
   openvr=args.bindings_dir.parent/'OPENVR-LICENSE'
   if openvr.is_file():shutil.copy2(openvr,licenses/'OpenVR-LICENSE')
   if not args.vc_runtime_dir:raise ValueError('--vc-runtime-dir is required without a baseline')
   for name in portable.VC_DLLS:
    source=args.vc_runtime_dir/name
    if not source.is_file():source=args.vc_runtime_dir/(name+'_amd64')
    shutil.copy2(source,base/name)
   for name in ['VC-Runtime-LICENSE.rtf','SOURCE.json']:
    source=args.vc_runtime_dir/name
    if source.is_file():shutil.copy2(source,licenses/('VC-Runtime-'+name if name=='SOURCE.json'else name))
  shaders=shader_audit.audit(args.gui_exe)
  preview_shaders=None
  if args.components_exe:
   preview_shaders=shader_audit.audit(args.components_exe)
   shutil.copy2(args.components_exe,base/'SlimeVR-Components.exe')
  shutil.copy2(args.gui_exe,base/'SlimeVR.exe')
  if args.server_exe:shutil.copy2(args.server_exe,base/'slimevr-server.exe')
  if not(base/'slimevr-server.exe').is_file():raise ValueError('A backend executable is required')
  if args.probe_exe:shutil.copy2(args.probe_exe,base/'slimevr-gpui-probe.exe')
  portable.notices.copy_notices(ROOT,base,args.build_revision)
  shutil.copytree(ROOT/'server-rust/licenses',licenses/'rust-backend',dirs_exist_ok=True)
  shutil.copytree(ROOT/'gui-gpui/assets/fonts',licenses/'fonts',ignore=shutil.ignore_patterns('*.ttf'),dirs_exist_ok=True)
  kit_license=ROOT/'gui-gpui/assets/GPUI-Kit-LICENSE-APACHE'
  if kit_license.is_file():shutil.copy2(kit_license,licenses/kit_license.name)
  shutil.copy2(ROOT/'gui-gpui/vendor/gpui-pre-windows/LICENSE-APACHE',licenses/'GPUI-Windows-LICENSE-APACHE')
  shutil.copy2(ROOT/'gui-gpui/vendor/gpui-pre-windows/SLIMEVR-PATCH.md',licenses/'GPUI-Windows-PATCH.md')
  portable.notices.copy_documentation(ROOT,base,args.build_revision,[('gui-gpui/README.zh-CN.md','原生前端说明.md')])
  for name,level in [('Start-SlimeVR.cmd','info'),('Start-SlimeVR-Debug.cmd','debug')]:
   (base/name).write_bytes(('@echo off\r\nsetlocal\r\ncd /d "%~dp0"\r\nstart "" "%~dp0SlimeVR.exe" --log-level '+level+' %*\r\n').encode())
  (base/'打开日志文件夹.cmd').write_bytes('@echo off\r\nstart "" "%APPDATA%\\dev.slimevr.SlimeVR\\logs"\r\n'.encode())
  (base/'使用说明.txt').write_text('''SlimeVR-Rust / GPUI — Windows x64 解压运行包

1. 完全退出已有前端和占用同一端口的后端，把整个文件夹解压到固定位置。
2. 双击 SlimeVR.exe 或 Start-SlimeVR.cmd，程序自动启动随包 Rust 后端。
3. 连接追踪器，确认左右佩戴与分配，执行完整 / 安装方向重置。

本包包含原生界面、Rust 后端、固定版本 SteamVR 驱动、Bindings Provider 和运行依赖。
已有 SlimeVR 驱动可继续使用。界面使用 GPUI 原生渲染器，运行依赖由包和系统提供。
SlimeVR-Components.exe 是使用内存示例值的组件预览；正常使用运行 SlimeVR.exe。

配置：%APPDATA%\\dev.slimevr.SlimeVR\\vrconfig.yml / vrconfig.yaml。
GUI 偏好沿用既有应用数据目录。自定义配置可用 --config "D:\\SlimeVR\\vrconfig.yml"。
日志：%APPDATA%\\dev.slimevr.SlimeVR\\logs\\gui-gpui.log。
默认 info；排查时完全退出后运行 Start-SlimeVR-Debug.cmd，正常启动恢复默认级别。
端口已有服务时连接该服务；--attach 指定连接已有后端。退出回收自己启动的后端。

防火墙提示出现时允许私有网络，以便接收 UDP。注册驱动后保持解压目录位置。
固件构建使用配置的构建服务；Discord Presence 使用本机 Discord 客户端。
构建提交、文件校验与资源来源见 BUILD-MANIFEST.json、SOURCE-CODE.txt 和 licenses/。
自动检查范围见 docs/rust-feature-status.zh-CN.md；真实设备与平台行为按 docs/rust-unified-hardware-test.zh-CN.md 验收。
CPU 与内存结论通过等价场景的实机测量记录。
''',encoding='utf-8-sig')
  files=[]
  for path in sorted(base.rglob('*')):
   if not path.is_file():continue
   entry={'path':path.relative_to(base).as_posix(),'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()}
   if path.suffix.lower()in ['.exe','.dll']:
    entry['pe']=portable.pe_info(path)
    for dll in entry['pe']['imports']:
     if dll in SYSTEM or dll.startswith(('api-ms-win-','ext-ms-win-')):continue
     available={f.name.lower()for directory in [path.parent,base]for f in directory.iterdir()if f.is_file()}
     if dll not in available:raise ValueError(f'Missing DLL {dll}: {entry["path"]}')
   files.append(entry)
  if portable.pe_info(base/'SlimeVR.exe')['subsystem']!=2:raise ValueError('Expected a Windows GUI executable')
  manifest={'app_version':'0.1.0','build_revision':args.build_revision,'frontend':'GPUI Kit 0.7.1','shaders':shaders,'component_preview_shaders':preview_shaders,'target':'Windows 11 x64','built_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'build_host':platform.system(),'webview2_required':False,'windows_native_execution_tested':False,'backend_source':('Updated Rust backend executable; baseline fix5 drivers and dependencies reused' if args.server_exe else 'Existing Rust backend; baseline fix5 components reused when selected'),'baseline_sha256':hashlib.sha256(args.baseline.read_bytes()).hexdigest()if args.baseline else None,'files':files}
  (base/'BUILD-MANIFEST.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
  args.output.parent.mkdir(parents=True,exist_ok=True)
  with zipfile.ZipFile(args.output,'w',zipfile.ZIP_DEFLATED,compresslevel=9)as archive:
   for path in sorted(base.rglob('*')):
    if path.is_file():archive.write(path,path.relative_to(base.parent).as_posix())
 with zipfile.ZipFile(args.output)as archive:
  if archive.testzip():raise ValueError('Generated ZIP failed CRC check')
 digest=hashlib.sha256(args.output.read_bytes()).hexdigest();args.output.with_suffix(args.output.suffix+'.sha256').write_text(f'{digest}  {args.output.name}\n');print(json.dumps({'path':str(args.output),'bytes':args.output.stat().st_size,'sha256':digest,'files':len(files)+1}))
if __name__=='__main__':main()
