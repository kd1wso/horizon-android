#!/usr/bin/env python3
"""Android native mods: horizon-mixins bundles built into the APK (docs/android-native-mods.md).

A phone can neither compile nor load native code, so a native mod reaches Android inside the
APK: the bundles in mods/android-native/ (folders with a horizon-mixin.toml, or .hnmod files)
are written into the game's source before the Android build, as horizon-mixins writes them
for the desktop launcher. Supported: HEAD (also cancellable) and OVERWRITE injections, by
named hook (native-hooks.json) or by target, support modules ([[module]]) and data files.

  check     validate the mods against this source (the default; writes nothing)
  apply     write them into the source; the originals are kept in .local/android-native-mods/
  clean     put the originals back
  new <id>  start a mod: mods/android-native/<id>/, a copy of the template (_example)

tools/android_build.py runs apply before Cargo and clean after it.
"""
import argparse
import contextlib
import difflib
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[1]
MODS = 'mods/android-native'
TEMPLATE = 'mods/android-native/_example'
WORK = '.local/android-native-mods'
DATA = 'native-data'
HOOKS_FILE = 'crates/horizon-client/src/game/native_hooks.rs'
# Where the rows of Options > Mods that need no game go, so that the launcher lists them too.
LAUNCHER_LIST = 'horizon_client::game::native_hooks::apk_native_mods'
BUNDLED = 'crates/horizon-client/src/game/android_bundled'
BEGIN = '// >>> android-native-mods (tools/android_native_mods.py; `clean` restores the original)'
END = '// <<< android-native-mods'
MARK = '// >>> android-native-mods'
# The .hnmod limits of horizon_mods::index::native, for folders too.
MAX_ENTRIES = 512
MAX_BYTES = 64 << 20
SUPPORTED = ('HEAD', 'OVERWRITE')
KINDS = {'HEAD', 'BEFORE_CALL', 'OVERWRITE', 'RETURN', 'MODIFY_ARG', 'MODIFY_CONSTANT',
         'WRAP_OPERATION', 'MODIFY_ARGS', 'MODIFY_VARIABLE', 'MODIFY_EXPRESSION_VALUE',
         'AFTER_CALL', 'AFTER_ASSIGN'}
INJECTION_KEYS = {'id', 'target', 'hook', 'hook_version', 'kind', 'helper', 'cancellable', 'argument',
                  'bindings', 'priority', 'selector', 'ordinal', 'require', 'fingerprint', 'slice',
                  'variable', 'result'}
SITE_KEYS = ('argument', 'selector', 'ordinal', 'require', 'slice', 'variable', 'result')
UNSUPPORTED_TABLES = {'bridge': 'bridges', 'state': 'per-instance state', 'crate': 'bundle crates',
                      'post_build': 'post-build steps'}
KEYWORDS = set('as async await break const continue crate dyn else enum extern false fn for if impl in '
               'let loop match mod move mut pub ref return self Self static struct super trait true '
               'type unsafe use where while abstract become box do final macro override priv try '
               'typeof unsized virtual yield gen'.split())


class ModError(Exception):
    """What is wrong with the mods, one problem per line."""


# ---------------------------------------------------------------------------------------------
# A small Rust lexer: enough to find items, signatures and bodies, never inside literals/comments.

IDENT = re.compile(r'(?:r#)?[^\W\d]\w*')
RAW = re.compile(r'(?:br|cr|r)(#*)"')
OPEN = {'(': ')', '[': ']', '{': '}'}


class Token:
    __slots__ = ('kind', 'text', 'start', 'end')

    def __init__(self, kind, text, start, end):
        self.kind, self.text, self.start, self.end = kind, text, start, end

    def punct(self, text):
        return self.kind == 'punct' and self.text == text

    def __repr__(self):
        return f'{self.kind}:{self.text}'


def lex(source):
    tokens, i, n = [], 0, len(source)
    while i < n:
        c = source[i]
        if c.isspace():
            i += 1
        elif source.startswith('//', i):
            j = source.find('\n', i)
            j = n if j < 0 else j
            text = source[i:j]
            if (text.startswith('///') and not text.startswith('////')) or text.startswith('//!'):
                tokens.append(Token('doc', text, i, j))
            i = j
        elif source.startswith('/*', i):
            depth, j = 1, i + 2
            while j < n and depth:
                if source.startswith('/*', j):
                    depth, j = depth + 1, j + 2
                elif source.startswith('*/', j):
                    depth, j = depth - 1, j + 2
                else:
                    j += 1
            if depth:
                raise ValueError(f'unterminated comment at offset {i}')
            text = source[i:j]
            if (text.startswith('/**') and not text.startswith('/***') and text != '/**/') or text.startswith('/*!'):
                tokens.append(Token('doc', text, i, j))
            i = j
        elif (m := RAW.match(source, i)) and (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == '_')):
            close = '"' + m.group(1)
            j = source.find(close, m.end())
            if j < 0:
                raise ValueError(f'unterminated raw string at offset {i}')
            tokens.append(Token('literal', source[i:j + len(close)], i, j + len(close)))
            i = j + len(close)
        elif c == '"' or (c in 'bc' and source.startswith('"', i + 1) and
                          (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == '_'))):
            j = i + (1 if c == '"' else 2)
            while j < n and source[j] != '"':
                j += 2 if source[j] == '\\' else 1
            if j >= n:
                raise ValueError(f'unterminated string at offset {i}')
            tokens.append(Token('literal', source[i:j + 1], i, j + 1))
            i = j + 1
        elif c == "'" or (c == 'b' and source.startswith("'", i + 1) and
                          (i == 0 or not (source[i - 1].isalnum() or source[i - 1] == '_'))):
            k = i + (1 if c == "'" else 2)
            if k < n and source[k] == '\\':
                j = source.find("'", k + 2)
                if j < 0:
                    raise ValueError(f'unterminated character at offset {i}')
                tokens.append(Token('literal', source[i:j + 1], i, j + 1))
                i = j + 1
            elif k + 1 < n and source[k + 1] == "'" and source[k] != "'":
                tokens.append(Token('literal', source[i:k + 2], i, k + 2))
                i = k + 2
            elif c == "'" and (m := IDENT.match(source, k)):
                tokens.append(Token('lifetime', source[i:m.end()], i, m.end()))
                i = m.end()
            else:
                raise ValueError(f'unexpected quote at offset {i}')
        elif c.isdigit():
            j = i + 1
            while j < n and (source[j].isalnum() or source[j] == '_' or
                             (source[j] == '.' and j + 1 < n and source[j + 1].isdigit())):
                j += 1
            tokens.append(Token('literal', source[i:j], i, j))
            i = j
        elif m := IDENT.match(source, i):
            tokens.append(Token('ident', m.group(), i, m.end()))
            i = m.end()
        elif source.startswith('::', i) or source.startswith('->', i):
            tokens.append(Token('punct', source[i:i + 2], i, i + 2))
            i += 2
        else:
            tokens.append(Token('punct', c, i, i + 1))
            i += 1
    return tokens


def pairs(tokens):
    """Each opening delimiter's index -> its closing one's."""
    stack, match = [], {}
    for index, token in enumerate(tokens):
        if token.kind != 'punct':
            continue
        if token.text in OPEN:
            stack.append(index)
        elif token.text in (')', ']', '}'):
            if not stack or OPEN[tokens[stack[-1]].text] != token.text:
                raise ValueError(f'unbalanced {token.text!r} at offset {token.start}')
            match[stack.pop()] = index
    if stack:
        raise ValueError(f'unclosed {tokens[stack[-1]].text!r} at offset {tokens[stack[-1]].start}')
    return match


def spaced(tokens):
    """Tokens as text, one space only between words: `&mut Vec < T >` and `&mut Vec<T>` agree."""
    out, word = [], False
    for token in tokens:
        now = token.kind in ('ident', 'lifetime', 'literal')
        if now and word:
            out.append(' ')
        out.append(token.text)
        word = now
    return ''.join(out)


def type_text(tokens):
    """A type or path segment as horizon-mixins names it: named lifetimes become '_."""
    return spaced([Token('lifetime', "'_", t.start, t.end)
                   if t.kind == 'lifetime' and t.text != "'static" else t for t in tokens])


def normal_type(text):
    return type_text(lex(text))


def split_commas(tokens, match, start, end):
    parts, first, depth, k = [], start, 0, start
    while k < end:
        token = tokens[k]
        if token.kind == 'punct':
            if token.text in OPEN:
                k = match[k] + 1
                continue
            if token.text == '<':
                depth += 1
            elif token.text == '>':
                depth -= 1
            elif token.text == ',' and depth == 0:
                parts.append((first, k))
                first = k + 1
        k += 1
    if first < end:
        parts.append((first, end))
    return parts


class Param:
    def __init__(self, tokens):
        colon = next((k for k, t in enumerate(tokens) if t.punct(':')), None)
        pattern = tokens if colon is None else tokens[:colon]
        self.receiver = any(t.kind == 'ident' and t.text == 'self' for t in pattern)
        names = [t for t in pattern if t.kind == 'ident' and t.text not in ('mut', 'ref')]
        plain = len(names) == 1 and all(t.kind == 'ident' for t in pattern)
        self.name = 'self' if self.receiver else names[0].text if plain else None
        self.type = type_text(tokens[colon + 1:]) if colon is not None else None


class Function:
    """A function item: its module path inside the file, its impl's type, signature and body."""

    def __init__(self, tokens, match, at, end):
        self.name = tokens[at + 1].text
        self.start = tokens[at].start
        k = at + 2
        self.generic = k < end and tokens[k].punct('<')
        if self.generic:
            depth = 0
            while k < end:
                if tokens[k].punct('<'):
                    depth += 1
                elif tokens[k].punct('>'):
                    depth -= 1
                    if depth == 0:
                        break
                k += 1
            k += 1
        self.valid = k < end and tokens[k].punct('(')
        if not self.valid:
            return
        close = match[k]
        self.params = [Param(tokens[a:b]) for a, b in split_commas(tokens, match, k + 1, close)]
        k, result, where = close + 1, None, None
        while k < end and not (tokens[k].kind == 'punct' and tokens[k].text in ('{', ';')):
            if tokens[k].punct('->') and result is None and where is None:
                result = k + 1
            elif tokens[k].kind == 'ident' and tokens[k].text == 'where' and where is None:
                where = k
            elif tokens[k].kind == 'punct' and tokens[k].text in ('(', '['):
                k = match[k]
            k += 1
        self.where = where is not None
        self.result = type_text(tokens[result:where if where is not None else k]) if result is not None else '()'
        self.body = k < end and tokens[k].punct('{')
        if self.body:
            self.open, self.close_index = tokens[k].start, match[k]
            self.close = tokens[match[k]].start
        self.next = (match[k] if self.body else k) + 1
        self.path, self.owner = [], None

    @property
    def names(self):
        return {p.name for p in self.params if p.name}


def impl_owner(header):
    """`impl<'a> Game<'a>` -> Game<'_>; `impl Trait for Type` -> <Type as Trait>."""
    k = 0
    if header and header[0].punct('<'):
        depth = 0
        for k, token in enumerate(header):
            if token.punct('<'):
                depth += 1
            elif token.punct('>'):
                depth -= 1
                if depth == 0:
                    break
        k += 1
    rest, depth, split = header[k:], 0, None
    for index, token in enumerate(rest):
        if token.punct('<'):
            depth += 1
        elif token.punct('>'):
            depth -= 1
        elif depth == 0 and token.kind == 'ident' and token.text == 'where':
            rest = rest[:index]
            break
        elif depth == 0 and token.kind == 'ident' and token.text == 'for' and split is None:
            split = index
    if split is None:
        return type_text(rest)
    return f'<{type_text(rest[split + 1:])} as {type_text(rest[:split])}>'


class Source:
    """A Rust file's functions and top-level names."""

    def __init__(self, text):
        self.text = text
        self.tokens = lex(text)
        self.match = pairs(self.tokens)
        self.functions, self.names = [], set()
        self._items(0, len(self.tokens), [], None)

    def _items(self, i, end, path, owner):
        tokens, match = self.tokens, self.match
        while i < end:
            token = tokens[i]
            if token.kind == 'punct' and token.text in OPEN:
                i = match[i] + 1
                continue
            if token.kind == 'ident':
                word = token.text
                after = tokens[i + 1] if i + 1 < end else None
                if word == 'mod' and owner is None and after and after.kind == 'ident' and i + 2 < end:
                    if not path:
                        self.names.add(after.text)
                    if tokens[i + 2].punct('{'):
                        close = match[i + 2]
                        self._items(i + 3, close, path + [after.text], None)
                        i = close + 1
                        continue
                elif word == 'impl' and owner is None:
                    j = i + 1
                    while j < end and not (tokens[j].kind == 'punct' and tokens[j].text in ('{', ';')):
                        if tokens[j].kind == 'punct' and tokens[j].text in ('(', '['):
                            j = match[j]
                        j += 1
                    if j < end and tokens[j].punct('{'):
                        self._items(j + 1, match[j], path, impl_owner(tokens[i + 1:j]))
                        i = match[j] + 1
                        continue
                    i = j + 1
                    continue
                elif word == 'fn' and after and after.kind == 'ident':
                    function = Function(tokens, match, i, end)
                    if function.valid:
                        function.path, function.owner = path, owner
                        if function.body:
                            self.functions.append(function)
                        if not path and owner is None:
                            self.names.add(function.name)
                        i = function.next
                        continue
                elif (word in ('struct', 'enum', 'union', 'trait', 'type', 'const', 'static')
                      and not path and owner is None and after and after.kind == 'ident'):
                    self.names.add(after.text)
                elif word == 'use' and not path and owner is None:
                    j = i + 1
                    while j < end and not tokens[j].punct(';'):
                        j += 1
                    self._use(tokens[i + 1:j])
                    i = j + 1
                    continue
            i += 1

    def _use(self, tokens):
        for k, token in enumerate(tokens):
            if token.kind != 'ident' or token.text in ('self', 'super', 'crate', 'as'):
                continue
            after = tokens[k + 1] if k + 1 < len(tokens) else None
            before = tokens[k - 1] if k else None
            if (before is not None and before.kind == 'ident' and before.text == 'as') or \
                    after is None or after.text in (',', '}'):
                self.names.add(token.text)


# ---------------------------------------------------------------------------------------------
# The workspace: crates, hook catalog, bundled Android mods.

def rel(root, path):
    return Path(path).resolve().relative_to(Path(root).resolve()).as_posix()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def crates(root):
    """Crate name (as in a target) -> its package folder."""
    manifest = tomllib.loads((root / 'Cargo.toml').read_text(encoding='utf-8'))
    found = {}
    for member in manifest.get('workspace', {}).get('members', []):
        for folder in sorted(root.glob(member)) if any(c in member for c in '*?[') else [root / member]:
            cargo = folder / 'Cargo.toml'
            if cargo.is_file():
                package = tomllib.loads(cargo.read_text(encoding='utf-8')).get('package', {})
                if 'name' in package:
                    found[package['name'].replace('-', '_')] = folder
    return found


def catalog(root):
    """(hook, version) -> target: native-hooks.json, and pending fragments as version 1."""
    hooks = {}
    for hook in json.loads((root / 'native-hooks.json').read_text(encoding='utf-8'))['hooks']:
        hooks[(hook['name'], hook['version'])] = hook['target']
    for fragment in sorted((root / 'tools/native-hook-fragments').glob('*.json')):
        for hook in json.loads(fragment.read_text(encoding='utf-8')).get('hooks', []):
            hooks.setdefault((hook['name'], hook.get('version', 1)), hook['target'])
    return hooks


def hooks_api(root):
    path = root / HOOKS_FILE
    if not path.is_file():
        return None
    found = re.search(r'pub\(crate\) const NATIVE_HOOKS_API: u32 = (\d+);', path.read_text(encoding='utf-8'))
    return int(found.group(1)) if found else None


def bundled(root):
    folder = root / BUNDLED
    return {p.name for p in folder.iterdir() if p.is_dir()} if folder.is_dir() else set()


def split_target(target):
    """`horizon_client::game::native_hooks::Game<'_>::native_key` -> its segments."""
    segments, current, depth = [], [], 0
    for token in lex(target):
        if token.punct('<'):
            depth += 1
        elif token.punct('>'):
            depth -= 1
        if token.punct('::') and depth == 0:
            segments.append(current)
            current = []
        else:
            current.append(token)
    segments.append(current)
    return [type_text(s) for s in segments]


# ---------------------------------------------------------------------------------------------
# Mods.

class Injection:
    pass


class Module:
    pass


class Mod:
    def __init__(self, label, files, archive=None):
        self.label = label          # mods/android-native/<name> or <name>.hnmod
        self.files = files          # bundle-relative path -> bytes
        self.archive = archive      # the .hnmod path, else None (a folder)
        self.folder = None          # the folder whose files the build reads
        self.injections, self.modules = [], []
        self.hooks = None
        self.id = self.version = None
        self.priority = 0

    @property
    def data(self):
        return {name[5:]: data for name, data in self.files.items() if name.startswith('data/')}


def valid_id(value):
    return isinstance(value, str) and 0 < len(value) <= 100 and re.fullmatch(r'[A-Za-z0-9_-]+', value)


def bundle_path(value):
    if not isinstance(value, str) or not value or value.startswith('/') or '\\' in value or ':' in value \
            or '"' in value or any(p in ('', '.', '..') for p in value.split('/')):
        return None
    return value


def read_folder(folder, label, errors):
    files, total = {}, 0
    for path in sorted(folder.rglob('*')):
        parts = path.relative_to(folder).parts
        if any(p in ('.git', 'target') for p in parts[:-1]) or parts[0] in ('.git', 'target'):
            continue
        if path.is_symlink():
            errors.append(f'{label}: symlinks are not allowed ({path.relative_to(folder).as_posix()})')
            continue
        if path.is_file():
            data = path.read_bytes()
            total += len(data)
            files[path.relative_to(folder).as_posix()] = data
    if len(files) > MAX_ENTRIES or total > MAX_BYTES:
        errors.append(f'{label}: a bundle holds at most {MAX_ENTRIES} files and 64 MiB')
    return files


def read_hnmod(path, label, errors):
    """A .hnmod's mixins/<id>/ files and its manifest, checked as horizon_mods::index::native does."""
    try:
        archive = zipfile.ZipFile(path)
    except (OSError, zipfile.BadZipFile) as e:
        errors.append(f'{label}: not a readable .hnmod ({e})')
        return None, None
    with archive:
        entries = archive.infolist()
        if len(entries) > MAX_ENTRIES:
            errors.append(f'{label}: too many entries')
            return None, None
        files, total = {}, 0
        for entry in entries:
            if entry.is_dir():
                continue
            mode = entry.external_attr >> 16
            name = entry.filename
            if (mode & 0o170000 and mode & 0o170000 != 0o100000) or bundle_path(name) is None:
                errors.append(f'{label}: invalid path in the bundle: {name}')
                return None, None
            with archive.open(entry) as stream:
                data = stream.read(MAX_BYTES + 1)
            total += len(data)
            if total > MAX_BYTES:
                errors.append(f'{label}: the bundle expands beyond 64 MiB')
                return None, None
            if name in files:
                errors.append(f'{label}: duplicate path {name}')
                return None, None
            files[name] = data
    try:
        manifest = json.loads(files['horizon-native.json'])
    except KeyError:
        errors.append(f'{label}: missing horizon-native.json')
        return None, None
    except ValueError as e:
        errors.append(f'{label}: horizon-native.json: {e}')
        return None, None
    revision = re.compile(r'[0-9a-f]{40}')
    known = {'schema', 'id', 'version', 'game', 'horizon', 'mixins', 'hooks'}
    if (not isinstance(manifest, dict) or set(manifest) - known or manifest.get('schema') != 1
            or not (isinstance(manifest.get('id'), str) and re.fullmatch(r'[a-z0-9_]{1,64}', manifest['id']))
            or not manifest.get('version') or not manifest.get('game')
            or not revision.fullmatch(str(manifest.get('horizon', '')))
            or not revision.fullmatch(str(manifest.get('mixins', '')))
            or manifest.get('hooks', 1) in (0, None) or isinstance(manifest.get('hooks', 1), bool)
            or not isinstance(manifest.get('hooks', 1), int)):
        errors.append(f'{label}: invalid horizon-native.json: schema 1, id, version, game and exact revisions required')
        return None, None
    prefix = f'mixins/{manifest["id"]}/'
    bundle = {}
    for name, data in files.items():
        if name == 'horizon-native.json':
            continue
        if not name.startswith(prefix):
            errors.append(f'{label}: unexpected file {name}')
            return None, None
        bundle[name[len(prefix):]] = data
    return bundle, manifest


def discover(root, folder, errors):
    mods = []
    if not folder.is_dir():
        return mods
    for path in sorted(folder.iterdir()):
        name = path.name
        label = rel(root, path) if path.resolve().is_relative_to(root.resolve()) else str(path)
        if name.startswith(('.', '_')):
            continue
        if path.is_symlink():
            errors.append(f'{label}: symlinks are not allowed')
        elif path.is_dir():
            if not (path / 'horizon-mixin.toml').is_file():
                hint = (' (an unpacked .hnmod: put the .hnmod file itself here, or its mixins/<id>/ folder)'
                        if (path / 'horizon-native.json').is_file() else '')
                errors.append(f'{label}: no horizon-mixin.toml{hint}')
                continue
            mod = Mod(label, read_folder(path, label, errors))
            mod.folder = path
            mods.append(mod)
        elif name.endswith('.hnmod'):
            files, manifest = read_hnmod(path, label, errors)
            if files is not None:
                mod = Mod(label, files, archive=path)
                mod.hnmod = manifest
                mod.hooks = manifest.get('hooks')
                mods.append(mod)
        elif not (name.lower().endswith(('.md', '.txt')) or name.startswith('README')):
            errors.append(f'{label}: not a mod (a folder with horizon-mixin.toml, or a .hnmod file)')
    return mods


def unknown(table, keys, where, errors):
    extra = sorted(set(table) - set(keys))
    if extra:
        errors.append(f'{where}: unknown field{"s" if len(extra) > 1 else ""} {", ".join(extra)}')


def read_manifest(mod, errors):
    where = f'{mod.label}/horizon-mixin.toml'
    try:
        manifest = tomllib.loads(mod.files['horizon-mixin.toml'].decode('utf-8-sig'))
    except KeyError:
        errors.append(f'{mod.label}: missing horizon-mixin.toml')
        return False
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as e:
        errors.append(f'{where}: {e}')
        return False
    unknown(manifest, {'mod', 'compatibility', 'injection', 'bridge', 'module', 'state', 'crate', 'post_build'},
            where, errors)
    info = manifest.get('mod')
    if not isinstance(info, dict):
        errors.append(f'{where}: needs a [mod] table with id and version')
        return False
    unknown(info, {'id', 'version', 'priority'}, f'{where} [mod]', errors)
    mod.id, mod.version = info.get('id'), info.get('version')
    if not valid_id(mod.id):
        errors.append(f'{where}: [mod] id must be 1-100 letters, digits, _ or -')
        return False
    if not isinstance(mod.version, str) or not mod.version:
        errors.append(f'{where}: [mod] version must be a non-empty string')
    priority = info.get('priority', 0)
    if not isinstance(priority, int) or isinstance(priority, bool):
        errors.append(f'{where}: [mod] priority must be an integer')
    else:
        mod.priority = priority
    compatibility = manifest.get('compatibility', {})
    if not isinstance(compatibility, dict):
        errors.append(f'{where}: [compatibility] must be a table')
        compatibility = {}
    unknown(compatibility, {'horizon_revision'}, f'{where} [compatibility]', errors)
    if mod.archive is not None:
        hnmod = mod.hnmod
        if (mod.id != hnmod['id'] or mod.version != hnmod['version']
                or compatibility.get('horizon_revision') != hnmod['horizon']):
            errors.append(f'{mod.label}: horizon-native.json and horizon-mixin.toml differ in id, version or revision')
    for table, what in UNSUPPORTED_TABLES.items():
        if manifest.get(table):
            errors.append(f'{where}: {what} ([[{table}]]) are not supported on Android yet')
    ids = set()
    for entry in manifest.get('injection', []):
        injection = read_injection(mod, entry, where, errors)
        if injection is not None:
            if injection.id in ids:
                errors.append(f'{where}: duplicate id {injection.id!r}')
            ids.add(injection.id)
            mod.injections.append(injection)
    for entry in manifest.get('module', []):
        module = read_module(mod, entry, where, errors)
        if module is not None:
            if module.id in ids:
                errors.append(f'{where}: duplicate id {module.id!r}')
            ids.add(module.id)
            mod.modules.append(module)
    return True


def read_injection(mod, entry, where, errors):
    if not isinstance(entry, dict):
        errors.append(f'{where}: [[injection]] must be a table')
        return None
    injection = Injection()
    injection.mod = mod
    injection.id = entry.get('id')
    here = f'{where} injection {injection.id!r}'
    if not valid_id(injection.id):
        errors.append(f'{where}: an [[injection]] needs an id of letters, digits, _ or -')
        return None
    unknown(entry, INJECTION_KEYS, here, errors)
    kind = entry.get('kind')
    if kind not in KINDS:
        errors.append(f'{here}: unknown kind {kind!r}')
        return None
    if kind not in SUPPORTED:
        errors.append(f'{here}: {kind} is not supported on Android yet (HEAD, cancellable HEAD and OVERWRITE are)')
        return None
    injection.kind = kind
    for key in SITE_KEYS:
        if key in entry:
            errors.append(f'{here}: {kind} takes no {key}')
    injection.cancellable = entry.get('cancellable', False)
    if not isinstance(injection.cancellable, bool) or (injection.cancellable and kind != 'HEAD'):
        errors.append(f'{here}: cancellable is true or false, for HEAD only')
        injection.cancellable = False
    priority = entry.get('priority', mod.priority)
    if not isinstance(priority, int) or isinstance(priority, bool):
        errors.append(f'{here}: priority must be an integer')
        priority = mod.priority
    injection.priority = priority
    target, hook, version = entry.get('target'), entry.get('hook'), entry.get('hook_version')
    if target is not None and hook is not None:
        errors.append(f'{here}: give either target or hook (with hook_version), not both')
        return None
    if hook is not None:
        if not isinstance(hook, str) or not isinstance(version, int) or isinstance(version, bool) or version < 1:
            errors.append(f'{here}: hook needs a hook_version (a number from native-hooks.json)')
            return None
    elif version is not None:
        errors.append(f'{here}: hook_version without hook')
        return None
    elif not isinstance(target, str) or not target:
        errors.append(f'{here}: needs a target or a hook')
        return None
    injection.target, injection.hook, injection.hook_version = target, hook, version
    helper = bundle_path(entry.get('helper'))
    if helper is None or not helper.endswith('.rs'):
        errors.append(f'{here}: helper must be a .rs file inside the bundle')
        return None
    if helper not in mod.files:
        errors.append(f'{here}: helper {helper} is missing')
        return None
    injection.helper_path = helper
    bindings = entry.get('bindings', [])
    good = isinstance(bindings, list) and all(
        isinstance(b, dict) and set(b) == {'name', 'ty', 'value'} and all(isinstance(v, str) for v in b.values())
        for b in bindings)
    if not good:
        errors.append(f'{here}: bindings are a list of {{ name, ty, value }} strings')
        return None
    injection.bindings = bindings
    return injection


def read_module(mod, entry, where, errors):
    if not isinstance(entry, dict):
        errors.append(f'{where}: [[module]] must be a table')
        return None
    module = Module()
    module.mod = mod
    module.id = entry.get('id')
    here = f'{where} module {module.id!r}'
    if not valid_id(module.id):
        errors.append(f'{where}: a [[module]] needs an id of letters, digits, _ or -')
        return None
    unknown(entry, {'id', 'file', 'name', 'source', 'visibility'}, here, errors)
    module.file = bundle_path(entry.get('file'))
    module.name = entry.get('name')
    module.source = bundle_path(entry.get('source'))
    module.visibility = entry.get('visibility', 'pub(crate)')
    if module.file is None or not module.file.endswith('.rs'):
        errors.append(f'{here}: file must be a .rs file of the workspace')
        return None
    if not (isinstance(module.name, str) and re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', module.name)
            and module.name not in KEYWORDS and module.name != '_'):
        errors.append(f'{here}: name must be a Rust identifier')
        return None
    if module.source is None or not module.source.endswith('.rs') or module.source not in mod.files:
        errors.append(f'{here}: source must be a .rs file of the bundle')
        return None
    if module.visibility not in ('pub(crate)', 'pub'):
        errors.append(f'{here}: visibility is "pub(crate)" or "pub"')
        return None
    return module


class Helper:
    def __init__(self, source):
        self.source = source
        tokens = lex(source)
        match = pairs(tokens)
        if not tokens or not (tokens[0].kind == 'ident' and tokens[0].text == 'fn'):
            raise ValueError('a helper is one private function: `fn name(...) { ... }` with no '
                             'attributes, doc comments, pub, async, const, unsafe or extern')
        function = Function(tokens, match, 0, len(tokens)) if len(tokens) > 1 and tokens[1].kind == 'ident' else None
        if function is None or not function.valid or not function.body:
            raise ValueError('a helper is one function with a body')
        if function.generic or function.where:
            raise ValueError('a helper has no generic parameters or where clause')
        if function.close_index != len(tokens) - 1:
            raise ValueError('a helper file holds exactly one function')
        if any(p.receiver for p in function.params):
            raise ValueError('a helper takes no self')
        self.function = function
        self.name = function.name


def value_name(value):
    """`&mut *game` -> game; None for anything but a parameter, its borrow or reborrow."""
    try:
        tokens = lex(value)
    except ValueError:
        return None
    k = 0
    if k < len(tokens) and tokens[k].punct('&'):
        k += 1
        if k < len(tokens) and tokens[k].kind == 'ident' and tokens[k].text == 'mut':
            k += 1
    if k < len(tokens) and tokens[k].punct('*'):
        k += 1
    if k == len(tokens) - 1 and tokens[k].kind == 'ident' and tokens[k].text not in KEYWORDS - {'self'}:
        return tokens[k].text
    return None


def control_flow(result):
    for prefix in ('::core::ops::', 'core::ops::', '::std::ops::', 'std::ops::', ''):
        if result.startswith(prefix + 'ControlFlow<') and result.endswith('>'):
            return result[len(prefix) + len('ControlFlow<'):-1]
    return None


# ---------------------------------------------------------------------------------------------
# The plan: what apply writes.

class Plan:
    def __init__(self, root, mods, edits, originals):
        self.root, self.mods, self.edits, self.originals = root, mods, edits, originals

    def summary(self):
        lines = []
        for mod in self.mods:
            hooks = sorted({i.hook or i.target.rsplit('::', 1)[-1] for i in mod.injections})
            lines.append(f'{mod.id} {mod.version} ({mod.label}): {len(mod.injections)} injection(s)'
                         f'{" on " + ", ".join(hooks) if hooks else ""}, {len(mod.modules)} module(s)'
                         f'{", " + str(len(mod.data)) + " data file(s)" if mod.data else ""}')
        return lines


class State:
    """What apply changed: files (with their originals' and results' hashes) and folders."""

    def __init__(self, root):
        self.root = root
        self.path = root / WORK / 'state.json'
        self.originals = root / WORK / 'originals'
        data = json.loads(self.path.read_text(encoding='utf-8')) if self.path.is_file() else {}
        self.files = data.get('files', {})
        self.folders = data.get('folders', [])

    def original(self, name):
        """The file as it was before apply, if apply changed it."""
        if name in self.files:
            return (self.originals / name).read_bytes()
        return None

    def save(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text(json.dumps({'format': 1, 'files': self.files, 'folders': self.folders},
                                        indent=2, sort_keys=True) + '\n', encoding='utf-8')


def original_text(root, state, name, errors):
    data = state.original(name)
    if data is None:
        path = root / name
        if not path.is_file():
            errors.append(f'{name}: no such file')
            return None
        data = path.read_bytes()
    try:
        text = data.decode('utf-8')
    except UnicodeDecodeError:
        errors.append(f'{name}: not UTF-8')
        return None
    if MARK in text:
        errors.append(f'{name} already holds android-native-mods code that apply did not record: '
                      f'restore it with git (git checkout -- {name})')
        return None
    return text


def load(root=ROOT, folder=None):
    """The mods in `folder` (mods/android-native/), checked against `root`'s source."""
    root = Path(root).resolve()
    folder = Path(folder).resolve() if folder else root / MODS
    errors = []
    mods = [m for m in discover(root, folder, errors) if read_manifest(m, errors)]
    state = State(root)
    if not mods:
        if errors:
            raise ModError('\n'.join(errors))
        return Plan(root, [], {}, {})
    seen, taken = {}, bundled(root)
    for mod in mods:
        if mod.id in seen:
            errors.append(f'{mod.label}: id {mod.id!r} is also {seen[mod.id]}\'s')
        seen[mod.id] = mod.label
        if mod.id in taken:
            errors.append(f'{mod.label}: {mod.id!r} is already built into the app ({BUNDLED}/{mod.id}); '
                          f'give your mod another id')
    api = hooks_api(root)
    for mod in mods:
        if mod.hooks is not None and api is not None and mod.hooks > api:
            errors.append(f'{mod.label}: needs hook points v{mod.hooks}; this source has v{api}')
    hooks, packages, sources, texts = catalog(root), crates(root), {}, {}

    def source(name):
        if name not in sources:
            text = original_text(root, state, name, errors)
            try:
                sources[name] = Source(text) if text is not None else None
            except ValueError as e:
                errors.append(f'{name}: can\'t read it ({e})')
                sources[name] = None
            texts[name] = text
        return sources[name]

    targets = {}
    for mod in mods:
        for injection in mod.injections:
            here = f'{mod.label} injection {injection.id!r}'
            if injection.hook is not None:
                target = hooks.get((injection.hook, injection.hook_version))
                if target is None:
                    versions = sorted(v for n, v in hooks if n == injection.hook)
                    if versions:
                        errors.append(f'{here}: hook {injection.hook!r} has version(s) {versions}, '
                                      f'not {injection.hook_version}')
                    else:
                        close = difflib.get_close_matches(injection.hook, sorted({n for n, _ in hooks}), 3)
                        errors.append(f'{here}: no hook {injection.hook!r} in native-hooks.json'
                                      f'{" (did you mean " + ", ".join(close) + "?)" if close else ""}')
                    continue
                injection.target = target
            found = resolve(root, injection.target, packages, source, here, errors)
            if found is None:
                continue
            injection.file, injection.function = found
            try:
                injection.helper = Helper(mod.files[injection.helper_path].decode('utf-8-sig'))
            except (UnicodeDecodeError, ValueError) as e:
                errors.append(f'{mod.label}/{injection.helper_path}: {e}')
                continue
            if check_contract(injection, f'{mod.label}/{injection.helper_path}', errors):
                key = (injection.file, injection.function.open)
                targets.setdefault(key, []).append(injection)
    listed(root, hooks, packages, source, targets, errors)
    edits = {}
    for (name, _), injections in targets.items():
        function = injections[0].function
        overwrite = [i for i in injections if i.kind == 'OVERWRITE']
        if overwrite and len(injections) > 1:
            errors.append(f'{injections[0].target}: an OVERWRITE ({overwrite[0].mod.id} {overwrite[0].id!r}) '
                          f'conflicts with every other injection there '
                          f'({", ".join(i.mod.id + " " + repr(i.id) for i in injections if i is not overwrite[0])})')
            continue
        injections.sort(key=lambda i: (-i.priority, i.mod.id, i.id))
        blocks = [block(i) for i in injections]
        text = '\n' + BEGIN + '\n' + '\n'.join(blocks) + '\n' + END + '\n'
        end = function.close if overwrite else function.open + 1
        edits.setdefault(name, []).append((function.open + 1, end, text))
    modules = {}
    for mod in mods:
        for module in mod.modules:
            here = f'{mod.label} module {module.id!r}'
            destination = source(module.file)
            if destination is None:
                continue
            if (module.file, module.name) in modules:
                errors.append(f'{here}: {modules[(module.file, module.name)]} also adds `{module.name}` to {module.file}')
                continue
            modules[(module.file, module.name)] = here
            if module.name in destination.names:
                errors.append(f'{here}: {module.file} already has a `{module.name}`; name the module otherwise')
                continue
            base = mod.folder if mod.folder is not None else root / WORK / 'bundles' / mod.id
            path = os.path.relpath(base / module.source, (root / module.file).parent).replace(os.sep, '/')
            path = path.replace('\\', '\\\\').replace('"', '\\"')
            text = (f'\n{BEGIN}\n// {mod.label}/{module.source}: {mod.id} module "{module.id}"\n'
                    f'#[path = "{path}"]\n{module.visibility} mod {module.name};\n{END}\n')
            size = len(texts[module.file])
            edits.setdefault(module.file, []).append((size, size, text))
    if errors:
        raise ModError('\n'.join(errors))
    results = {}
    for name, changes in edits.items():
        text = texts[name]
        for start, end, insert in sorted(changes, key=lambda c: (c[0], c[1]), reverse=True):
            text = text[:start] + insert + text[end:]
        results[name] = text
    return Plan(root, mods, results, {name: texts[name] for name in results})


def listed(root, hooks, packages, source, targets, errors):
    """Move the `mods` rows that need no game into apk_native_mods (LAUNCHER_LIST).

    A HEAD on the `mods` hook whose bindings are all `mods` only pushes the mod's row:
    native_mods runs apk_native_mods first, so Options > Mods lists it as before, and the
    launcher's Mods page, which has no game, lists it too, with its switch."""
    target = hooks.get(('mods', 1))
    if not any(name == HOOKS_FILE for name, _ in targets):
        return
    found = resolve(root, target, packages, source, 'hook "mods"', []) if target else None
    key = (found[0], found[1].open) if found else None
    injections = targets.get(key, [])
    if any(i.kind == 'OVERWRITE' for i in injections):
        return
    rows = [i for i in injections if not i.cancellable and i.bindings
            and all(value_name(b['value']) == 'mods' for b in i.bindings)]
    if not rows:
        return
    found = resolve(root, LAUNCHER_LIST, packages, source, f'{HOOKS_FILE} (the launcher\'s list)', errors)
    if found is None:
        return
    for injection in rows:
        injection.target = LAUNCHER_LIST
        injection.file, injection.function = found
        injections.remove(injection)
        if check_contract(injection, f'{injection.mod.label}/{injection.helper_path}', errors):
            targets.setdefault((found[0], found[1].open), []).append(injection)
    if not injections:
        del targets[key]


def resolve(root, target, packages, source, here, errors):
    """The file and function a target names."""
    segments = split_target(target)
    if len(segments) < 2 or not all(segments):
        errors.append(f'{here}: {target!r} is not a function path')
        return None
    crate, *middle, name = segments
    if crate not in packages:
        errors.append(f'{here}: no workspace crate {crate!r} (in {target})')
        return None
    folder = packages[crate] / 'src'
    files, consumed = [], 0
    for segment in middle:
        if not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', segment):
            break
        for candidate in (folder / f'{segment}.rs', folder / segment / 'mod.rs'):
            if candidate.is_file():
                files, folder, consumed = [candidate], folder / segment, consumed + 1
                break
        else:
            break
    if not files:
        files = [p for p in (folder / 'lib.rs', folder / 'main.rs') if p.is_file()]
    rest = middle[consumed:]
    found = []
    for path in files:
        name_rel = rel(root, path)
        parsed = source(name_rel)
        if parsed is None:
            return None
        for function in parsed.functions:
            if function.name != name:
                continue
            if (function.owner is None and function.path == rest) or \
                    (function.owner is not None and function.path == rest[:-1] and rest and function.owner == rest[-1]):
                found.append((name_rel, function))
    if len(found) != 1:
        errors.append(f'{here}: {target} {"is not a function with a body in" if not found else "names several functions in"} '
                      f'{", ".join(rel(root, p) for p in files)}')
        return None
    return found[0]


def check_contract(injection, where, errors):
    helper, function = injection.helper, injection.function
    good = True
    if len(helper.function.params) != len(injection.bindings):
        errors.append(f'{where}: `{helper.name}` takes {len(helper.function.params)} parameter(s), '
                      f'the bindings give {len(injection.bindings)}')
        return False
    for param, binding in zip(helper.function.params, injection.bindings):
        if param.name != binding['name']:
            errors.append(f'{where}: parameter {param.name or "(pattern)"} is bound as {binding["name"]!r}; '
                          f'the names must agree, in order')
            good = False
            continue
        try:
            ty = normal_type(binding['ty'])
        except ValueError:
            ty = None
        if ty != param.type:
            errors.append(f'{where}: `{param.name}: {param.type}`, but the binding says ty = {binding["ty"]!r}')
            good = False
        if binding['value'].startswith('$'):
            errors.append(f'{where}: special binding {binding["value"]!r} is not supported on Android yet')
            good = False
            continue
        value = value_name(binding['value'])
        receiver = any(p.receiver for p in function.params)
        if value is None or (value == 'self' and not receiver) or (value != 'self' and value not in function.names):
            params = ', '.join(sorted(function.names)) or 'none'
            errors.append(f'{where}: value {binding["value"]!r} is not one of the target\'s parameters ({params}), '
                          f'a borrow (&x, &mut x) or a reborrow (&*x, &mut *x)')
            good = False
        elif value == helper.name:
            errors.append(f'{where}: the function is named `{helper.name}` like the value it is given; rename it')
            good = False
    result = helper.function.result
    if injection.kind == 'OVERWRITE':
        expected = function.result
        if result != expected:
            errors.append(f'{where}: an OVERWRITE returns what the target does ({expected}), not {result}')
            good = False
    elif injection.cancellable:
        inner = control_flow(result)
        if inner != function.result:
            errors.append(f'{where}: a cancellable HEAD returns ControlFlow<{function.result}>, not {result}')
            good = False
    elif result != '()':
        errors.append(f'{where}: a HEAD helper returns nothing (`cancellable = true` to return ControlFlow)')
        good = False
    return good


def block(injection):
    """horizon-mixins' code for one injection: the helper, a local function, and its call."""
    values = ', '.join(b['value'] for b in injection.bindings)
    source = injection.helper.source.strip('\ufeff').rstrip()
    origin = f'// {injection.mod.label}/{injection.helper_path}: {injection.mod.id} injection "{injection.id}"'
    call = f'{injection.helper.name}({values})'
    if injection.kind == 'OVERWRITE':
        return f'{origin}\n{{\n{source}\n{call}\n}}'
    if injection.cancellable:
        return (f'{origin}\nif let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {{\n{source}\n'
                f'{call}\n}} {{\nreturn __horizon_mixins_return;\n}}')
    return f'{origin}\n{{\n{source}\n{call}\n}};'


# ---------------------------------------------------------------------------------------------
# Apply and clean.

def sync(folder, files):
    """Make `folder` hold exactly `files`, writing only what changed (Cargo goes by file times)."""
    folder.mkdir(parents=True, exist_ok=True)
    for name, data in files.items():
        path = folder / name
        if not path.is_file() or path.read_bytes() != data:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    for path in sorted(folder.rglob('*'), reverse=True):
        name = path.relative_to(folder).as_posix()
        if path.is_file() and name not in files:
            path.unlink()
        elif path.is_dir() and not any(path.iterdir()):
            path.rmdir()


def apply(plan):
    """Write the plan's mods into the source, keeping the originals; returns the changed files."""
    root = plan.root
    state = State(root)
    for name in plan.edits:
        record, path = state.files.get(name), root / name
        if record is not None and (digest(path.read_bytes()) if path.is_file() else None) \
                not in (record['applied'], record['original']):
            raise ModError(f'{name} changed since android-native-mods applied the mods: keep your change '
                           f'elsewhere, restore the file with git checkout, then delete {WORK}/')
    clean_except(root, state, set(plan.edits))
    folders = []
    for mod in plan.mods:
        if mod.archive is not None:
            folders.append((f'{WORK}/bundles/{mod.id}', mod.files))
        if mod.data:
            folders.append((f'{DATA}/{mod.id}', mod.data))
    owned = set(state.folders)
    for name, _ in folders:
        if name not in owned and (root / name).exists():
            raise ModError(f'{name} exists and is not android-native-mods\' own: move it away first')
    for name in sorted(owned - {f for f, _ in folders}):
        remove_folder(root, name)
    state.folders = [name for name, _ in folders]
    for name, text in plan.edits.items():
        original = plan.originals[name].encode('utf-8')
        if name not in state.files:
            backup = state.originals / name
            backup.parent.mkdir(parents=True, exist_ok=True)
            backup.write_bytes(original)
        state.files[name] = {'original': digest(original), 'applied': digest(text.encode('utf-8'))}
    state.save()
    for name, files in folders:
        sync(root / name, files)
    changed = []
    for name, text in plan.edits.items():
        path, data = root / name, text.encode('utf-8')
        if path.read_bytes() != data:
            path.write_bytes(data)
            changed.append(name)
    return changed


def remove_folder(root, name):
    path = (root / name).resolve()
    allowed = [(root / DATA).resolve(), (root / WORK / 'bundles').resolve()]
    if any(path.is_relative_to(a) and path != a for a in allowed) and path.is_dir():
        shutil.rmtree(path)
        parent = path.parent
        if parent == (root / DATA).resolve() and not any(parent.iterdir()):
            parent.rmdir()


def clean_except(root, state, keep):
    """Restore every file apply changed but those in `keep`."""
    problems = []
    for name in sorted(set(state.files) - set(keep)):
        record, path = state.files[name], root / name
        now = digest(path.read_bytes()) if path.is_file() else None
        if now == record['applied']:
            path.write_bytes(state.original(name))
        elif now != record['original']:
            problems.append(name)
            continue
        (state.originals / name).unlink(missing_ok=True)
        del state.files[name]
    if problems:
        state.save()
        raise ModError('changed since android-native-mods applied the mods, so not restored: '
                       + ', '.join(problems) + f'. Restore with git checkout, then delete {WORK}/')
    return state


def clean(root=ROOT):
    """Put back what apply changed. True when there was something to restore."""
    root = Path(root).resolve()
    state = State(root)
    if not state.path.is_file():
        return False
    clean_except(root, state, set())
    for name in state.folders:
        remove_folder(root, name)
    state.path.unlink()
    shutil.rmtree(state.originals, ignore_errors=True)
    shutil.rmtree(root / WORK / 'bundles', ignore_errors=True)
    with contextlib.suppress(OSError):
        (root / WORK).rmdir()
    return True


def new(root, mod_id):
    """mods/android-native/<id>/: the template with its id, module and title renamed."""
    root = Path(root).resolve()
    if not re.fullmatch(r'[a-z][a-z0-9_]{0,63}', mod_id):
        raise ModError(f'{mod_id!r}: a new mod\'s id is lower-case letters, digits and _, from a letter')
    folder, template = root / MODS / mod_id, root / TEMPLATE
    if mod_id in bundled(root):
        raise ModError(f'{mod_id!r} is already built into the app ({BUNDLED}/{mod_id}); choose another id')
    if folder.exists():
        raise ModError(f'{rel(root, folder)} already exists')
    module = f'android_{mod_id}'
    if module in Source((root / 'crates/horizon-client/src/game.rs').read_text(encoding='utf-8')).names:
        raise ModError(f'the game already has a `{module}`; choose another id')
    title = json.dumps(mod_id.replace('_', ' ').title())
    renames = [('android_example', module), ('"example"', f'"{mod_id}"'), ('"Example"', title)]
    files = {}
    for path in sorted(template.rglob('*')):
        if path.is_file():
            data = path.read_bytes()
            if path.suffix in ('.rs', '.toml'):
                text = data.decode('utf-8')
                for old, replacement in renames:
                    text = text.replace(old, replacement)
                data = text.encode('utf-8')
            files[path.relative_to(template).as_posix()] = data
    if not files:
        raise ModError(f'no template in {TEMPLATE}')
    sync(folder, files)
    return folder


@contextlib.contextmanager
def applied(plan, log=lambda line: print(line, flush=True)):
    """The mods written into the source for the duration of a build."""
    if plan is None or not plan.mods:
        yield
        return
    try:
        changed = apply(plan)
        for line in plan.summary():
            log(f'Android native mod: {line}')
        log(f'android-native-mods: {len(changed)} file(s) written; they are restored after the build')
        yield
    finally:
        clean(plan.root)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', nargs='?', default='check', choices=['check', 'apply', 'clean', 'new'])
    parser.add_argument('id', nargs='?', help='new: the mod\'s id (lower-case letters, digits, _)')
    parser.add_argument('--root', type=Path, default=ROOT, help='the Horizon checkout (this one)')
    parser.add_argument('--mods', type=Path, help=f'the mods folder (default: <root>/{MODS})')
    args = parser.parse_args(argv)
    if (args.command == 'new') != (args.id is not None):
        parser.error('new takes the mod\'s id; the other commands take none')
    try:
        if args.command == 'new':
            folder = new(args.root, args.id)
            print(f'Made {rel(args.root, folder)}/. Write the mod there, then `python3 {rel(ROOT, __file__)} check`.')
            return 0
        if args.command == 'clean':
            print('Restored the original source.' if clean(args.root) else 'Nothing to restore.')
            return 0
        plan = load(args.root, args.mods)
        if not plan.mods:
            print(f'No Android native mods in {args.mods or MODS}/.')
            return 0
        for line in plan.summary():
            print(line)
        if args.command == 'apply':
            changed = apply(plan)
            print(f'Applied: {len(changed)} file(s) changed. `python3 {rel(ROOT, __file__)} clean` restores them.')
        else:
            print(f'OK: {len(plan.mods)} mod(s), {sum(len(m.injections) for m in plan.mods)} injection(s) '
                  f'in {len(plan.edits)} file(s).')
        return 0
    except ModError as e:
        for line in str(e).splitlines():
            print(f'android-native-mods: error: {line}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
