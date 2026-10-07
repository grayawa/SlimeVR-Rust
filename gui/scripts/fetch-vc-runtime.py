#!/usr/bin/env python3
"""Extract the pinned Microsoft x64 redistributable DLLs and original license."""
import argparse
import hashlib
import json
import shutil
import struct
import subprocess
import tempfile
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

URL = ('https://download.visualstudio.microsoft.com/download/pr/'
       'bd1c8d9d-ba95-4eee-bc6e-df1fcc876373/'
       'CC0FF0EB1DC3F5188AE6300FAEF32BF5BEEBA4BDD6E8E445A9184072096B713B/VC_redist.x64.exe')
SHA256 = 'cc0ff0eb1dc3f5188ae6300faef32bf5beeba4bdd6e8e445a9184072096b713b'
DLLS = ['msvcp140.dll', 'msvcp140_atomic_wait.dll', 'vcruntime140.dll', 'vcruntime140_1.dll']


def extract(seven_zip, source, output):
    output.mkdir(parents=True, exist_ok=True)
    result = subprocess.run([seven_zip, 'x', '-y', '-bd', f'-o{output}', str(source)],
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    if result.returncode:
        raise RuntimeError(result.stdout)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--installer', type=Path, help='Use an already downloaded installer; hash is still checked')
    parser.add_argument('--seven-zip', default='7z')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='slimevr-vc-runtime-') as work:
        work = Path(work)
        if args.installer:
            data = args.installer.read_bytes()
        else:
            with urllib.request.urlopen(URL, timeout=60) as response:
                data = response.read()
        if hashlib.sha256(data).hexdigest() != SHA256:
            raise ValueError('Microsoft redistributable SHA-256 does not match the pinned source')
        # Burn embeds two CAB containers. Only scan the verified pinned image;
        # a valid header lets us skip compressed payloads and find the next CAB.
        cabinets = []
        offset = 0
        while (offset := data.find(b'MSCF', offset)) >= 0:
            size = struct.unpack_from('<I', data, offset + 8)[0]
            if data[offset + 4:offset + 8] == b'\0' * 4 and 36 <= size <= len(data) - offset:
                path = work / f'container-{len(cabinets)}.cab'
                path.write_bytes(data[offset:offset + size])
                cabinets.append(path)
                offset += size
            else:
                offset += 4
        if len(cabinets) != 2:
            raise ValueError(f'Expected two pinned Burn CAB containers, found {len(cabinets)}')
        bootstrap = work / 'bootstrap'
        attached = work / 'attached'
        extract(args.seven_zip, cabinets[0], bootstrap)
        extract(args.seven_zip, cabinets[1], attached)
        manifest = ET.fromstring((bootstrap / '0').read_bytes())
        payloads = {e.attrib['FilePath']: e.attrib for e in manifest.iter() if e.tag.endswith('}Payload')}
        args.output.mkdir(parents=True, exist_ok=True)
        shutil.copy2(bootstrap / payloads['license.rtf']['SourcePath'], args.output / 'VC-Runtime-LICENSE.rtf')
        found = {}
        for package in ['vcRuntimeMinimum_amd64', 'vcRuntimeAdditional_amd64']:
            payload = payloads[f'packages\\{package}\\cab1.cab']
            unpacked = work / package
            extract(args.seven_zip, attached / payload['SourcePath'], unpacked)
            for name in DLLS:
                source = unpacked / (name + '_amd64')
                if source.is_file():
                    found[name] = source
        for name in DLLS:
            if name not in found:
                raise ValueError(f'Missing x64 redistributable DLL: {name}')
            shutil.copy2(found[name], args.output / name)
        (args.output / 'SOURCE.json').write_text(json.dumps({
            'url': URL, 'sha256': SHA256, 'bytes': len(data), 'architecture': 'x86_64',
            'dlls': {name: hashlib.sha256((args.output / name).read_bytes()).hexdigest() for name in DLLS},
        }, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'output': str(args.output), 'dlls': DLLS, 'source_sha256': SHA256}))


if __name__ == '__main__':
    main()
