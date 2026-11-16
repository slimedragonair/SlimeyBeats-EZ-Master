"""Header and decoded allocation attacks fail before the CLI is invoked."""

import math
from pathlib import Path
import struct
import sys
import tempfile
import unittest

from hybrid.server.audio_guard import inspect_wav
from hybrid.server.config import Config
from hybrid.server.errors import ProcessingError
from hybrid.tests.support import TOKEN, running_service, wav_bytes


def custom_wav(encoding=1, bits=16, channels=1, rate=48000, data=b"\0\0" * 4800, metadata=b""):
    align = channels * (bits // 8)
    fmt = struct.pack("<HHIIHH", encoding, channels, rate, rate * align, align, bits)
    body = b"WAVEfmt " + struct.pack("<I", len(fmt)) + fmt + metadata + b"data" + struct.pack("<I", len(data)) + data
    if len(data) % 2:
        body += b"\0"
    return b"RIFF" + struct.pack("<I", len(body)) + body


class AudioGuardTests(unittest.TestCase):
    def inspect(self, data, **limits):
        with tempfile.TemporaryDirectory(prefix="hybrid-wav-guard-") as temp:
            config = Config(cli=Path(sys.executable), token=TOKEN, **limits)
            path = Path(temp) / "input.wav"
            path.write_bytes(data)
            return inspect_wav(path, config)

    def test_pcm_and_float_layouts(self):
        self.assertEqual(self.inspect(wav_bytes()).decoded_bytes, 4800 * 4)
        data = struct.pack("<f", 0.25) * 4800
        self.assertEqual(self.inspect(custom_wav(encoding=3, bits=32, data=data)).frames, 4800)
        for bits in (8, 16, 24, 32):
            self.assertEqual(self.inspect(custom_wav(bits=bits, data=b"\0" * (4800 * bits // 8))).frames, 4800)

    def test_rejects_nonfinite_or_compressed_wav(self):
        for data in (custom_wav(encoding=3, bits=32, data=struct.pack("<f", math.nan)),
                     custom_wav(encoding=3, bits=32, data=struct.pack("<f", math.inf)),
                     custom_wav(encoding=6), custom_wav(channels=3), custom_wav(rate=7000),
                     custom_wav(encoding=3, bits=64)):
            with self.subTest(header=data[20:36]), self.assertRaises(ProcessingError):
                self.inspect(data)

    def test_declared_bytes_and_frames_are_bounded(self):
        data = wav_bytes()
        with self.assertRaises(ProcessingError):
            self.inspect(data, max_decoded_bytes=4800 * 4 - 1)
        self.assertEqual(self.inspect(data, max_decoded_bytes=4800 * 4).frames, 4800)
        with self.assertRaises(ProcessingError):
            self.inspect(wav_bytes(1.001), max_audio_seconds=1)
        self.assertEqual(self.inspect(wav_bytes(1), max_audio_seconds=1).frames, 48000)
        for bad in (data[:-1], data + b"secret", data[:40] + struct.pack("<I", 0xFFFFFFFF) + data[44:],
                    data[:4] + struct.pack("<I", 0xFFFFFFFF) + data[8:], b"RF64" + data[4:]):
            with self.assertRaises(ProcessingError):
                self.inspect(bad)

    def test_duplicate_chunks_and_block_alignment_are_rejected(self):
        data = custom_wav(channels=2, data=b"\0" * 3)
        with self.assertRaises(ProcessingError):
            self.inspect(data)
        duplicate = b"data" + struct.pack("<I", 4) + b"\0" * 4
        with self.assertRaises(ProcessingError):
            self.inspect(custom_wav(metadata=duplicate))
        hostile = b"JUNK" + struct.pack("<I", 0xFFFFFFFE)
        with self.assertRaises(ProcessingError):
            self.inspect(custom_wav(metadata=hostile))

    def test_http_invalid_wav_never_invokes_native_runner(self):
        with running_service() as (service, client):
            called = []
            def forbidden(*args, **kwargs):
                called.append(True)
                raise AssertionError("Invalid WAV reached native code")
            service.worker.runner.run = forbidden
            invalid = custom_wav(encoding=6)
            job = client.create(invalid)[2]["id"]
            client.upload(job, invalid)
            self.assertEqual(client.request("POST", f"/v1/jobs/{job}/run", {})[0], 200)
            self.assertEqual(client.wait(job)["state"], "failed")
            self.assertEqual(called, [], "Invalid WAV reached native code")
            self.assertEqual([item.name for item in service.store.jobs[job].directory.iterdir()], [".slimeybeats-hybrid-job"])
