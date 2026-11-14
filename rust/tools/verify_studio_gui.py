"""Capture real native Studio jobs and record prepared-preview/playback state.

Opt-in rendering checks; this does not claim pointer interaction or audibility.
Use an installed test model cache and a trusted compatible plugin fixture.
"""
import argparse
import json
from pathlib import Path
import subprocess


def verify(args):
    args.out.mkdir(parents=True, exist_ok=True)
    cases = [
        ('four-stems', 'stems', 'separate', [1480, 940], ['--studio-four-stems', '--studio-data', str(args.engine.resolve())]),
        ('voice-compact', 'voice', 'voice', [1100, 720], []),
        ('repair-compact', 'repair', 'spectrogram', [1100, 720], []),
        ('plugins', 'plugins', 'plugin', [1480, 940], ['--studio-plugin', str(args.plugin.resolve())]),
    ]
    result = {'scope': 'Actual native renderer and real load/process/prepare jobs. Captures do not prove audible hardware output or all pointer interactions.', 'cases': []}
    for name, page, action, size, extra in cases:
        capture = args.out.resolve() / (name + '.png')
        if capture.exists() or capture.with_suffix('.state.json').exists():
            raise FileExistsError(f'Choose a fresh capture directory: {capture}')
        completed = subprocess.run([str(args.gui.resolve()), str(args.input.resolve()), '--view', page, '--studio-action', action,
                                    '--capture-ui', str(capture), '--capture-size', *map(str, size), *extra],
                                   capture_output=True, text=True, timeout=180)
        assert completed.returncode == 0, (name, completed.stderr)
        state = json.loads(capture.with_suffix('.state.json').read_text())
        assert state['error'] is None and state['preview_ready'] and state['playback_ready'], (name, state)
        result['cases'].append({'view': name, 'size': size, 'state': state, 'capture': capture.name})
        print(json.dumps(result['cases'][-1]), flush=True)
    result['passed'] = True
    (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gui', type=Path, required=True)
    parser.add_argument('--input', type=Path, required=True)
    parser.add_argument('--engine', type=Path, required=True)
    parser.add_argument('--plugin', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    verify(parser.parse_args())
