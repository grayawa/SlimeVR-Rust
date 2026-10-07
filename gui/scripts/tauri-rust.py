#!/usr/bin/env python3
"""Build/stage the Rust server and host bridge, then launch or bundle Tauri."""
import argparse, os, pathlib, platform, subprocess
root=pathlib.Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['dev','build']);args=parser.parse_args()
env=dict(os.environ);env.setdefault('VITE_DESKTOP_VERSION','0.1.0')
subprocess.run(['cargo','build','--manifest-path',str(root/'server-rust/Cargo.toml'),'--release','--locked'],cwd=root,env=env,check=True)
for script in ['fetch-steamvr-drivers.py','build-bindings-provider.py']:
    subprocess.run([os.sys.executable,str(root/'gui/scripts'/script)],cwd=root,env=env,check=True)
config='tauri.rust.windows.conf.json' if platform.system()=='Windows' else 'tauri.rust.conf.json'
subprocess.run(['pnpm.cmd' if platform.system()=='Windows' else 'pnpm','exec','tauri',args.mode,'--config',f'src-tauri/{config}'],cwd=root/'gui',env=env,check=True)
