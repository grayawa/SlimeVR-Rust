#!/usr/bin/env python3
"""Reject Windows executables that still load GPUI shaders from source files."""
import argparse
import json
import struct
from pathlib import Path


def shader_compiler_imports(path):
    data = Path(path).read_bytes()
    pe = struct.unpack_from('<I', data, 0x3C)[0]
    if data[:2] != b'MZ' or data[pe:pe + 4] != b'PE\0\0':
        raise ValueError('Not a Windows PE executable')
    machine, count = struct.unpack_from('<HH', data, pe + 4)
    optional = pe + 24
    size = struct.unpack_from('<H', data, pe + 20)[0]
    if machine != 0x8664 or struct.unpack_from('<H', data, optional)[0] != 0x20B:
        raise ValueError('Expected an x64 executable')
    sections = []
    for index in range(count):
        virtual_size, address, raw_size, raw_offset = struct.unpack_from(
            '<IIII', data, optional + size + index * 40 + 8)
        sections.append((address, max(virtual_size, raw_size), raw_offset))

    def offset(rva):
        for address, length, raw in sections:
            if address <= rva < address + length:
                return raw + rva - address
        raise ValueError(f'Invalid PE RVA: {rva:x}')

    def string(rva):
        start = offset(rva)
        return data[start:data.index(b'\0', start)].decode('ascii')

    descriptor = offset(struct.unpack_from('<I', data, optional + 120)[0])
    symbols = []
    while any(data[descriptor:descriptor + 20]):
        names, _, _, dll, addresses = struct.unpack_from('<IIIII', data, descriptor)
        if string(dll).lower() == 'd3dcompiler_47.dll':
            thunk = offset(names or addresses)
            while (value := struct.unpack_from('<Q', data, thunk)[0]):
                if value & (1 << 63):
                    raise ValueError('Unexpected shader compiler ordinal import')
                symbols.append(string(value + 2))
                thunk += 8
        descriptor += 20
    return sorted(set(symbols))


def audit(path):
    symbols = shader_compiler_imports(path)
    if 'D3DCompileFromFile' in symbols:
        raise ValueError('Non-portable GPUI shader loader: D3DCompileFromFile is still imported')
    if 'D3DCompile' not in symbols:
        raise ValueError('Expected embedded runtime shader compiler D3DCompile')
    return {'embedded_runtime_shaders': True, 'compiler_imports': symbols}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('exe', type=Path)
    print(json.dumps(audit(parser.parse_args().exe)))
