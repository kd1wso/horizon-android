#!/usr/bin/env python3
"""Build the licensed, pinned Kenui's Default Android resource pack."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import struct
import sys
import urllib.request
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / '.local/android/kenuis-default-v6.6.zip'
URL = 'https://cdn.modrinth.com/data/Wx065BLG/versions/6LmnEsY6/Kenui%27s%20Default%20%5B1.20%2B%5D%20v6.6.zip'
SHA512 = ('f1053b11767b5f0be646d6e53301265f300c582f9d048fcbe093130c42afd98'
          '3313ab36cdc0748b334a909f432ba943b0b73ece48f2ac456452fcd34cbd1b595')
SIZE = 2786538
NAME = 'Kenuis-Default-Horizon-26.3.zip'
LICENSE = '''MIT License

Copyright (c) KenuiZ (Kenui_Z)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
'''
NOTES = '''Kenui's Default [1.20+] v6.6 by KenuiZ (Kenui_Z), MIT.
Source: https://modrinth.com/resourcepack/kenuis-default
Horizon 26.3 adaptation: metadata normalized; old armor and glint textures copied to
current paths; three corrupt PNG color profiles removed; Windows shortcut omitted.
OptiFine/ETF effects may not work. See LICENSE-KENUI.txt.
'''
FIX_PNG = {f'assets/minecraft/textures/block/{n}_e.png' for n in
           ('gilded_blackstone', 'lapis_ore', 'nether_gold_ore')}


def original(cache=CACHE):
    if cache.is_file():
        data = cache.read_bytes()
        if len(data) == SIZE and hashlib.sha512(data).hexdigest() == SHA512:
            return data
    req = urllib.request.Request(URL, headers={'User-Agent': 'Horizon-Android/1.0'})
    with urllib.request.urlopen(req, timeout=60) as response:
        data = response.read(SIZE + 1)
    if len(data) != SIZE or hashlib.sha512(data).hexdigest() != SHA512:
        raise ValueError('Kenui resource pack download failed SHA-512/size verification')
    cache.parent.mkdir(parents=True, exist_ok=True)
    part = cache.with_suffix('.partial')
    part.write_bytes(data)
    part.replace(cache)
    return data


def fix_png(data):
    if not data.startswith(b'\x89PNG\r\n\x1a\n'):
        raise ValueError('not a PNG')
    output, pos, removed = data[:8], 8, 0
    while pos < len(data):
        if pos + 12 > len(data): raise ValueError('truncated PNG')
        size = struct.unpack('>I', data[pos:pos+4])[0]
        end = pos + 12 + size
        if end > len(data): raise ValueError('truncated PNG chunk')
        chunk = data[pos:end]
        crc = struct.unpack('>I', chunk[-4:])[0]
        valid = crc == zlib.crc32(chunk[4:-4])
        if chunk[4:8] == b'iCCP' and not valid:
            removed += 1
        elif valid:
            output += chunk
        else:
            raise ValueError('unexpected corrupt PNG chunk')
        pos = end
    if removed != 1: raise ValueError('expected one corrupt color profile')
    return output


def adapted(data):
    output = io.BytesIO()
    with zipfile.ZipFile(io.BytesIO(data)) as source, zipfile.ZipFile(output, 'w',
            compression=zipfile.ZIP_DEFLATED, compresslevel=8) as dest:
        infos = source.infolist()
        names = {i.filename for i in infos}
        if len(infos) != len(names) or len(infos) > 32768:
            raise ValueError('duplicate names or too many entries')
        if sum(i.file_size for i in infos) > 512 << 20:
            raise ValueError('pack exceeds expanded size limit')
        meta = {'pack': {'description': '§2Kenui’s Default §7by §6Kenui_Z §8| §bHorizon 26.3 adaptation',
                         'min_format': [97, 1], 'max_format': [97, 1]}}
        def put(name, content):
            # Fixed timestamps make the installed-version digest stable across builds.
            entry = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            dest.writestr(entry, content)
        for e in infos:
            n = e.filename
            if (not n or n.startswith('/') or any(not c.isprintable() or c in '\\:' for c in n)
                    or any(p in ('.', '..') for p in n.split('/'))):
                raise ValueError(f'unsafe archive path: {n!r}')
            if n.endswith('resourcepacks - Atalho.lnk') or e.is_dir(): continue
            content = source.read(e)
            if n == 'pack.mcmeta': content = json.dumps(meta, ensure_ascii=False, indent=2).encode() + b'\n'
            if n in FIX_PNG: content = fix_png(content)
            put(n, content)
        for material in ('chainmail', 'leather'):
            for folder, layer in (('humanoid', '1'), ('humanoid_leggings', '2')):
                for overlay in (('', '_overlay') if material == 'leather' else ('',)):
                    old = f'assets/minecraft/textures/models/armor/{material}_layer_{layer}{overlay}.png'
                    new = f'assets/minecraft/textures/entity/equipment/{folder}/{material}{overlay}.png'
                    if old in names and new not in names: put(new, source.read(old))
        old = 'assets/minecraft/textures/misc/enchanted_item_glint.png'
        new = 'assets/minecraft/textures/misc/enchanted_glint_item.png'
        if old in names and new not in names:
            put(new, source.read(old))
            if old + '.mcmeta' in names: put(new + '.mcmeta', source.read(old + '.mcmeta'))
        put('LICENSE-KENUI.txt', LICENSE.encode())
        put('HORIZON-ANDROID.txt', NOTES.encode())
    return output.getvalue()


def packs():
    try:
        adapted(original())
        return [NAME], []
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        return [], [str(error)]


def write(out):
    data = adapted(original())
    out.mkdir(parents=True, exist_ok=True)
    partial = out / (NAME + '.partial')
    partial.write_bytes(data)
    partial.replace(out / NAME)
    for stale in out.iterdir():
        if stale.name != NAME:
            shutil.rmtree(stale) if stale.is_dir() else stale.unlink()
    return [out / NAME]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('check')
    stage = sub.add_parser('zip'); stage.add_argument('out', type=Path)
    args = parser.parse_args()
    if args.command == 'zip':
        for p in write(args.out): print(f'{p.name}: {p.stat().st_size} bytes')
        return 0
    found, errors = packs()
    for n in found: print(n)
    for e in errors: print(f'error: {e}', file=sys.stderr)
    if errors: return 1
    print(f'OK: {len(found)} resource pack(s).')
    return 0

if __name__ == '__main__': sys.exit(main())
