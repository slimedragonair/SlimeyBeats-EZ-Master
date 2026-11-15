"""Check real browser ZIP builders, Rust settings, and independent Python extraction.

No real recordings are used. MP3 packaging tests treat caller-encoded bytes as
opaque; MP3 decoding/quality and application export controls have separate tests.
"""
import argparse
import base64
import functools
import hashlib
import http.server
import io
import json
from pathlib import Path
import struct
import threading
import zipfile

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]


class Handler(http.server.SimpleHTTPRequestHandler):
    wasm_site = None

    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        asset = path.split('?', 1)[0]
        if asset in ['/engine/dsp.js', '/engine/dsp_bg.wasm']:
            return str(self.wasm_site / 'pkg' / asset.rsplit('/', 1)[1])
        return super().translate_path(path)


def read_archive(encoded):
    raw = base64.b64decode(encoded, validate=True)
    archive = zipfile.ZipFile(io.BytesIO(raw))
    assert archive.testzip() is None
    assert len(archive.namelist()) == len(set(n.lower() for n in archive.namelist()))
    for entry in archive.infolist():
        assert '/' not in entry.filename and '\\' not in entry.filename
        assert entry.compress_type == zipfile.ZIP_STORED
        assert not entry.is_dir()
    return raw, archive


def verify_artifacts(artifacts):
    archives = {key: read_archive(value) for key, value in artifacts.items() if key not in ['eq', 'settings']}
    master_raw, master = archives['master']
    assert set(master.namelist()) == {'master.wav', 'session.json', 'analysis.json', 'settings.json', 'eq-profile.json', 'manifest.json'}
    session = json.loads(master.read('session.json'))
    assert session['schema'] == 'slimeybeats-browser-session' and session['version'] == 1
    assert session['source']['name'] == 'Song.wav' and session['reference']['name'] == 'Reference.wav'
    assert not session['original_audio_included'] and not session['reference_audio_included']
    assert 'restore' in session['restore_support'].lower() or 'restoration' in session['restore_support'].lower()
    analysis = json.loads(master.read('analysis.json'))
    assert analysis['producer'] == {'schema_version': 7, 'version': '0.3.0'}
    assert analysis['measurements']['output']['metrics']['integrated_lufs'] == -14.2
    assert analysis['measurements']['output']['spectrum']['cutoff_hz'] is None
    assert 'noise_candidate' not in analysis['measurements']['output']['spectrum']
    assert analysis['suggestions']['output']['noise_candidate'] is True
    assert 'issues' not in analysis['measurements']['output']
    assert analysis['suggestions']['output']['issues'][0]['severity'] == 30
    assert 'not measurements' in analysis['suggestion_scope']
    assert 'remeasure' in analysis['measurement_scope']
    assert archives['original_ignored'][0] == master_raw
    original = archives['original'][1]
    assert 'original.wav' in original.namelist()
    assert json.loads(original.read('session.json'))['original_audio_included']
    assert archives['mp3'][1].read('master.mp3') == bytes([73, 68, 51, 4, 0, 0, 0, 0, 0, 0])
    eq = json.loads(master.read('eq-profile.json'))
    assert eq == json.loads(base64.b64decode(artifacts['eq']))
    assert eq['version'] == 2 and eq['bands'][2]['gain'] == -3
    assert json.loads(master.read('settings.json')) == json.loads(base64.b64decode(artifacts['settings']))

    studio = archives['studio'][1]
    assert set(studio.namelist()) == {'mix.wav', 'vocals.wav', 'part_64.wav', 'analysis.json', 'settings.json', 'session.json', 'stems.json', 'manifest.json'}
    assert studio.read('mix.wav')[34:36] == struct.pack('<H', 24)
    mapping = json.loads(studio.read('stems.json'))
    assert mapping['version'] == 9 and mapping['format'] == 'wav32f'
    assert mapping['stems'][1]['display_name'] == 'Backing take'
    state = json.loads(studio.read('session.json'))
    assert state['stem_layout'] == {'frames': 32, 'sample_rate': 48000, 'channels': 2}
    assert not state['info']['preview_is_mix'] and state['info']['controls']['tracks'][0]['gain_db'] == -6
    controls = json.loads(studio.read('settings.json'))
    assert controls['eq_status'] == 'not_applied' and controls['preview_is_mix'] is False
    assert 'eq-profile.json' not in studio.namelist()
    for kind in ['studio_empty', 'studio_partial']:
        minimal = archives[kind][1]
        assert 'stems.json' not in minimal.namelist()
        assert json.loads(minimal.read('analysis.json'))['status'] == 'not_recorded'

    for key, (raw, archive) in archives.items():
        if key == 'raw_stems':
            continue
        assert b'private-user' not in raw and b'C:\\Users' not in raw and b'/home/' not in raw
        if key == 'general':
            continue
        inventory = json.loads(archive.read('manifest.json'))
        assert inventory['schema'] == 'slimeybeats-session-bundle' and inventory['version'] == 1
        assert {entry['name'] for entry in inventory['entries']} == set(archive.namelist()) - {'manifest.json'}
        for entry in inventory['entries']:
            assert entry['bytes'] == len(archive.read(entry['name']))
            assert entry['role']
    general = archives['general'][1]
    assert general.read('vector.bin') == b'123456789' and general.getinfo('vector.bin').CRC == 0xcbf43926
    assert general.read('empty.json') == b''
    assert general.read('slice.bin') == bytes([1, 2])
    assert general.getinfo('vector.bin').date_time == (1980, 1, 1, 0, 0, 0)
    corrupt = bytearray(archives['general'][0])
    corrupt[30 + len('vector.bin')] ^= 1
    assert zipfile.ZipFile(io.BytesIO(corrupt)).testzip() == 'vector.bin'
    assert hashlib.sha256(archives['raw_stems'][0]).hexdigest() == 'd498e5cd7bf98647ac2c84869110f6e404f136e83b136df2885e6ee9c15e8c8c'
    return len(archives)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--wasm-site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--executable')
    args = parser.parse_args()
    Handler.wasm_site = args.wasm_site.resolve()
    for name in ['dsp.js', 'dsp_bg.wasm']:
        if not (Handler.wasm_site / 'pkg' / name).is_file():
            parser.error('Build the browser site first, or supply --wasm-site with a built site')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(ROOT / 'web')))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(headless=True, **({'executable_path': args.executable} if args.executable else {}))
            try:
                page = browser.new_page()
                errors, requests = [], []
                page.on('pageerror', lambda error: errors.append(str(error)))
                page.on('request', lambda request: requests.append(request))
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/', wait_until='networkidle')
                result = page.evaluate('''async () => {
                    const engine = await import('/engine/dsp.js');
                    await engine.default();
                    const defaults = JSON.parse(engine.defaults()).settings;
                    const studio = new engine.BrowserStudio();
                    const samples = Float32Array.from({length: 48001}, (_, i) => 0.01 * Math.sin(i * 440 * Math.PI * 2 / 48000));
                    let realExports;
                    try {
                        studio.load(samples, 48000, 1);
                        realExports = ['wav16', 'wav24', 'wav32f'].map(format => studio.export(format));
                    } finally { studio.free(); }
                    let realMaster;
                    const master = new engine.Session(samples, 48000, 1);
                    try {
                        master.master(JSON.stringify(defaults), () => {});
                        const audio = master.export_wav(JSON.stringify(defaults), 'wav24');
                        realMaster = {audio, report: JSON.parse(master.report())};
                    }
                    finally { master.free(); }
                    const {verifySessionExports} = await import('/tools/session_export_checks.js');
                    return verifySessionExports(defaults, settings => {
                        const response = engine.response(JSON.stringify(settings), 48000);
                        if (!(response instanceof Float32Array) || response.length !== 256 ||
                            !response.every(Number.isFinite)) throw Error('Rust Settings validation failed');
                    }, realExports, realMaster);
                }''')
                assert not errors, errors
                assert all(request.method == 'GET' and not request.post_data_buffer for request in requests)
                count = verify_artifacts(result['artifacts'])
                print(json.dumps({'passed': True, 'browser': args.browser + ' ' + browser.version,
                    'independently_extracted_archives': count, 'checks': result['checks'], 'page_errors': errors}, indent=2))
            finally:
                browser.close()
    finally:
        server.shutdown()


if __name__ == '__main__':
    main()
