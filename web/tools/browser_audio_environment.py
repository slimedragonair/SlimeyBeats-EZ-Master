"""Owned Linux null audio clock; never reuse a desktop/WSLg output device."""
from contextlib import contextmanager
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time


@contextmanager
def browser_audio_environment(out):
    if not sys.platform.startswith('linux'):
        yield {'scope': 'headless browser output capture; physical device not qualified'}
        return
    executable = shutil.which('pulseaudio')
    if not executable: raise RuntimeError('Linux audio qualification requires the existing PulseAudio null-sink fixture dependency')
    saved = {key: os.environ.get(key) for key in ['PULSE_SERVER', 'PULSE_SINK', 'PULSE_RUNTIME_PATH', 'PULSE_STATE_PATH', 'ALSA_CONFIG_PATH']}
    with tempfile.TemporaryDirectory(prefix='arrangement-pulse-') as directory:
        socket = Path(directory) / 'native'
        environment = dict(os.environ, PULSE_RUNTIME_PATH=directory, PULSE_STATE_PATH=directory,
                           DBUS_SESSION_BUS_ADDRESS='unix:path=' + directory + '/no-session-bus')
        process = subprocess.Popen([executable, '-n', '--daemonize=no', '--exit-idle-time=-1', '--use-pid-file=no',
            '--log-target=file:' + str(Path(out).resolve() / 'pulse.log'),
            '--load=module-native-protocol-unix socket=' + str(socket) + ' auth-anonymous=1',
            '--load=module-null-sink sink_name=arrangement rate=48000 channels=2'],
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=environment)
        evidence = {'scope': 'private PulseAudio 48 kHz stereo null sink', 'pid': process.pid}
        try:
            deadline = time.monotonic() + 10
            while not socket.exists():
                if process.poll() is not None or time.monotonic() >= deadline: raise RuntimeError('Private audio fixture could not start; inspect pulse.log')
                time.sleep(.01)
            os.environ.update(PULSE_SERVER='unix:' + str(socket), PULSE_SINK='arrangement')
            os.environ.update(PULSE_RUNTIME_PATH=directory, PULSE_STATE_PATH=directory)
            checked = subprocess.run([shutil.which('pactl') or 'pactl', 'info'], env=dict(os.environ),
                                     capture_output=True, timeout=5, check=True)
            if process.poll() is not None or 'arrangement' not in checked.stdout.decode('utf-8', errors='replace'):
                raise RuntimeError('Private audio fixture did not expose its owned null sink')
            alsa = Path('/tmp/slimeybeats-linux-20261112-493c/alsa-null.conf')
            if alsa.is_file(): os.environ['ALSA_CONFIG_PATH'] = str(alsa)
            yield evidence
        finally:
            for key, value in saved.items():
                if value is None: os.environ.pop(key, None)
                else: os.environ[key] = value
            if process.poll() is None:
                process.terminate()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=5)
            evidence['reaped'] = process.poll() is not None
