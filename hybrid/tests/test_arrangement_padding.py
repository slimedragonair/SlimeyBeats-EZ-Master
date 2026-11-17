"""Preview-only codec tails preserve real PCM, source clocks and cancellation."""
import array
import hashlib
import json
import math
import os
import struct
import subprocess
import time
import unittest
from types import SimpleNamespace
from unittest.mock import patch

from hybrid.server.arrangement_preview_formats import PREVIEW_RATE, preview_padding
from hybrid.server.arrangement_stream import ArrangementStream
from hybrid.server.errors import ProcessingError
from hybrid.tests.project_support import project_service, create_project, upload, native_studio


class CapturedInput:
    def __init__(self, on_flush=None, short_write=False):
        self.data = bytearray()
        self.on_flush, self.short_write, self.closed = on_flush, short_write, False

    def write(self, data):
        count = len(data) - 4 if self.short_write else len(data)
        self.data.extend(data[:count])
        return count

    def flush(self):
        if self.on_flush:
            self.on_flush()

    def close(self):
        self.closed = True


def feed(rate, frames, container='mp4', interruption=None):
    requests = []
    state = SimpleNamespace(closed=False, project=SimpleNamespace(deleting=False, deadline=time.monotonic() + 30))
    remaining = frames

    def block(request, **unused):
        nonlocal remaining
        requests.append(request['frames'])
        if interruption == 'native_error':
            raise ProcessingError('Injected native failure')
        count = 0 if interruption == 'early_eof' else min(request['frames'], remaining)
        remaining -= count
        return {'frames': count, 'samples': [.25, -.5] * count}

    state.native = SimpleNamespace(call=block)
    stream = ArrangementStream(state, {'stream_id': 'fixture', 'sample_rate': rate, 'channels': 2,
        'start_frame': 100, 'side_duration_frames': 100 + frames, 'preview_container': container})

    def interrupt():
        if interruption == 'cancel': stream.cancel.set()
        elif interruption == 'closed': state.closed = True
        elif interruption == 'deleted': state.project.deleting = True
        elif interruption == 'expired': state.project.deadline = 0

    sink = CapturedInput(interrupt, interruption == 'short_write')
    stream.process = SimpleNamespace(stdin=sink)
    stream._feed()
    return stream, sink, requests


class ArrangementPaddingTests(unittest.TestCase):
    def test_integer_geometry_zero_one_frame_twenty_ms_and_minimum_at_each_rate(self):
        for rate, minimum, one_frame_padding in [(8000, 960, 5754), (32000, 3840, 5759),
                (44100, 5292, 5759), (48000, 5760, 5759), (96000, 11520, 5760), (192000, 23040, 5760)]:
            with self.subTest(rate=rate):
                self.assertEqual(preview_padding(0, rate), (0, 0))
                self.assertEqual(preview_padding(1, rate), (minimum - 1, one_frame_padding))
                self.assertEqual(preview_padding(rate // 50, rate), (minimum - rate // 50, 4800))
                self.assertEqual(preview_padding(minimum, rate), (0, 0))
                self.assertEqual(preview_padding(minimum + 1, rate), (0, 0))
                self.assertEqual(preview_padding(rate * 20, rate), (0, 0))
        # Nonstandard integer rates must ceiling-round both clock conversions.
        self.assertEqual(preview_padding(1, 8001), (960, 5760))
        self.assertEqual(preview_padding(961, 8001), (0, 0))

    def test_only_zero_tail_is_written_after_exact_real_frames_without_extra_native_calls(self):
        for rate in (8000, 32000, 44100, 48000, 96000, 192000):
            for frames in (0, 1, rate // 50, rate * 3 // 25, rate * 3 // 25 + 1):
                for container in ('mp3', 'mp4'):
                    with self.subTest(rate=rate, frames=frames, container=container):
                        stream, sink, requests = feed(rate, frames, container)
                        tail, _ = preview_padding(frames, rate)
                        real = struct.pack('<ff', .25, -.5) * frames
                        self.assertEqual(bytes(sink.data), real + bytes(tail * 8))
                        self.assertEqual(sum(requests), frames)
                        self.assertTrue(sink.closed)
                        self.assertFalse(stream.failed)

    def test_cancel_expiry_close_delete_and_failed_or_partial_native_input_never_pad(self):
        for reason in ('cancel', 'closed', 'deleted', 'expired', 'native_error', 'early_eof', 'short_write'):
            with self.subTest(reason=reason):
                stream, sink, requests = feed(48000, 960, interruption=reason)
                self.assertLessEqual(len(sink.data), 960 * 8)
                self.assertTrue(sink.closed)
                self.assertEqual(requests, [960])
                if reason in ('native_error', 'early_eof', 'short_write'):
                    self.assertTrue(stream.failed)


def marker_wav(rate, frames, marker):
    pcm = array.array('f', [0.] * frames)
    pcm[marker] = .8
    payload = pcm.tobytes()
    return struct.pack('<4sI4s4sIHHIIHH4sI', b'RIFF', 36 + len(payload), b'WAVE', b'fmt ',
                       16, 3, 1, rate, rate * 4, 4, 32, b'data', len(payload)) + payload


class ArrangementPaddingNativeTests(unittest.TestCase):
    def test_real_http_suffix_float_input_zero_tail_and_independent_codec_packets(self):
        worker, ffmpeg = native_studio(self)
        rows = []
        with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True) as (service, client):
            for rate, single_frame in [(8000, False), (32000, False), (44100, False),
                                       (48000, False), (96000, False), (48000, True)]:
                total = 1 if single_frame else rate * 5 // 4
                real = 1 if single_frame else rate // 50
                start = total - real
                marker = 0 if single_frame else total - (rate * 256 + 24000) // 48000
                source = marker_wav(rate, total, marker)
                project = create_project(client)
                base = '/v2/projects/' + project['id']
                try:
                    uploaded = upload(client, project['id'], source, filename='Marker.wav')
                    status, _, snapshot = client.request('POST', base + '/arrangement',
                        {'operation': 'create', 'sample_rate': rate, 'channels': 2})
                    self.assertEqual(status, 200)
                    status, _, snapshot = client.request('POST', base + '/arrangement',
                        {'operation': 'add_uploads', 'epoch': snapshot['epoch'], 'expected_revision': 0,
                         'upload_ids': [uploaded]})
                    self.assertEqual(status, 200)
                    native = service.store.projects.projects[project['id']].arrangement.native
                    for container in ('mp3', 'mp4'):
                        status, _, descriptor = client.request('POST', base + '/arrangement',
                            {'operation': 'prepare', 'epoch': snapshot['epoch'], 'expected_revision': 1,
                             'side': 'after', 'start_frame': start, 'preview_container': container})
                        self.assertEqual(status, 200)
                        tail, padding = preview_padding(real, rate)
                        self.assertEqual(descriptor['preview_padding_frames'], padding)
                        self.assertEqual((descriptor['start_frame'], descriptor['side_duration_frames']), (start, total))
                        self.assertEqual((descriptor['preview_sample_rate'], descriptor['preview_priming_frames']), (48000, 1105))
                        captured, requested = [], []
                        write, call = ArrangementStream._write_pcm, native.call

                        def capture(stream, data):
                            write(stream, data); captured.append(bytes(data))

                        def native_call(request, **options):
                            if request['operation'] == 'block': requested.append(request['frames'])
                            return call(request, **options)

                        with patch.object(ArrangementStream, '_write_pcm', capture), patch.object(native, 'call', side_effect=native_call):
                            status, headers, encoded = client.request('GET', descriptor['stream_path'])
                        self.assertEqual(status, 200)
                        self.assertNotIn('Content-Length', headers)
                        self.assertEqual(sum(requested), real)
                        pcm = b''.join(captured)
                        self.assertEqual(len(pcm), (real + tail) * 8)
                        self.assertEqual(pcm[real * 8:], bytes(tail * 8))
                        real_pcm = array.array('f'); real_pcm.frombytes(pcm[:real * 8])
                        self.assertEqual(max(range(real), key=lambda frame: abs(real_pcm[frame * 2])), marker - start)
                        decoded = subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', 'pipe:0',
                            '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1'], input=encoded,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=15).stdout
                        samples = array.array('f'); samples.frombytes(decoded)
                        left = samples[::2]
                        peak = max(range(len(left)), key=lambda frame: abs(left[frame]))
                        expected = round((marker - start) * PREVIEW_RATE / rate) + 1105
                        self.assertLessEqual(abs(peak - expected), 2)
                        self.assertGreater(abs(left[peak]), .12)
                        self.assertTrue(all(math.isfinite(value) for value in samples))
                        self.assertLess(max(map(abs, left[-960:])), 1e-5)
                        ffprobe = ffmpeg.with_name('ffprobe' + ffmpeg.suffix)
                        packets = json.loads(subprocess.run([str(ffprobe), '-v', 'error', '-select_streams', 'a:0',
                            '-show_packets', '-show_entries', 'packet=pts,dts', '-of', 'json', '-i', 'pipe:0'],
                            input=encoded, stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, timeout=15).stdout)['packets']
                        self.assertEqual(len(packets), 6)
                        self.assertEqual(len(left), 6912)
                        rows.append({'project_rate': rate, 'container': container, 'real_input_frames': real,
                            'zero_input_frames': tail, 'preview_padding_frames': padding, 'native_frames_requested': sum(requested),
                            'encoder_zero_tail_exact': True, 'packets': len(packets), 'decoded_frames': len(left),
                            'marker_preview_frame': peak, 'decoded_peak': abs(left[peak]), 'encoded_bytes': len(encoded)})
                    stored = service.store.projects.projects[project['id']].uploads[uploaded].path.read_bytes()
                    self.assertEqual(hashlib.sha256(stored).digest(), hashlib.sha256(source).digest())
                finally:
                    self.assertEqual(client.request('DELETE', base)[0], 200)
        print(json.dumps({'padding_http_encoder_passed': True, 'rows': rows}, indent=2))


if __name__ == '__main__':
    unittest.main()
