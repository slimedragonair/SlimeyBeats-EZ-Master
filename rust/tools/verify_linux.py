"""Linux native/export/XDG/GUI smoke checks using only Python's standard library."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from concurrent.futures import ThreadPoolExecutor
from threading import Event
import wave
from verify_exports import verify
from linux_test_display import park_pointer_until


def command(arguments, park_pointer=False, **options):
    if park_pointer:
        stop = Event()
        with ThreadPoolExecutor(max_workers=1) as executor:
            future = executor.submit(park_pointer_until, stop)
            try:
                result = subprocess.run([str(item) for item in arguments], capture_output=True, text=True,
                                        encoding='utf-8', errors='replace', timeout=120, **options)
            finally:
                stop.set()
                future.result()
    else:
        result = subprocess.run([str(item) for item in arguments], capture_output=True, text=True,
                                encoding='utf-8', errors='replace', timeout=120, **options)
    if result.returncode:
        raise RuntimeError(f'Exit {result.returncode}: {arguments}\n{result.stdout}\n{result.stderr}')
    return result


def smoke(binary, source, folder, gui):
    original = hashlib.sha256(source.read_bytes()).hexdigest()
    # Native WAV must remain usable without an external codec on PATH.
    environment = dict(os.environ, PATH='')
    command([binary, source, '--format', 'wav24', '--out', folder/'native', '--threads', '2'], env=environment)
    native = next((folder/'native').glob('*.wav'))
    with wave.open(str(source)) as before, wave.open(str(native)) as after:
        assert (before.getnframes(), before.getframerate(), before.getnchannels()) == (
            after.getnframes(), after.getframerate(), after.getnchannels())
    # Actually change rate before the common six-format/resampling/error-path harness.
    converted = folder/'demo-48000.wav'
    command(['ffmpeg', '-v', 'error', '-nostdin', '-i', source, '-ar', '48000', '-c:a', 'pcm_s24le', converted])
    exports = folder/'formats'; exports.mkdir()
    record = {'native_wav_without_ffmpeg': True, 'native_layout_preserved': True,
              'exports': verify(binary, converted, exports), 'resampling_input_hz': 48000}
    if gui:
        root = folder/'runtime with spaces'; root.mkdir()
        installed = root/gui.name; shutil.copy2(gui, installed)
        config, data = folder/'config', folder/'data'
        alsa = folder/'alsa-null.conf'
        alsa.write_text('pcm.!default { type null }\n')
        environment = dict(os.environ, XDG_CONFIG_HOME=str(config), XDG_DATA_HOME=str(data),
                           RAYON_NUM_THREADS='2', LIBGL_ALWAYS_SOFTWARE='1', ALSA_CONFIG_PATH=str(alsa),
                           WINIT_X11_SCALE_FACTOR='1', XDG_SESSION_TYPE='x11')
        environment.pop('WAYLAND_DISPLAY', None)
        environment.pop('WAYLAND_SOCKET', None)
        captures = []
        for name, args in (('welcome', []), ('manual-eq', [source, '--auto-master', '--view', 'eq']),
                           ('compact-eq', [source, '--auto-master', '--view', 'eq', '--capture-size', '1100', '720']),
                           ('compact-controls', [source, '--auto-master', '--view', 'eq', '--capture-size', '1100', '720', '--capture-workspace-scroll', '200'])):
            screenshot = folder/f'{name}.png'
            command([installed, *args, '--capture-ui', screenshot, '--capture-save-settings'], park_pointer=True, env=environment, cwd=root)
            payload = screenshot.read_bytes()
            assert payload[:8] == b'\x89PNG\r\n\x1a\n'
            width, height = struct.unpack('>II', payload[16:24])
            assert (width, height) == ((1100, 720) if name.startswith('compact') else (1480, 940)), (name, width, height)
            state = json.loads(screenshot.with_suffix('.state.json').read_text())
            assert state['error'] is None, state['error']
            if name != 'welcome': assert state['preview_ready'] and state['playback_ready']
            captures.append({'file':str(screenshot), 'width':width, 'height':height, 'state':state})
        settings = config/'slimeybeats-ez-master/rust-settings.json'
        assert settings.is_file(), 'GUI settings did not use XDG_CONFIG_HOME'
        configured = json.loads(settings.read_text())
        assert Path(configured['out_dir']) == data/'slimeybeats-ez-master/output'
        assert not (root/'user_data').exists(), 'Installed GUI wrote beside its binary'
        assert not list(data.rglob('*.wav')), 'Master preview unexpectedly exported audio'
        record.update(gui_captures=captures, xdg_settings_and_data_paths=True,
                      gui_master_preview_did_not_export=True,
                      gui_audio_backend='ALSA null sink (synthetic)',
                      hardware_playback_tested=False, portal_file_dialog_tested=False)
    assert hashlib.sha256(source.read_bytes()).hexdigest() == original
    record['source_preserved'] = True
    return record


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    project = Path(__file__).resolve().parents[1]
    parser.add_argument('--binary', type=Path, default=project/'target/release/slimeybeats-ez-master-cli')
    parser.add_argument('--input', type=Path, default=project.parent/'examples/audio/demo-input.wav')
    parser.add_argument('--gui', type=Path)
    parser.add_argument('--out', type=Path, required=True, help='New dedicated evidence directory')
    args = parser.parse_args()
    if args.out.exists(): raise FileExistsError(args.out)
    args.out.mkdir(parents=True)
    record = smoke(args.binary.resolve(), args.input.resolve(), args.out.resolve(), args.gui.resolve() if args.gui else None)
    (args.out/'verification.json').write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps(record, indent=2))
