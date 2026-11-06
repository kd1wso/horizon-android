"""Regression checks for the Android Lux Overworld quality budgets."""
import math
from pathlib import Path
import re
import unittest

SHADERS = (Path(__file__).resolve().parents[1] / 'shaders/android-native/'
           'Lux_v1.2_Horizon_Android/shaders')


def read(name):
    return (SHADERS / name).read_text(encoding='utf-8')


class OverworldQualityTests(unittest.TestCase):
    def test_defaults_and_quality_profiles(self):
        settings = read('lib/settings.glsl')
        self.assertRegex(settings, r'(?m)^#define CLOUD_NOISE_OCTAVES 4\s+// \[4 6\]')
        self.assertRegex(settings, r'(?m)^#define OVERWORLD_REFLECTION_STEPS 20\s+// \[12 20 30\]')
        properties = read('shaders.properties')
        low = re.search(r'(?m)^profile\.LOW=(.*)$', properties).group(1)
        android = re.search(r'(?m)^profile\.ANDROID=(.*)$', properties).group(1)
        medium = re.search(r'(?m)^profile\.MEDIUM=(.*)$', properties).group(1)
        for option in ('CLOUD_NOISE_OCTAVES=4', 'OVERWORLD_REFLECTION_STEPS=20'):
            self.assertIn(option, low)
        self.assertIn('profile.LOW', android)
        for option in ('CLOUD_NOISE_OCTAVES=6', 'OVERWORLD_REFLECTION_STEPS=30'):
            self.assertIn(option, medium)
        for page, option in (('CLOUDS', 'CLOUD_NOISE_OCTAVES'),
                             ('MATERIAL_REFLECTION_SUPPORT', 'OVERWORLD_REFLECTION_STEPS')):
            self.assertRegex(properties, rf'(?m)^screen\.{page}=.*\b{option}\b')
            self.assertIn('option.' + option + '=', read('lang/en_US.lang'))

    def test_cloud_budget_and_average_normalization(self):
        clouds = read('lib/atmospherics/clouds.glsl')
        self.assertIn('i < CLOUD_NOISE_OCTAVES', clouds)
        self.assertIn('i < 6; i++', clouds)  # Preserve six cloud layers.
        value = float(re.search(r'9\.0 \* (1\.\d+)', clouds).group(1))
        expected = sum(0.7 ** i for i in range(6)) / sum(0.7 ** i for i in range(4))
        self.assertTrue(math.isclose(value, expected, abs_tol=0.00001))

    def test_reflection_budget_does_not_change_other_dimensions(self):
        trace = read('lib/reflections/raytrace.glsl')
        self.assertRegex(trace, r'#ifdef OVERWORLD\s+for \(int i = 0; i < OVERWORLD_REFLECTION_STEPS; i\+\+\)\s+#else\s+for \(int i = 0; i < 30; i\+\+\)\s+#endif')
        for world, flag in (('world0', 'OVERWORLD'), ('world-1', 'NETHER'), ('world1', 'END')):
            self.assertIn('#define ' + flag, read(world + '/gbuffers_water.fsh'))


if __name__ == '__main__':
    unittest.main()
