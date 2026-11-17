"""Real timestamped MP4 decoder: short EOF, source clock, bounds and cancellation."""
import argparse
import io
import json
from pathlib import Path
import shutil
import struct
import subprocess
import sys
from urllib.parse import urlparse
import wave

from playwright.sync_api import sync_playwright
from browser_audio_environment import browser_audio_environment
from browser_capture import BrowserDiagnostics
from remote_browser_fixture import RemoteBrowserFixture, ROOT

sys.path.insert(0, str(ROOT))
from hybrid.tests.support import Client
from hybrid.tests.project_support import create_project, upload


def marked_source():
    values = bytearray(20 * 48000 * 2)
    for frame in [2400, 24000, 48000]: struct.pack_into('<h', values, frame * 2, 26000)
    stream = io.BytesIO()
    with wave.open(stream, 'wb') as output:
        output.setparams((1, 2, 48000, 0, 'NONE', 'not compressed')); output.writeframes(values)
    return stream.getvalue()


def verify(owned, args, result):
    client = Client(urlparse(owned.origin).port, origin=owned.origin)
    status, _, session = client.request('POST', '/v1/session', {}, auth=False); assert status == 201, (status, session)
    client.token = session['token']; project = create_project(client); path = f'/v2/projects/{project["id"]}/arrangement'
    def command(operation, **data):
        status, _, value = client.request('POST', path, {'operation': operation, **data})
        assert status == 200, (status, value); return value
    state = command('create', sample_rate=48000, channels=2)
    source = upload(client, project['id'], marked_source())
    state = command('add_uploads', epoch=state['epoch'], expected_revision=state['revision'], upload_ids=[source], names=['Clock markers'])
    clip = state['arrangement']['lanes'][0]['clips'][0]['id']
    shutil.copy2(Path(__file__).with_name('studio_capture_processor.js'), owned.site / '__arrangement_capture.js')
    with browser_audio_environment(args.out) as audio, sync_playwright() as p:
        result['audio_environment'] = audio
        browser = p.webkit.launch(headless=True); result['browser'] = 'webkit ' + browser.version
        try:
            page = browser.new_page(); result['errors'] = []; BrowserDiagnostics(page, 'webkit', result)
            page.add_init_script(path=str(Path(__file__).with_name('arrangement_stream_browser_probe.js')))
            page.goto(owned.origin, wait_until='networkidle')
            for mode, end, start, markers in [('short', 5760, 0, [.05]), ('markers', 60000, 0, [.05, .5, 1]),
                                               ('seek', 60000, 36000, [.25]), ('bounds', 960000, 0, []), ('cancel', 960000, 0, [])]:
                if args.mode and mode not in args.mode: continue
                state = command('edit', epoch=state['epoch'], expected_revision=state['revision'],
                    edits=[{'kind': 'trim_clip', 'clip_id': clip, 'start_frame': 0, 'source_start_frame': 0, 'end_frame': end}])
                descriptor = command('prepare', epoch=state['epoch'], expected_revision=state['revision'],
                                     side='after', start_frame=start, preview_container='mp4')
                assert descriptor['preview_priming_frames'] == args.expected_priming and descriptor['start_frame'] == start
                page.evaluate('value=>{window.streamProbeResult=null;void checkArrangementStream(value).then(result=>window.streamProbeResult={result},error=>window.streamProbeResult={error:String(error)});}', {
                    'path': descriptor['stream_path'], 'token': client.token, 'mode': mode,
                    'duration': (end - start) / 48000 + args.expected_priming / 48000, 'priming': args.expected_priming,
                    'mime': descriptor['preview_mime'], 'expectedMarkers': markers})
                if mode != 'cancel': page.get_by_role('button', name='Play decoder probe', exact=True).click(timeout=15000)
                page.wait_for_function('()=>window.streamProbeResult!==null', timeout=30000)
                outcome = page.evaluate('()=>window.streamProbeResult'); assert 'result' in outcome, outcome
                measured = outcome['result']
                result['cases'].append({'mode': mode, 'measurement': measured})
                if measured.get('encodedChunks'):
                    encoded = args.out / f'{mode}-preview.mp4'
                    encoded.write_bytes(bytes(value for chunk in measured.pop('encodedChunks') for value in chunk))
                    decoded = subprocess.run([str(args.ffmpeg), '-v', 'error', '-i', str(encoded), '-f', 'f32le', '-'], capture_output=True, check=True)
                    values = __import__('array').array('f'); values.frombytes(decoded.stdout)
                    measured['independent_encoded_peak'] = max(map(abs, values))
                summary = {key: value for key, value in measured.items() if key != 'raw'}
                assert measured['aborted'] and not measured['errors'], measured
                if markers:
                    assert measured['ended'] and len(measured['peaks']) == len(markers), summary
                    # Media playing events use the main thread clock; marker spacing is measured in captured PCM frames.
                    for actual, wanted in zip(measured['peaks'], markers):
                        assert abs(actual['fromPlaying'] - wanted - args.expected_priming / 48000) < .05, summary
                    for index in range(1, len(markers)):
                        delta = measured['peaks'][index]['sample'] - measured['peaks'][index - 1]['sample']
                        assert abs(delta - (markers[index] - markers[index - 1]) * 48000) <= 2, measured
                if mode == 'bounds':
                    assert measured['maxAhead'] < 1.6 and measured['buffered'][0] > 1, measured
                    assert measured['buffered'][1] - measured['buffered'][0] < 4, measured
                if mode == 'cancel':
                    assert measured['pausedReadsAfter'] == measured['pausedReads'] and measured['bufferedEnd'] < 1.6, measured
                command('stop', epoch=state['epoch'], expected_revision=state['revision'], stream_id=descriptor['stream_id'])
                measured.pop('raw', None)
            assert not result['errors'], result['errors']
        finally: browser.close()
    assert client.request('DELETE', f'/v2/projects/{project["id"]}')[0] == 200
    result['passed'] = True


def main():
    parser = argparse.ArgumentParser()
    for name in ['site', 'out', 'cli', 'worker', 'ffmpeg']: parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--mode', choices=['short', 'markers', 'seek', 'bounds', 'cancel'], action='append', help='Focused diagnostic; omit to qualify all modes')
    parser.add_argument('--server-source', type=Path, help='Private frozen backend source root for isolated codec diagnostics')
    parser.add_argument('--expected-priming', type=int, default=1105)
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True); args.arrangement = True
    result = {'scope': 'Shipped MP4 reader + actual HTTP/native codec + WebKit decoder; PCM markers, no physical device claim', 'cases': []}
    try:
        with RemoteBrowserFixture(args) as owned:
            result['fixture'] = owned.provenance; result['requests'] = owned.requests; verify(owned, args, result)
    finally: (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__': main()
