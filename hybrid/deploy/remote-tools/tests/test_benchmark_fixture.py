from array import array
import importlib.util
from pathlib import Path
import tempfile
import unittest
import wave

SPEC = importlib.util.spec_from_file_location("neural_benchmark", Path(__file__).parents[1] / "benchmark.py")
BENCHMARK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BENCHMARK)


class BenchmarkFixtureTests(unittest.TestCase):
    def test_fixture_is_deterministic_stereo_and_preserves_full_rate(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = (Path(directory) / name for name in ("a.wav", "b.wav"))
            report = BENCHMARK.synthetic(first, 1)
            other = BENCHMARK.synthetic(second, 1)
            self.assertEqual(report["sha256"], other["sha256"])
            self.assertEqual(report["decoded_float_bytes"], 44100 * 2 * 4)
            with wave.open(str(first), "rb") as audio:
                self.assertEqual((audio.getnframes(), audio.getframerate(), audio.getnchannels()), (44100, 44100, 2))
                samples = array("h", audio.readframes(audio.getnframes()))
            self.assertLess(max(map(abs, samples)), 32767)
            self.assertTrue(any(samples[::2]))
            self.assertNotEqual(samples[::2], samples[1::2])


if __name__ == "__main__":
    unittest.main()
