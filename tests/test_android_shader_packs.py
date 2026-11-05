"""Shader packs built into the Android APK from shaders/android-native (tools/android_shader_packs.py)."""
import os
from pathlib import Path
import re
import sys
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import android_shader_packs as packs  # noqa: E402

LUX = packs.FOLDER / 'Lux_v1.2_Horizon_Android'


def names(path):
    with zipfile.ZipFile(path) as archive:
        return archive.namelist()


class RepositoryTests(unittest.TestCase):
    def test_the_repositorys_packs_have_no_problems(self):
        found, problems = packs.packs()
        self.assertEqual(problems, [])
        self.assertIn('Lux_v1.2_Horizon_Android.zip', [name for name, _ in found])

    def test_lux_keeps_its_authors_and_terms(self):
        for name in ('AGREEMENT.txt', 'HORIZON.txt', 'ANDROID.txt', 'LICENSE'):
            self.assertTrue((LUX / name).is_file(), name)
        android = (LUX / 'ANDROID.txt').read_text(encoding='utf-8')
        for author in ('GBanan', 'TechDevOnGithub', 'Capt Tatsu'):
            self.assertIn(author, android)
        self.assertIn('GNU GENERAL PUBLIC LICENSE', (LUX / 'LICENSE').read_text(encoding='utf-8'))

    def test_lux_starts_with_the_android_profile(self):
        shaders = LUX / 'shaders'
        settings = (shaders / 'lib/settings.glsl').read_text(encoding='utf-8')
        for line in (r'const int shadowMapResolution = 1024;', r'const float shadowDistance = 128\.0;',
                     r'#define AA 1\b', r'#define SHADOW_ADVANCED_FILTER 0\b', r'// #define AO\s*$',
                     r'// #define VOLUMETRIC_FOG\s*$'):
            self.assertRegex(settings, re.compile('^' + line, re.MULTILINE))
        properties = (shaders / 'shaders.properties').read_text(encoding='utf-8')
        self.assertRegex(properties, re.compile(r'^profile\.ANDROID=profile\.LOW .*AA=1', re.MULTILINE))
        self.assertIn('profile.ANDROID=', (shaders / 'lang/en_US.lang').read_text(encoding='utf-8'))

    def test_lux_is_zipped_with_shaders_at_the_top(self):
        with tempfile.TemporaryDirectory() as out:
            written = packs.write(Path(out))
            lux = Path(out) / 'Lux_v1.2_Horizon_Android.zip'
            self.assertIn(lux, written)
            entries = names(lux)
            for name in ('shaders/shaders.properties', 'shaders/lib/settings.glsl', 'shaders/world0/composite.fsh',
                         'ANDROID.txt', 'LICENSE'):
                self.assertIn(name, entries)


class ZipTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.folder = self.root / 'android-native'
        self.folder.mkdir()
        self.out = self.root / 'out'

    def file(self, relative, text='x'):
        path = self.folder / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
        return path

    def zip(self, relative, entries):
        path = self.folder / relative
        with zipfile.ZipFile(path, 'w') as archive:
            for entry in entries:
                archive.writestr(entry, 'x')
        return path

    def test_a_folder_becomes_a_zip_of_its_files_without_hidden_ones(self):
        self.file('Pack/shaders/shaders.properties')
        self.file('Pack/shaders/final.fsh')
        self.file('Pack/README.txt')
        self.file('Pack/.DS_Store')
        self.file('Pack/shaders/.cache/x')
        self.file('README.md')
        [written] = packs.write(self.out, self.folder)
        self.assertEqual(written.name, 'Pack.zip')
        self.assertEqual(names(written), ['README.txt', 'shaders/final.fsh', 'shaders/shaders.properties'])

    def test_the_same_files_always_make_the_same_zip(self):
        source = self.file('Pack/shaders/shaders.properties')
        first = packs.write(self.out, self.folder)[0].read_bytes()
        os.utime(source, (0, 0))
        self.assertEqual(packs.write(self.out, self.folder)[0].read_bytes(), first)

    def test_zips_go_in_as_they_are_and_others_are_removed_from_the_output(self):
        original = self.zip('Ready.zip', ['shaders/final.fsh']).read_bytes()
        self.out.mkdir()
        (self.out / 'Old.zip').write_text('old', encoding='utf-8')
        (self.out / 'Old.zip.partial').write_text('old', encoding='utf-8')
        written = packs.write(self.out, self.folder)
        self.assertEqual([p.name for p in written], ['Ready.zip'])
        self.assertEqual(sorted(p.name for p in self.out.iterdir()), ['Ready.zip'])
        self.assertEqual((self.out / 'Ready.zip').read_bytes(), original)

    def test_what_is_not_a_pack_is_a_problem(self):
        self.file('NoShaders/final.fsh')
        self.file('notes.txt')
        self.file('Broken.zip', 'not a zip')
        self.zip('Flat.zip', ['final.fsh'])
        self.file('_Template/final.fsh')
        self.file('.hidden/final.fsh')
        self.file('README.md')
        found, problems = packs.packs(self.folder)
        self.assertEqual(found, [])
        self.assertEqual(len(problems), 4, problems)
        for text in ('NoShaders: no shaders/ folder', 'notes.txt: not a shader pack', 'Broken.zip: not a ZIP',
                     'Flat.zip: no shaders/ folder at the top'):
            self.assertTrue(any(text in p for p in problems), (text, problems))
        with self.assertRaises(SystemExit):
            packs.write(self.out, self.folder)

    def test_two_packs_with_one_name_are_a_problem(self):
        self.file('Same/shaders/final.fsh')
        self.zip('same.zip', ['shaders/final.fsh'])
        _, problems = packs.packs(self.folder)
        self.assertEqual(len(problems), 1, problems)
        self.assertIn('same.zip: two packs', problems[0])


if __name__ == '__main__':
    unittest.main()
