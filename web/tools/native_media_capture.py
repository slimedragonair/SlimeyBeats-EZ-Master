"""Capture actual native media output from an owned 48 kHz Pulse null sink.

No Web Audio graph is attached to the product. Captures and individual reads
have independent byte/time bounds; failed fixture recordings remain available.
"""
import array
from contextlib import contextmanager
import json
import math
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import time

from browser_audio_environment import browser_audio_environment


@contextmanager
def native_browser_audio_environment(out):
    """Match the owned Pulse daemon to the project's 48 kHz fixture clock."""
    out = Path(out); out.mkdir(parents=True, exist_ok=True)
    config = out / 'native-pulse-48000.conf'
    with config.open('x', encoding='utf-8') as output:
        output.write('default-sample-rate = 48000\nalternate-sample-rate = 48000\n')
    previous = os.environ.get('PULSE_CONFIG')
    os.environ['PULSE_CONFIG'] = str(config.resolve())
    try:
        with browser_audio_environment(out) as evidence:
            evidence['native_daemon_config'] = {'path': str(config.resolve()),
                'default_sample_rate': 48000, 'alternate_sample_rate': 48000}
            yield evidence
    finally:
        if previous is None: os.environ.pop('PULSE_CONFIG', None)
        else: os.environ['PULSE_CONFIG'] = previous


class PulseMonitor:
    rate = 48000
    channels = 2
    bytes_per_frame = 8
    max_window_frames = rate * 30

    def __init__(self, out, *, max_bytes=256 * 1024**2, max_seconds=900):
        self.out = Path(out)
        self.path = self.out / 'native-media.f32'
        self.max_bytes, self.max_seconds = max_bytes, max_seconds
        self.process = self.output = self.errors = None
        self.last_window = None
        self.evidence = {'scope': 'actual private Pulse arrangement.monitor PCM; no physical device claim',
                         'rate': self.rate, 'channels': self.channels, 'max_bytes': max_bytes,
                         'max_seconds': max_seconds, 'path': str(self.path)}

    def start(self):
        server = os.environ.get('PULSE_SERVER', '')
        socket = Path(server[5:]) if server.startswith('unix:') else Path('.')
        runtime = str(socket.parent)
        if (not server.startswith('unix:') or socket.name != 'native' or
                not socket.parent.name.startswith('arrangement-pulse-') or
                os.environ.get('PULSE_RUNTIME_PATH') != runtime or
                os.environ.get('PULSE_STATE_PATH') != runtime or
                os.environ.get('PULSE_SINK') != 'arrangement'):
            raise RuntimeError('Run inside browser_audio_environment; refusing a desktop/WSLg sink')
        executable = shutil.which('parec')
        if not executable: raise RuntimeError('Native media acceptance requires the installed PulseAudio utilities')
        self.out.mkdir(parents=True, exist_ok=True)
        self.output = self.path.open('xb')
        self.errors = (self.out / 'native-media.log').open('xb')
        self.name = f'arrangement-qa-{os.getpid()}-{secrets.token_hex(4)}'
        self.started = time.monotonic()
        self.process = subprocess.Popen([executable, '--device=arrangement.monitor', '--client-name=' + self.name,
            '--stream-name=' + self.name, '--format=float32le', '--rate=48000', '--channels=2',
            '--latency-msec=10', '--raw'], stdout=self.output, stderr=self.errors, stdin=subprocess.DEVNULL)
        self.evidence['pid'] = self.process.pid
        deadline = time.monotonic() + 5
        while True:
            self.check()
            value = subprocess.run([shutil.which('pactl') or 'pactl', '-f', 'json', 'list', 'source-outputs'],
                                   capture_output=True, check=True, timeout=5)
            if any(item.get('properties', {}).get('media.name') == self.name for item in json.loads(value.stdout)):
                break
            if time.monotonic() >= deadline: raise RuntimeError('Owned native PCM monitor did not attach')
            time.sleep(.02)
        return self

    def check(self):
        if self.process.poll() is not None: raise RuntimeError('Owned native PCM monitor exited; inspect native-media.log')
        if self.path.stat().st_size > self.max_bytes or time.monotonic() - self.started > self.max_seconds:
            self.close()
            raise RuntimeError('Native PCM fixture exceeded its capture budget')

    def frames(self):
        return self.path.stat().st_size // self.bytes_per_frame

    def wait_frames(self, end, timeout=15):
        deadline = time.monotonic() + timeout
        while self.frames() < end:
            self.check()
            if time.monotonic() >= deadline: raise RuntimeError('Actual sink PCM did not reach the requested window')
            time.sleep(.01)

    def read_window(self, first, frames, timeout=15):
        if first < 0 or frames <= 0 or frames > self.max_window_frames:
            raise ValueError('Require a nonnegative start and a bounded positive PCM window')
        self.wait_frames(first + frames, timeout)
        with self.path.open('rb') as source:
            source.seek(first * self.bytes_per_frame)
            data = source.read(frames * self.bytes_per_frame)
        if len(data) != frames * self.bytes_per_frame: raise RuntimeError('Native PCM window was incomplete')
        values = array.array('f'); values.frombytes(data)
        if sys.byteorder != 'little': values.byteswap()
        if not all(map(math.isfinite, values)): raise RuntimeError('Native PCM contains nonfinite samples')
        self.last_window = values
        return values

    def measure_tones(self, *, frames=18048, skip_frames=3840):
        first = self.frames() + skip_frames
        values = self.read_window(first, frames)
        samples = values[0::2][-9600:]
        if len(samples) != 9600: raise RuntimeError('Insufficient actual output PCM')
        def tone(hz):
            real = imag = 0.
            for index, sample in enumerate(samples):
                phase = index * hz * 2 * math.pi / self.rate
                real += sample * math.cos(phase); imag += sample * math.sin(phase)
            return math.hypot(real, imag) * 2 / len(samples)
        return {'peak': max(map(abs, samples)), 'first': tone(440), 'second': tone(880), 'rate': self.rate,
                'capture_source': 'private Pulse monitor', 'captured_frames': frames,
                'capture_start_frame': first, 'capture_end_frame': first + frames,
                'measured_start_frame': first + frames - 9600, 'measured_end_frame': first + frames,
                'clock': (first + frames) / self.rate, 'clock_domain': 'capture frames, not project time'}

    def close(self):
        if self.process is not None:
            if self.process.poll() is None:
                self.process.terminate()
                try: self.process.wait(timeout=5)
                except subprocess.TimeoutExpired: self.process.kill(); self.process.wait(timeout=5)
            self.evidence['reaped'] = self.process.poll() is not None
        if self.output: self.output.close()
        if self.errors: self.errors.close()


@contextmanager
def native_media_capture(out, **limits):
    monitor = PulseMonitor(out, **limits)
    try: yield monitor.start()
    finally: monitor.close()


pulse_monitor_capture = native_media_capture
