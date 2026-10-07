#!/usr/bin/env python3
"""Build the original owned OpenVR helper for Tauri on the current host."""
import argparse, pathlib, platform, shutil, subprocess
root=pathlib.Path(__file__).resolve().parents[2]
p=argparse.ArgumentParser();p.add_argument('--cmake',default='cmake');p.add_argument('--build-dir',type=pathlib.Path);args=p.parse_args()
if platform.system()=='Darwin':
    print('SteamVR desktop bridge is unavailable on macOS; no helper is bundled.');raise SystemExit(0)
arch='win64' if platform.system()=='Windows' else 'linuxarm64' if platform.machine()=='aarch64' else 'linux64'
build=args.build_dir or root/'bindings-provider'/'build'/arch
subprocess.run(['git','submodule','update','--init','bindings-provider/openvr'],cwd=root,check=True)
subprocess.run(['git','submodule','update','--init','lib/flatbuffers'],cwd=root/'solarxr-protocol',check=True)
configure=[args.cmake,'-S',str(root/'bindings-provider'),'-B',str(build),'-DCMAKE_BUILD_TYPE=Release','-DCMAKE_BUILD_RPATH=$ORIGIN','-DCMAKE_INSTALL_RPATH=$ORIGIN']
if platform.system()=='Windows':configure+=['-A','x64']
subprocess.run(configure,check=True)
subprocess.run([args.cmake,'--build',str(build),'--config','Release','--parallel'],check=True)
dest=root/'gui/src-tauri/resources/bindings'/arch;dest.mkdir(parents=True,exist_ok=True)
for name in (['SlimeVR-Bindings-Provider.exe','openvr_api.dll'] if arch=='win64' else ['slimevr-bindings-provider','libopenvr_api.so']):
    source=next((path for path in [build/name,build/'Release'/name] if path.is_file()),None)
    if not source:raise SystemExit(f'Missing build output: {name}')
    shutil.copy2(source,dest/name)
shutil.copy2(root/'bindings-provider/openvr/LICENSE',dest.parent/'OPENVR-LICENSE')
print(dest)
