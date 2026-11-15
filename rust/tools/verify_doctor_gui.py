"""Capture real Doctor pixels from synthetic audio without desktop input automation.

The native application loads, scans, settles and closes itself. Each run uses
private preferences, component caches and recovery. No repair or export is run.
This qualifies renderer evidence, not audible playback or pointer interaction.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import subprocess
import wave


def fixture(path):
    """Stereo tone / quiet gap / resumed tone / quiet tail, plus one plateau."""
    rate = 16000
    samples = bytearray()
    shoulders = [0.75, 0.9, 1.0, 1.0, 1.0, 0.9, 0.75]
    for frame in range(rate * 8):
        seconds = frame / rate
        active = seconds < 2 or 4 <= seconds < 6
        left = 0.12 * math.sin(2 * math.pi * 1000 * seconds) if active else 0
        right = 0.10 * math.sin(2 * math.pi * 1700 * seconds) if active else 0
        if 3200 <= frame < 3207:
            left = shoulders[frame - 3200]
        samples.extend(struct.pack('<hh', round(left * 32767), round(right * 32767)))
    with path.open('xb') as stream:
        with wave.open(stream, 'wb') as audio:
            audio.setparams((2, 2, rate, rate * 8, 'NONE', 'not compressed'))
            audio.writeframes(samples)


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def png_size(path):
    header = path.read_bytes()[:24]
    if header[:8] != b'\x89PNG\r\n\x1a\n' or header[12:16] != b'IHDR':
        raise AssertionError(f'Not a PNG screenshot: {path}')
    return list(struct.unpack('>II', header[16:24]))


def write_new(path, text):
    with path.open('x', encoding='utf-8') as stream:
        stream.write(text)


def verify(args):
    gui, out = args.gui.resolve(), args.out.resolve()
    if not gui.is_file():
        raise FileNotFoundError(gui)
    out.mkdir(parents=True, exist_ok=False)
    source = out / 'Synthetic Doctor fixture.wav'
    fixture(source)
    original_hash = sha256(source)
    result = {
        'scope': 'Actual native renderer, synthetic WAV load and background issue scan. No desktop input automation, repair, final export or hardware-listening qualification.',
        'fixture_sha256': original_hash,
        'cases': [],
    }
    for name, size in [('doctor-wide', [1480, 940]), ('doctor-compact', [1100, 720])]:
        capture = out / f'{name}.png'
        private = out / f'{name}-private'
        (private / 'preferences').mkdir(parents=True)
        env = os.environ.copy()
        for key, folder in [('LOCALAPPDATA', 'local'), ('APPDATA', 'roaming'),
                            ('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                            ('XDG_CACHE_HOME', 'cache')]:
            directory = private / folder
            directory.mkdir()
            env[key] = str(directory)
        command = [str(gui), str(source), '--capture-doctor', '--capture-ui', str(capture),
                   '--recovery-data', str(private), '--capture-size', *map(str, size)]
        completed = subprocess.run(command, cwd=out, env=env, capture_output=True, text=True,
                                   timeout=180, creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        write_new(out / f'{name}.stdout.log', completed.stdout)
        write_new(out / f'{name}.stderr.log', completed.stderr)
        if completed.returncode:
            raise AssertionError(f'{name}: native capture failed ({completed.returncode}); see {out / (name + ".stderr.log")}')
        state = json.loads(capture.with_suffix('.state.json').read_text(encoding='utf-8'))
        assert state['error'] is None and state['preview_ready'] and state['playback_ready'], state
        assert state['doctor_scan_ready'] is True and state['doctor_issue_count'] >= 3, state
        assert sha256(source) == original_hash, 'Capture changed the synthetic input'
        assert not any((private / 'output').glob('**/*')), 'Capture unexpectedly exported audio'
        case = {'view': name, 'viewport_points': size, 'image_pixels': png_size(capture),
                'capture': capture.name, 'capture_sha256': sha256(capture), 'state': state}
        result['cases'].append(case)
        print(json.dumps(case), flush=True)
    result['passed'] = True
    write_new(out / 'results.json', json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gui', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True, help='A new output directory; existing captures are preserved.')
    verify(parser.parse_args())
