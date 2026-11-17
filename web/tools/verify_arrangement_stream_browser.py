"""Native MP3/MP4 decoder: actual sink PCM, source clock, bounds and cancellation."""
import argparse
from array import array
import io
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
from urllib.parse import urlparse
import wave

from playwright.sync_api import sync_playwright
from browser_capture import BrowserDiagnostics
from native_media_capture import native_browser_audio_environment, native_media_capture
from remote_browser_fixture import RemoteBrowserFixture, ROOT

sys.path.insert(0, str(ROOT))
from hybrid.tests.support import Client
from hybrid.tests.project_support import create_project, upload

RATE = 48000
MIN_PREVIEW_FRAMES = RATE * 3 // 25
SOURCE_MARKERS = (256, 2400, 5504, 24000, 48000, 59744)
CASES = (('short', 5760, 0), ('markers', 60000, 0), ('seek', 60000, 36000),
         ('near_eof', 60000, 59040), ('one_frame', 59745, 59744),
         ('bounds', 960000, 0), ('cancel', 960000, 0))


def marked_source():
    values = bytearray(20 * RATE * 2)
    for frame in SOURCE_MARKERS:
        struct.pack_into('<h', values, frame * 2, 26000)
    stream = io.BytesIO()
    with wave.open(stream, 'wb') as output:
        output.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        output.writeframes(values)
    return stream.getvalue()


def pcm_markers(values, channels=2):
    """Group every above-threshold sample; choose the strongest point per group."""
    assert len(values) % channels == 0
    assert all(math.isfinite(value) for value in values), 'Nonfinite captured PCM'
    groups = []
    for index in range(0, len(values), channels):
        amplitude = abs(values[index])
        if amplitude <= .12:
            continue
        frame = index // channels
        if not groups or frame - groups[-1]['end'] > 128:
            groups.append({'sample': frame, 'end': frame, 'amplitude': amplitude})
        else:
            groups[-1]['end'] = frame
            if amplitude > groups[-1]['amplitude']:
                groups[-1].update(sample=frame, amplitude=amplitude)
    return groups


def qualify_pcm(measured, raw, first, playing, markers, expected_priming):
    peaks = pcm_markers(raw)
    measured.update(captureFrames=len(raw) // 2, capturePeak=max(map(abs, raw), default=0), peaks=peaks)
    assert measured['ended'] and len(peaks) == len(markers), measured
    for actual, wanted in zip(peaks, markers):
        actual['fromPlaying'] = (first + actual['sample'] - playing) / RATE
        assert abs(actual['fromPlaying'] - wanted - expected_priming / RATE) < .05, measured
    for index in range(1, len(markers)):
        delta = peaks[index]['sample'] - peaks[index - 1]['sample']
        assert abs(delta - (markers[index] - markers[index - 1]) * RATE) <= 2, measured
    return peaks


def verify(owned, args, result):
    client = Client(urlparse(owned.origin).port, origin=owned.origin)
    status, _, session = client.request('POST', '/v1/session', {}, auth=False)
    assert status == 201, (status, session)
    client.token = session['token']
    project = create_project(client)
    path = f'/v2/projects/{project["id"]}/arrangement'

    def command(operation, **data):
        status, _, value = client.request('POST', path, {'operation': operation, **data})
        assert status == 200, (status, value)
        return value

    try:
        state = command('create', sample_rate=RATE, channels=2)
        source = upload(client, project['id'], marked_source())
        state = command('add_uploads', epoch=state['epoch'], expected_revision=state['revision'],
                        upload_ids=[source], names=['Head, original clock and tail markers'])
        clip = state['arrangement']['lanes'][0]['clips'][0]['id']
        # A minimal private page prevents unrelated application audio from reaching the sink.
        (owned.site / '__arrangement_stream_probe.html').write_text('<!doctype html><title>Native decoder qualification</title>')
        with native_browser_audio_environment(args.out) as audio, native_media_capture(args.out) as monitor, sync_playwright() as p:
            result['audio_environment'] = audio
            result['native_capture'] = monitor.evidence
            launch_options = {'ignore_default_args': ['--mute-audio']} if args.browser == 'chromium' else {}
            browser = getattr(p, args.browser).launch(headless=True, **launch_options)
            result['native_output_launch'] = launch_options
            result['browser'] = args.browser + ' ' + browser.version
            try:
                page = browser.new_page()
                result['errors'] = []
                BrowserDiagnostics(page, args.browser, result)
                clock_events = []

                def clock(source, value):
                    entry = {**value, 'capture_frame': monitor.frames()}
                    if value['event'] == 'playing':
                        checked = subprocess.run(['pactl', '-f', 'json', 'list', 'sink-inputs'],
                                                 capture_output=True, check=True, timeout=5)
                        entry['sink_inputs'] = json.loads(checked.stdout)
                    clock_events.append(entry)

                page.expose_binding('arrangementStreamClock', clock)
                page.add_init_script(path=str(Path(__file__).with_name('arrangement_stream_browser_probe.js')))
                page.goto(owned.origin + '/__arrangement_stream_probe.html', wait_until='networkidle')
                for mode, end, start in CASES:
                    if args.mode and mode not in args.mode:
                        continue
                    if args.preview_container == 'mp3' and mode in ('bounds', 'cancel'):
                        assert not args.mode, 'Reader bounds/cancellation modes require MP4; MP3 uses native media buffering'
                        continue
                    state = command('edit', epoch=state['epoch'], expected_revision=state['revision'],
                        edits=[{'kind': 'trim_clip', 'clip_id': clip, 'start_frame': 0, 'source_start_frame': 0, 'end_frame': end}])
                    descriptor = command('prepare', epoch=state['epoch'], expected_revision=state['revision'],
                                         side='after', start_frame=start, preview_container=args.preview_container)
                    assert descriptor['preview_priming_frames'] == args.expected_priming and descriptor['start_frame'] == start
                    padding = descriptor['preview_padding_frames']
                    assert type(padding) is int and 0 <= padding <= MIN_PREVIEW_FRAMES, descriptor
                    assert padding == max(0, MIN_PREVIEW_FRAMES - (end - start)), descriptor
                    assert (descriptor['side_duration_frames'], descriptor['sample_rate'],
                            descriptor['preview_sample_rate'], descriptor['channels']) == (end, RATE, RATE, 2), descriptor
                    markers = [(frame - start) / RATE for frame in SOURCE_MARKERS if start <= frame < end] if mode not in ('bounds', 'cancel') else []
                    first = monitor.frames()
                    clock_events.clear()
                    case = {'mode': mode, 'source_start_frame': start, 'source_end_frame': end,
                            'source_marker_frames': [round(marker * RATE) + start for marker in markers],
                            'preview_padding_frames': padding,
                            'source_duration_seconds': (end - start) / RATE,
                            'asset_duration_seconds': (end - start + args.expected_priming + padding) / RATE}
                    result['cases'].append(case)
                    page.evaluate('value=>{window.streamProbeResult=null;void checkArrangementStream(value).then(result=>window.streamProbeResult={result},error=>window.streamProbeResult={error:String(error)});}', {
                        'path': descriptor['stream_path'], 'token': client.token, 'mode': mode, 'descriptor': descriptor, 'projectId': project['id'],
                        'duration': case['asset_duration_seconds'], 'priming': args.expected_priming,
                        'mime': descriptor['preview_mime'], 'expectedMarkers': markers})
                    if mode != 'cancel':
                        page.wait_for_function('()=>document.querySelector("button") || window.streamProbeResult?.error', timeout=15000)
                        assert not page.evaluate('()=>window.streamProbeResult?.error'), page.evaluate('()=>window.streamProbeResult')
                        page.get_by_role('button', name='Play decoder probe', exact=True).click(timeout=15000)
                    page.wait_for_function('()=>window.streamProbeResult!==null', timeout=30000)
                    outcome = page.evaluate('()=>window.streamProbeResult')
                    measured = outcome.get('result') or page.evaluate('()=>window.arrangementStreamProbeSnapshot?.()') or {}
                    case['measurement'] = measured
                    case['pulse_events'] = list(clock_events)
                    # Drain a fixed device-clock interval after EOF, never choose PCM by its amplitude.
                    after = monitor.frames() + RATE // 10
                    raw = monitor.read_window(first, after - first)
                    case['pulse_window'] = {'first': first, 'end': after, 'path': str(monitor.path)}
                    (args.out / f'{mode}-capture.f32').write_bytes(raw.tobytes())
                    if args.preview_container == 'mp3':
                        # Native progressive requests stay untouched. Compare a separate,
                        # identically prepared HTTP encode without delaying media events.
                        command('stop', epoch=state['epoch'], expected_revision=state['revision'], stream_id=descriptor['stream_id'])
                        control = command('prepare', epoch=state['epoch'], expected_revision=state['revision'],
                                          side='after', start_frame=start, preview_container='mp3')
                        status, _, data = client.request('GET', control['stream_path'])
                        assert status == 200 and len(data) < 128 * 1024, (status, len(data))
                        measured['encodedChunks'] = [list(data)]
                        measured['encoded_control'] = 'Separate HTTP stream with identical project revision, source frame and codec'
                        command('stop', epoch=state['epoch'], expected_revision=state['revision'], stream_id=control['stream_id'])
                    if measured.get('encodedChunks'):
                        encoded = args.out / f'{mode}-preview.{args.preview_container}'
                        encoded.write_bytes(bytes(value for chunk in measured.pop('encodedChunks') for value in chunk))
                        decoded = subprocess.run([str(args.ffmpeg), '-nostdin', '-v', 'error', '-i', str(encoded),
                            '-f', 'f32le', '-ar', str(RATE), '-ac', '2', '-'], capture_output=True, check=True, timeout=15)
                        values = array('f')
                        values.frombytes(decoded.stdout)
                        (args.out / f'{mode}-decoded.f32').write_bytes(decoded.stdout)
                        encoded_peaks = pcm_markers(values)
                        measured['independent_encoded'] = {'frames': len(values) // 2, 'peak': max(map(abs, values), default=0),
                                                          'peaks': encoded_peaks}
                        if padding:
                            # Lossy decoder ringing can extend past the real source;
                            # only the far padding tail should already be silent.
                            measured['encoded_far_tail_peak'] = max(map(abs, values[-RATE // 50 * 2:]), default=0)
                            assert measured['encoded_far_tail_peak'] < 1e-5, measured
                        assert len(encoded_peaks) == len(markers), measured
                        for peak, marker in zip(encoded_peaks, markers):
                            assert abs(peak['sample'] - (marker * RATE + args.expected_priming)) <= 2, measured
                    assert 'result' in outcome, outcome
                    assert measured['disposed'] and not measured['errors'] and measured['webAudioGraphs'] == 0, measured
                    if args.preview_container == 'mp4':
                        assert measured['aborted'], measured
                    if markers:
                        playing = next(event['capture_frame'] for event in clock_events if event['event'] == 'playing')
                        peaks = qualify_pcm(measured, raw, first, playing, markers, args.expected_priming)
                        assert measured['finalFrame'] == end, measured
                        assert all(start <= sample['sourceFrame'] <= end for sample in measured['clock']), measured
                        if padding:
                            real_end = (end - start + args.expected_priming) / RATE
                            padding_clock = [sample for sample in measured['clock'] if sample['position'] >= real_end]
                            measured['padding_clock_samples'] = len(padding_clock)
                            assert padding_clock and all(sample['sourceFrame'] == end for sample in padding_clock), measured
                            measured['pulse_post_eof_tail_peak'] = max(map(abs, raw[-RATE // 50 * 2:]), default=0)
                            assert measured['pulse_post_eof_tail_peak'] < 1e-5, measured
                        encoded_peaks = measured['independent_encoded']['peaks']
                        measured['native_gain_ratios'] = [peak['amplitude'] / encoded['amplitude'] for peak, encoded in zip(peaks, encoded_peaks)]
                        assert all(abs(ratio - .7) < .01 for ratio in measured['native_gain_ratios']), measured
                    if mode == 'bounds':
                        assert measured['maxAhead'] < 1.6 and measured['buffered'][0] > 1, measured
                        assert measured['buffered'][1] - measured['buffered'][0] < 4, measured
                    if mode == 'cancel':
                        assert measured['pausedReadsAfter'] == measured['pausedReads'] and measured['bufferedEnd'] < 1.6, measured
                    if args.preview_container == 'mp4':
                        command('stop', epoch=state['epoch'], expected_revision=state['revision'], stream_id=descriptor['stream_id'])
                assert not result['errors'], result['errors']
            finally:
                browser.close()
    finally:
        result['project_cleanup_status'] = client.request('DELETE', f'/v2/projects/{project["id"]}')[0]
        assert result['project_cleanup_status'] == 200
    result['passed'] = True


def main():
    parser = argparse.ArgumentParser()
    for name in ['site', 'out', 'cli', 'worker', 'ffmpeg']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='webkit')
    parser.add_argument('--preview-container', choices=['mp3', 'mp4'], default='mp4')
    parser.add_argument('--mode', choices=[case[0] for case in CASES], action='append', help='Focused diagnostic; omit to qualify all modes')
    parser.add_argument('--server-source', type=Path, help='Private frozen backend source root')
    parser.add_argument('--expected-priming', type=int, default=1105)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    args.arrangement = True
    result = {'scope': 'Shipped native HTML arrangement player + HTTP/native codec + private 48 kHz stereo Pulse sink; actual PCM, no physical device claim',
              'cases': [], 'passed': False, 'preview_container': args.preview_container}
    try:
        with RemoteBrowserFixture(args) as owned:
            result['fixture'] = owned.provenance
            result['requests'] = owned.requests
            verify(owned, args, result)
    except Exception as error:
        result['failure'] = str(error)
        raise
    finally:
        (args.out / 'results.json').write_text(json.dumps(result, indent=2, allow_nan=False) + '\n')


if __name__ == '__main__':
    main()
