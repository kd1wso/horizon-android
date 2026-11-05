"""tools/android_native_mods.py: native mods written into the source for the Android build."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import textwrap
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('android_native_mods', ROOT / 'tools/android_native_mods.py')
native = importlib.util.module_from_spec(spec)
spec.loader.exec_module(native)

GAME = 'crates/horizon-client/src/game.rs'
HOOKS = native.HOOKS_FILE
REVISION = 'a' * 40

MANIFEST = '''
[mod]
id = "demo"
version = "1.0.0"

[[module]]
id = "state"
file = "crates/horizon-client/src/game.rs"
name = "android_demo"
source = "state.rs"

[[injection]]
id = "tick"
kind = "HEAD"
hook = "tick"
hook_version = 1
helper = "tick.rs"
bindings = [{ name = "game", ty = "&mut crate::game::Game<'_>", value = "&mut *self" }]

[[injection]]
id = "wheel"
kind = "HEAD"
cancellable = true
hook = "wheel"
hook_version = 1
helper = "wheel.rs"
bindings = [
  { name = "steps", ty = "f32", value = "steps" },
  { name = "claimed", ty = "&mut bool", value = "&mut *claimed" },
]
'''
FILES = {
    'state.rs': 'pub(super) fn tick(_game: &mut super::Game<\'_>) {}\n',
    'tick.rs': '// Every tick.\nfn on_tick(game: &mut crate::game::Game<\'_>) {\n'
               '    crate::game::android_demo::tick(game);\n}\n',
    'wheel.rs': 'fn on_wheel(steps: f32, claimed: &mut bool) -> ::core::ops::ControlFlow<()> {\n'
                '    if steps > 0.0 {\n        *claimed = true;\n'
                '        return ::core::ops::ControlFlow::Break(());\n    }\n'
                '    ::core::ops::ControlFlow::Continue(())\n}\n',
    'data/levels.txt': '1 2 3\n',
}


def write(folder, files):
    for name, text in files.items():
        path = folder / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')


def hnmod(path, manifest=MANIFEST, files=FILES, extra=None, revision=REVISION, mod_id='demo'):
    with zipfile.ZipFile(path, 'w') as archive:
        archive.writestr('horizon-native.json', json.dumps({
            'schema': 1, 'id': mod_id, 'version': '1.0.0', 'game': '26.1', 'horizon': REVISION,
            'mixins': 'b' * 40}))
        compatibility = f'\n[compatibility]\nhorizon_revision = "{revision}"\n'
        archive.writestr(f'mixins/{mod_id}/horizon-mixin.toml', manifest + compatibility)
        for name, text in {**files, **(extra or {})}.items():
            archive.writestr(f'mixins/{mod_id}/{name}', text)


class Workspace(unittest.TestCase):
    """A workspace of horizon-client's real game.rs and native hooks, with mods/android-native."""

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        write(self.root, {'Cargo.toml': '[workspace]\nmembers = ["crates/horizon-client"]\n',
                          'crates/horizon-client/Cargo.toml': '[package]\nname = "horizon-client"\n'})
        for name in (GAME, HOOKS, 'native-hooks.json'):
            (self.root / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, self.root / name)
        (self.root / native.BUNDLED / 'zoom').mkdir(parents=True)
        self.mods = self.root / native.MODS
        self.mods.mkdir(parents=True)

    def tearDown(self):
        self.temporary.cleanup()

    def mod(self, manifest=MANIFEST, files=FILES, name='demo'):
        write(self.mods / name, {'horizon-mixin.toml': manifest, **files})

    def error(self, manifest=MANIFEST, files=FILES, **changes):
        """The problems load finds in the demo mod, changed by `changes` (old text -> new)."""
        for old, new in changes.items():
            self.assertIn(old, manifest)
            manifest = manifest.replace(old, new)
        self.mod(manifest, files)
        with self.assertRaises(native.ModError) as caught:
            native.load(self.root)
        return str(caught.exception)

    def original(self, name):
        return (ROOT / name).read_bytes()

    def function(self, name, text):
        """The body of `fn name` in `text`, up to the next function."""
        start = text.index(f'fn {name}(')
        return text[start:text.index('\n    pub(super) fn ', start)]


class CatalogTests(unittest.TestCase):
    def test_every_hook_of_the_catalog_is_a_function_of_this_source(self):
        hooks, packages, sources, errors = native.catalog(ROOT), native.crates(ROOT), {}, []

        def source(name):
            if name not in sources:
                sources[name] = native.Source((ROOT / name).read_text(encoding='utf-8'))
            return sources[name]
        self.assertGreater(len(hooks), 100)
        for (name, version), target in sorted(hooks.items()):
            found = native.resolve(ROOT, target, packages, source, f'{name} v{version}', errors)
            self.assertIsNotNone(found, errors)
        self.assertEqual(errors, [])

    def test_the_template_mod_fits_this_source(self):
        with tempfile.TemporaryDirectory() as folder:
            shutil.copytree(ROOT / native.MODS / '_example', Path(folder) / 'example')
            plan = native.load(ROOT, folder)
        self.assertEqual([m.id for m in plan.mods], ['example'])
        self.assertEqual(len(plan.mods[0].injections), 6)
        self.assertEqual(sorted(plan.edits), [GAME, HOOKS])

    def test_the_template_is_not_built(self):
        with tempfile.TemporaryDirectory() as folder:
            shutil.copytree(ROOT / native.MODS / '_example', Path(folder) / '_example')
            (Path(folder) / 'README.md').write_text('notes')
            self.assertEqual(native.load(ROOT, folder).mods, [])


class ApplyTests(Workspace):
    def test_apply_writes_the_mod_and_clean_restores_the_source(self):
        self.mod()
        plan = native.load(self.root)
        self.assertEqual(sorted(native.apply(plan)), [GAME, HOOKS])
        hooks = (self.root / HOOKS).read_text(encoding='utf-8')
        tick = self.function('native_tick', hooks)
        self.assertIn(f'{{\n{native.BEGIN}\n// mods/android-native/demo/tick.rs: demo injection "tick"\n'
                      '{\n// Every tick.\nfn on_tick(', tick)
        self.assertIn('on_tick(&mut *self)\n};', tick)
        wheel = self.function('native_wheel', hooks)
        self.assertIn('if let ::core::ops::ControlFlow::Break(__horizon_mixins_return) = {\nfn on_wheel(', wheel)
        self.assertIn('on_wheel(steps, &mut *claimed)\n} {\nreturn __horizon_mixins_return;\n}', wheel)
        self.assertLess(wheel.index(native.BEGIN), wheel.index('android_bundled::native_wheel'))
        game = (self.root / GAME).read_text(encoding='utf-8')
        self.assertTrue(game.endswith('#[path = "../../../mods/android-native/demo/state.rs"]\n'
                                      f'pub(crate) mod android_demo;\n{native.END}\n'))
        self.assertEqual((self.root / 'native-data/demo/levels.txt').read_text(), '1 2 3\n')
        self.assertEqual((self.root / native.WORK / 'originals' / HOOKS).read_bytes(), self.original(HOOKS))

        times = {name: (self.root / name).stat().st_mtime_ns for name in (GAME, HOOKS, 'native-data/demo/levels.txt')}
        self.assertEqual(native.apply(native.load(self.root)), [])
        self.assertEqual(times, {name: (self.root / name).stat().st_mtime_ns for name in times})

        self.assertTrue(native.clean(self.root))
        for name in (GAME, HOOKS):
            self.assertEqual((self.root / name).read_bytes(), self.original(name))
        self.assertFalse((self.root / native.DATA).exists())
        self.assertFalse((self.root / native.WORK).exists())
        self.assertFalse(native.clean(self.root))

    def test_a_mod_taken_out_is_taken_out_of_the_source(self):
        self.mod()
        native.apply(native.load(self.root))
        shutil.rmtree(self.mods / 'demo')
        plan = native.load(self.root)
        self.assertEqual(plan.mods, [])
        with native.applied(plan):
            pass
        native.clean(self.root)
        self.assertEqual((self.root / HOOKS).read_bytes(), self.original(HOOKS))
        self.assertFalse((self.root / native.DATA).exists())

    def test_a_failed_build_still_restores_the_source(self):
        self.mod()
        lines = []
        with self.assertRaises(RuntimeError):
            with native.applied(native.load(self.root), log=lines.append):
                self.assertIn(native.BEGIN, (self.root / HOOKS).read_text(encoding='utf-8'))
                raise RuntimeError('cargo failed')
        self.assertIn('Android native mod: demo 1.0.0 (mods/android-native/demo): 2 injection(s) on tick, wheel, '
                      '1 module(s), 1 data file(s)', lines)
        for name in (GAME, HOOKS):
            self.assertEqual((self.root / name).read_bytes(), self.original(name))

    def test_edits_made_while_applied_are_never_overwritten(self):
        self.mod()
        native.apply(native.load(self.root))
        with (self.root / HOOKS).open('a', encoding='utf-8') as file:
            file.write('// mine\n')
        with self.assertRaisesRegex(native.ModError, 'native_hooks.rs'):
            native.clean(self.root)
        with self.assertRaisesRegex(native.ModError, 'changed since'):
            native.apply(native.load(self.root))
        self.assertTrue((self.root / HOOKS).read_text(encoding='utf-8').endswith('// mine\n'))
        shutil.copyfile(ROOT / HOOKS, self.root / HOOKS)
        self.assertTrue(native.clean(self.root))
        self.assertEqual((self.root / GAME).read_bytes(), self.original(GAME))

    def test_source_with_unrecorded_mod_code_is_refused(self):
        self.mod()
        with (self.root / HOOKS).open('a', encoding='utf-8') as file:
            file.write(f'{native.BEGIN}\n{native.END}\n')
        with self.assertRaisesRegex(native.ModError, 'apply did not record'):
            native.load(self.root)

    def test_overwrite_replaces_the_body_of_a_target(self):
        manifest = '''
[mod]
id = "demo"
version = "1.0.0"

[[injection]]
id = "wheel_event"
kind = "OVERWRITE"
target = "horizon_client::game::native_hooks::Game<'_>::native_wheel_event"
helper = "event.rs"
bindings = [{ name = "steps", ty = "f32", value = "steps" }]
'''
        self.mod(manifest, {'event.rs': 'fn event(steps: f32) -> bool {\n    steps > 0.0\n}\n'})
        native.apply(native.load(self.root))
        body = self.function('native_wheel_event', (self.root / HOOKS).read_text(encoding='utf-8'))
        self.assertIn('{\nfn event(steps: f32) -> bool {\n    steps > 0.0\n}\nevent(steps)\n}\n', body)
        self.assertNotIn('let mut claimed', body)

    def test_hnmod_bundles_are_built_from_their_own_copy(self):
        hnmod(self.mods / 'demo.hnmod')
        plan = native.load(self.root)
        self.assertEqual([m.id for m in plan.mods], ['demo'])
        native.apply(plan)
        game = (self.root / GAME).read_text(encoding='utf-8')
        self.assertIn('#[path = "../../../.local/android-native-mods/bundles/demo/state.rs"]', game)
        self.assertTrue((self.root / native.WORK / 'bundles/demo/state.rs').is_file())
        native.clean(self.root)
        self.assertFalse((self.root / native.WORK).exists())
        self.assertEqual((self.root / GAME).read_bytes(), self.original(GAME))

    def test_bad_hnmod_bundles(self):
        hnmod(self.mods / 'escape.hnmod', extra={'../escape.rs': ''})
        hnmod(self.mods / 'other.hnmod', revision='c' * 40)
        with self.assertRaises(native.ModError) as caught:
            native.load(self.root)
        problems = str(caught.exception)
        self.assertIn('escape.hnmod: invalid path in the bundle: mixins/demo/../escape.rs', problems)
        self.assertIn('other.hnmod: horizon-native.json and horizon-mixin.toml differ', problems)

    def test_everything_else_in_the_folder_is_an_error(self):
        self.mod()
        write(self.mods, {'_draft/horizon-mixin.toml': 'not toml', 'README.md': 'notes', 'stray.json': '{}'})
        write(self.mods, {'unpacked/horizon-native.json': '{}'})
        with self.assertRaises(native.ModError) as caught:
            native.load(self.root)
        problems = str(caught.exception).splitlines()
        self.assertEqual(len(problems), 2, problems)
        self.assertIn('stray.json: not a mod', problems[0])
        self.assertIn('unpacked: no horizon-mixin.toml (an unpacked .hnmod', problems[1])


class NewTests(Workspace):
    def test_new_mods_are_the_template_with_their_id(self):
        shutil.copytree(ROOT / native.TEMPLATE, self.root / native.TEMPLATE)
        folder = native.new(self.root, 'my_mod')
        self.assertEqual(folder.name, 'my_mod')
        runtime = (folder / 'runtime.rs').read_text(encoding='utf-8')
        self.assertIn('pub(super) const ID: &str = "my_mod";', runtime)
        self.assertIn('const TITLE: &str = "My Mod";', runtime)
        plan = native.load(self.root)
        self.assertEqual([m.id for m in plan.mods], ['my_mod'])
        self.assertIn('pub(crate) mod android_my_mod;', plan.edits[GAME])
        for path in folder.iterdir():
            self.assertNotIn('example', path.read_text(encoding='utf-8').lower(), path.name)
        for bad, problem in [('my_mod', 'already exists'), ('zoom', 'already built into the app'),
                             ('input', 'already has a `android_input`'), ('My-Mod', 'lower-case')]:
            with self.assertRaisesRegex(native.ModError, problem):
                native.new(self.root, bad)


class ProblemTests(Workspace):
    def test_unsupported_injections_and_tables(self):
        self.assertIn('RETURN is not supported on Android yet',
                      self.error(**{'kind = "HEAD"\nhook = "tick"': 'kind = "RETURN"\nhook = "tick"'}))
        self.assertIn('bundle crates ([[crate]]) are not supported on Android yet',
                      self.error(MANIFEST + '\n[[crate]]\nname = "x"\n'))
        self.assertIn('HEAD takes no selector', self.error(**{'helper = "tick.rs"': 'helper = "tick.rs"\nselector = "x"'}))
        self.assertIn("unknown field colour", self.error(**{'helper = "tick.rs"': 'helper = "tick.rs"\ncolour = 1'}))

    def test_hooks_and_targets_that_are_not_there(self):
        self.assertIn("no hook 'whel' in native-hooks.json (did you mean wheel",
                      self.error(**{'hook = "wheel"': 'hook = "whel"'}))
        self.assertIn("hook 'tick' has version(s) [1], not 9",
                      self.error(**{'hook = "tick"\nhook_version = 1': 'hook = "tick"\nhook_version = 9'}))
        self.assertIn('is not a function with a body', self.error(**{
            'hook = "tick"\nhook_version = 1': 'target = "horizon_client::game::native_hooks::Game<\'_>::no_such"'}))
        self.assertIn('helper missing.rs is missing', self.error(**{'helper = "tick.rs"': 'helper = "missing.rs"'}))

    def test_helpers_must_fit_their_bindings(self):
        self.assertIn("`steps: f32`, but the binding says ty = '&mut f32'",
                      self.error(**{'ty = "f32"': 'ty = "&mut f32"'}))
        self.assertIn("value 'steps + 1.0' is not one of the target's parameters",
                      self.error(**{'value = "steps"': 'value = "steps + 1.0"'}))
        self.assertIn('a cancellable HEAD returns ControlFlow<()>', self.error(
            files={**FILES, 'wheel.rs': 'fn on_wheel(steps: f32, claimed: &mut bool) -> ::core::ops::ControlFlow<bool> '
                                        '{\n    ::core::ops::ControlFlow::Continue(())\n}\n'}))
        self.assertIn('a HEAD helper returns nothing', self.error(
            files={**FILES, 'tick.rs': 'fn on_tick(game: &mut crate::game::Game<\'_>) -> bool {\n    true\n}\n'}))
        self.assertIn('a helper is one private function', self.error(
            files={**FILES, 'tick.rs': '#[inline]\n' + FILES['tick.rs'].split('\n', 1)[1]}))
        self.assertIn('the function is named `claimed` like the value it is given', self.error(
            files={**FILES, 'wheel.rs': FILES['wheel.rs'].replace('fn on_wheel', 'fn claimed')}))
        self.assertIn('parameter game is bound as', self.error(**{'name = "game"': 'name = "player"'}))

    def test_names_ids_and_overwrites_that_clash(self):
        self.assertIn('crates/horizon-client/src/game.rs already has a `android_bundled`',
                      self.error(**{'name = "android_demo"': 'name = "android_bundled"'}))
        self.assertIn("'zoom' is already built into the app", self.error(**{'id = "demo"': 'id = "zoom"'}))
        overwrite = MANIFEST + '''
[[injection]]
id = "replace"
kind = "OVERWRITE"
hook = "tick"
hook_version = 1
helper = "tick.rs"
bindings = [{ name = "game", ty = "&mut crate::game::Game<'_>", value = "&mut *self" }]
'''
        self.assertIn("an OVERWRITE (demo 'replace') conflicts with every other injection there (demo 'tick')",
                      self.error(overwrite))
        self.mod(name='again')
        self.assertIn("id 'demo' is also mods/android-native/again's", self.error())


class LexerTests(unittest.TestCase):
    def test_literals_comments_and_lifetimes(self):
        source = native.Source(textwrap.dedent('''
            /* a /* nested */ fn hidden() {} */
            const R: &str = r#"fn fake() { "quoted" }"#;
            impl<'a> Foo<'a> {
                pub(crate) fn bar(&mut self, x: &'a str) -> Option<char> { let _ = '\\''; Some('{') }
            }
            impl Display for Foo<'_> { fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { Ok(()) } }
            fn bounded<T>(t: T) where T: Fn() -> u8 {}
            mod inner { fn deep() {} }
            '''))
        found = {f.name: f for f in source.functions}
        self.assertEqual(sorted(found), ['bar', 'bounded', 'deep', 'fmt'])
        self.assertEqual(found['bar'].owner, "Foo<'_>")
        self.assertEqual([(p.name, p.type) for p in found['bar'].params], [('self', None), ('x', "&'_ str")])
        self.assertEqual(found['bar'].result, 'Option<char>')
        self.assertEqual(found['fmt'].owner, "<Foo<'_> as Display>")
        self.assertEqual(found['bounded'].result, '()')
        self.assertEqual(found['deep'].path, ['inner'])
        self.assertEqual(sorted(source.names), ['R', 'bounded', 'inner'])
        self.assertEqual([t.kind for t in native.lex("b'\\\\' 'a 'static '\\u{1F600}' c\"x\" br##\"y\"##")],
                         ['literal', 'lifetime', 'lifetime', 'literal', 'literal', 'literal'])

    def test_types_agree_however_they_are_spaced(self):
        self.assertEqual(native.normal_type("&mut  Vec < &'static str >"), "&mut Vec<&'static str>")
        self.assertEqual(native.normal_type("&'a mut Game<'a>"), "&'_ mut Game<'_>")


if __name__ == '__main__':
    unittest.main()
