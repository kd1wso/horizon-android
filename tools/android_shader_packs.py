#!/usr/bin/env python3
"""Shader packs built into the Android APK, from shaders/android-native/.

Each folder there holding a shaders/ folder (the files of a classic pack's ZIP) goes into
the APK as <folder>.zip, and each .zip there as it is. On the phone the launcher puts them
in the profile's shaderpacks/, where Installed and the game's Shader Packs page list them
(docs/android-content.md). Names starting with _ or . and README.md are skipped.

  python3 tools/android_shader_packs.py check         what the next APK carries
  python3 tools/android_shader_packs.py zip OUT_DIR   write the ZIPs (tools/android_build.py does)
"""
import argparse
from pathlib import Path
import shutil
import sys
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FOLDER = ROOT / 'shaders/android-native'
# Every entry gets this time, so the same files always make the same ZIP.
STAMP = (2026, 1, 1, 0, 0, 0)
# The launcher's own limit for a shader file (content_files::MAX_INPUT).
MAX_BYTES = 256 << 20


def shown(path):
    try:
        return path.relative_to(ROOT).as_posix()
    except ValueError:
        return path.as_posix()


def plain(name):
    """content_files::plain_name: a name the launcher lists."""
    return (0 < len(name) <= 180 and not name.startswith('.')
            and not any(c in name for c in '/\\:\0') and all(c.isprintable() for c in name))


def files(source):
    """A pack folder's files, sorted, without hidden ones."""
    return sorted(p for p in source.rglob('*') if p.is_file()
                  and not any(part.startswith('.') for part in p.relative_to(source).parts))


def packs(folder=FOLDER):
    """([(name in shaderpacks/, source)], [problem]) for the packs in `folder`."""
    found, problems = [], []
    for path in sorted(folder.iterdir()) if folder.is_dir() else []:
        if path.name.startswith(('_', '.')) or path.name.lower() == 'readme.md':
            continue
        if path.is_dir():
            if not (path / 'shaders').is_dir():
                problems.append(f'{shown(path)}: no shaders/ folder in it (a pack keeps its files in shaders/)')
                continue
            name = path.name + '.zip'
            size = sum(p.stat().st_size for p in files(path))
        elif path.suffix.lower() == '.zip':
            name, size = path.name, path.stat().st_size
            try:
                with zipfile.ZipFile(path) as archive:
                    if not any(e.startswith('shaders/') for e in archive.namelist()):
                        problems.append(f'{shown(path)}: no shaders/ folder at the top of the ZIP')
                        continue
            except zipfile.BadZipFile:
                problems.append(f'{shown(path)}: not a ZIP file')
                continue
        else:
            problems.append(f'{shown(path)}: not a shader pack (a folder with shaders/ in it, or a .zip)')
            continue
        if not plain(name):
            problems.append(f'{shown(path)}: {name!r} is not a plain file name')
        elif size > MAX_BYTES:
            problems.append(f'{shown(path)}: larger than {MAX_BYTES >> 20} MiB')
        else:
            found.append((name, path))
    names = [name.lower() for name, _ in found]
    for name in sorted({n for n in names if names.count(n) > 1}):
        problems.append(f'{name}: two packs in {shown(folder)} have this name')
    return found, problems


def write_zip(source, target):
    with zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for path in files(source):
            entry = zipfile.ZipInfo(path.relative_to(source).as_posix(), STAMP)
            entry.compress_type = zipfile.ZIP_DEFLATED
            entry.external_attr = 0o644 << 16
            archive.writestr(entry, path.read_bytes())


def write(out, folder=FOLDER):
    """Write the packs' ZIPs into `out` and remove anything else there; returns their paths."""
    found, problems = packs(folder)
    if problems:
        raise SystemExit('android shader packs:\n  ' + '\n  '.join(problems))
    out.mkdir(parents=True, exist_ok=True)
    for name, source in found:
        partial = out / (name + '.partial')
        if source.is_dir():
            write_zip(source, partial)
        else:
            shutil.copyfile(source, partial)
        partial.replace(out / name)
    keep = {name for name, _ in found}
    for stale in out.iterdir():
        if stale.name not in keep:
            shutil.rmtree(stale) if stale.is_dir() else stale.unlink()
    return [out / name for name, _ in found]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('check', help='list the packs and their problems')
    zip_command = commands.add_parser('zip', help='write the ZIPs into a folder')
    zip_command.add_argument('out', type=Path)
    args = parser.parse_args()
    if args.command == 'zip':
        for path in write(args.out):
            print(f'{path.name}: {path.stat().st_size} bytes')
        return 0
    found, problems = packs()
    for name, source in found:
        kind = 'folder' if source.is_dir() else 'ZIP'
        print(f'{name} ({kind} {shown(source)})')
    for problem in problems:
        print(f'error: {problem}', file=sys.stderr)
    if problems:
        return 1
    print(f'OK: {len(found)} shader pack(s).')
    return 0


if __name__ == '__main__':
    sys.exit(main())
