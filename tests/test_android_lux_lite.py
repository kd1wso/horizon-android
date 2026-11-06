"""Structural regressions for the separately bundled mobile Lux Lite pack."""
from pathlib import Path
import re
import unittest
import zipfile

PACK = Path(__file__).resolve().parents[1] / 'shaders/android-native/Lux_Lite_Xiaomi13.zip'

class LuxLiteTests(unittest.TestCase):
    def setUp(self):
        self.archive = zipfile.ZipFile(PACK)
        self.addCleanup(self.archive.close)
    def text(self, name):
        return self.archive.read(name).decode('utf-8')
    def test_credits_and_pack_structure(self):
        for path in ('LICENSE', 'AGREEMENT.txt', 'HORIZON.txt', 'ANDROID.txt',
                     'shaders/world0/gbuffers_water.fsh', 'shaders/world-1/composite.fsh',
                     'shaders/world1/composite.fsh'):
            self.assertIn(path, self.archive.namelist())
        for author in ('GBanan', 'TechDevOnGithub', 'Capt Tatsu'):
            self.assertIn(author, self.text('ANDROID.txt'))
    def test_defaults_and_ultra_are_mobile_budget(self):
        settings = self.text('shaders/lib/settings.glsl')
        for line in (r'const int shadowMapResolution = 512;', r'const float shadowDistance = 64\.0;',
                     r'#define WATER_NORMALS 2\b', r'#define GERSTNER_WAVE_ITERATIONS 2\b',
                     r'#define NOISE_WAVE_ITERATIONS 1\b', r'#define OVERWORLD_REFLECTION_STEPS 12\b',
                     r'// #define SHADOW_COLOR\s*$', r'// #define SHADOW_FILTER\s*$',
                     r'// #define BLOOM\s*$', r'// #define VOLUMETRIC_FOG\s*$'):
            self.assertRegex(settings, re.compile('^' + line, re.MULTILINE))
        props = self.text('shaders/shaders.properties')
        ultra = re.search(r'(?m)^profile\.ULTRA=(.*)$', props).group(1)
        for opt in ('shadowMapResolution=512', 'shadowDistance=64.0', '!SHADOW_FILTER',
                    '!VOLUMETRIC_FOG', '!BLOOM', 'CLOUDS', 'REFLECTION',
                    'GERSTNER_WAVE_ITERATIONS=2', 'NOISE_WAVE_ITERATIONS=1'):
            self.assertIn(opt.split('=')[0] + '=' if '=' in opt else opt, ultra)
            self.assertIn(opt, ultra.split())
        self.assertIn('program.composite4.enabled=BLOOM', props)
    def test_water_and_cloud_work(self):
        water = self.text('shaders/program/gbuffers_water.glsl')
        self.assertIn('float noise = 0.0;', water)
        self.assertNotIn('float noise;', water)
        clouds = self.text('shaders/lib/atmospherics/clouds.glsl')
        self.assertIn('i < CLOUD_NOISE_OCTAVES', clouds)
        self.assertIn('i < 4; i++', clouds)
        self.assertIn('#ifdef AUTO_EXPOSURE\nconst bool colortex0MipmapEnabled',
                      self.text('shaders/program/composite5.glsl').replace('\r\n', '\n'))

if __name__ == '__main__':
    unittest.main()
