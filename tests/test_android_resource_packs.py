"""Android's pinned resource-pack adaptation and staging."""
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'tools'))
import android_resource_packs as packs  # noqa: E402

class BuiltInResources(unittest.TestCase):
    def test_actual_pack_and_determinism(self):
        source = packs.original()
        self.assertEqual(hashlib.sha512(source).hexdigest(), packs.SHA512)
        a = packs.adapted(source)
        self.assertEqual(a, packs.adapted(source))
        with tempfile.TemporaryDirectory() as tmp:
            with zipfile.ZipFile(__import__('io').BytesIO(a)) as z:
                self.assertIsNone(z.testzip())
                meta = json.loads(z.read('pack.mcmeta'))['pack']
                self.assertEqual((meta['min_format'], meta['max_format']), ([97, 1], [97, 1]))
                self.assertIn(b'Kenui', z.read('LICENSE-KENUI.txt'))
                self.assertIn('assets/minecraft/textures/entity/equipment/humanoid/leather.png', z.namelist())
            out = Path(tmp); (out / 'stale.zip').write_bytes(b'old')
            self.assertEqual([p.name for p in packs.write(out)], [packs.NAME])
            self.assertEqual((out / packs.NAME).read_bytes(), a)
            self.assertFalse((out / 'stale.zip').exists())
    def test_bad_png_rejected(self):
        with self.assertRaises(ValueError): packs.fix_png(b'bad')

if __name__ == '__main__': unittest.main()
