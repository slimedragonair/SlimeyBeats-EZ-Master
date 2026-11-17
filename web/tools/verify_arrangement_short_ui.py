"""Normal Server On controls: short-source head/tail PCM and natural EOF."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import traceback
import wave

from playwright.sync_api import expect, sync_playwright
from native_media_capture import native_browser_audio_environment, native_media_capture
from arrangement_remote_diagnostics import ArrangementRemoteDiagnostics
from remote_browser_fixture import RemoteBrowserFixture

RATE = 48000
MARKERS = (256, 2400, 5504, 24000, 48000, 59744)


def marked_source(path, frames):
    samples = bytearray(frames * 2)
    markers = [frame for frame in MARKERS if frame < frames]
    for frame in markers: struct.pack_into('<h', samples, frame * 2, 26000)
    with wave.open(str(path), 'wb') as output:
        output.setparams((1, 2, RATE, 0, 'NONE', 'not compressed'))
        output.writeframes(samples)
    return markers, hashlib.sha256(path.read_bytes()).hexdigest()


def peaks(values):
    assert all(map(math.isfinite, values)), 'Nonfinite native output'
    found = []
    for index in range(0, len(values), 2):
        amplitude, frame = abs(values[index]), index // 2
        if amplitude <= .12: continue
        if not found or frame - found[-1]['end'] > 128:
            found.append({'frame': frame, 'end': frame, 'amplitude': amplitude})
        else:
            found[-1]['end'] = frame
            if amplitude > found[-1]['amplitude']:
                found[-1].update(frame=frame, amplitude=amplitude)
    return found


def verify(browser, owned, args, monitor, result, source, frames, markers, width):
    context = browser.new_context(viewport={'width': width, 'height': 844})
    page = context.new_page()
    row = {'width': width, 'source_frames': frames, 'source_markers': markers, 'errors': []}
    result['cases'].append(row)
    diagnostics = ArrangementRemoteDiagnostics(page, args.browser, row)
    page.add_init_script(path=str(Path(__file__).with_name('arrangement_remote_audit.js')))
    if getattr(args, 'page_setup', None): args.page_setup(page, row)
    a = lambda name: page.locator(f'#s-arrangement-server-page [data-arr="{name}"]')
    try:
        page.goto(owned.origin, wait_until='networkidle')
        if getattr(args, 'page_ready', None): args.page_ready(page, row)
        page.locator('#tab-studio-tools').click(); page.locator('#hybrid-enabled').check()
        expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
        page.evaluate('()=>arrangementRemoteAudit.serverPhase=true')
        page.locator('#s-arrange-entry').click()
        expect(a('workspace')).to_be_visible(timeout=30000); expect(a('add-files')).to_be_enabled(timeout=30000)
        a('files').set_input_files(str(source))
        expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(1)
        a('import-commit').click(); expect(page.locator('#s-play')).to_be_enabled(timeout=30000)
        document = page.evaluate('()=>arrangementRemoteAudit.latest.arrangement')
        assert document['lanes'][0]['clips'][0]['end_frame'] == frames
        expect(page.locator('#s-play')).to_have_text('Play')
        assert not page.evaluate('()=>arrangementRemoteAudit.mediaEvents.some(value=>value.event==="playing")')
        if not getattr(args, 'suffix_only', False):
            first = monitor.frames()
            page.locator('#s-play').click()
            page.wait_for_function('()=>arrangementRemoteAudit.mediaEvents.some(value=>value.event==="ended")', timeout=15000)
            expect(page.locator('#s-play')).to_have_text('Play')
            # Drain a fixed device interval after EOF; never select a window by its signal.
            end = monitor.frames() + RATE // 10
            raw = monitor.read_window(first, end - first)
            (args.out / f'{width}-{frames}-capture.f32').write_bytes(raw.tobytes())
            row['capture'] = {'first': first, 'end': end, 'peaks': peaks(raw), 'path': str(monitor.path)}
            found = row['capture']['peaks']
            assert len(found) == len(markers), row['capture']
            for index in range(1, len(markers)):
                assert abs(found[index]['frame'] - found[index-1]['frame'] - markers[index] + markers[index-1]) <= 2, row['capture']
            assert page.evaluate('()=>arrangementRemoteAudit.latest.arrangement') == document
        row['streams'] = page.evaluate('()=>arrangementRemoteAudit.streams')
        descriptor = next(value for value in reversed(row['streams']) if 'start_frame' in value)
        assert descriptor['start_frame'] == 0
        assert descriptor['preview_priming_frames'] == 1105
        assert descriptor['preview_padding_frames'] == 0, descriptor
        if frames == 60000:
            # Use the imported clip and normal paused seek, not a synthetic 20 ms source.
            if not page.locator('#s-playback-details').evaluate('element=>element.open'):
                page.locator('#s-playback-details > summary').click()
            page.locator('#s-seek').fill('1.23')
            page.wait_for_function('()=>arrangementRemoteAudit.streams.some(value=>value.start_frame===59040)', timeout=15000)
            expect(page.locator('#s-play')).to_have_text('Play')
            ended = page.evaluate('()=>arrangementRemoteAudit.mediaEvents.filter(value=>value.event==="ended").length')
            playing = page.evaluate('()=>arrangementRemoteAudit.mediaEvents.filter(value=>value.event==="playing").length')
            first = monitor.frames(); page.locator('#s-play').click()
            page.wait_for_function('count=>arrangementRemoteAudit.mediaEvents.filter(value=>value.event==="ended").length>count', arg=ended, timeout=15000)
            expect(page.locator('#s-play')).to_have_text('Play')
            assert page.evaluate('()=>arrangementRemoteAudit.mediaEvents.filter(value=>value.event==="playing").length') > playing
            end = monitor.frames() + RATE // 10
            raw = monitor.read_window(first, end - first)
            (args.out / f'{width}-near-eof-capture.f32').write_bytes(raw.tobytes())
            tail = {'first': first, 'end': end, 'peaks': peaks(raw), 'source_start_frame': 59040,
                    'source_end_frame': 60000, 'source_marker_frame': 59744}
            row['near_eof'] = tail
            assert len(tail['peaks']) == 1, tail
            row['streams'] = page.evaluate('()=>arrangementRemoteAudit.streams')
            tail['descriptor'] = next(value for value in reversed(row['streams']) if 'start_frame' in value)
            assert tail['descriptor']['start_frame'] == 59040 and tail['descriptor']['side_duration_frames'] == 60000
            assert tail['descriptor']['preview_sample_rate'] == RATE and tail['descriptor']['preview_priming_frames'] == 1105
            assert tail['descriptor']['preview_padding_frames'] == 4800, tail['descriptor']
            expect(page.locator('#s-seek')).to_have_value('1.25')
            assert page.evaluate('()=>arrangementRemoteAudit.latest.arrangement') == document
        row['audit'] = page.evaluate('()=>arrangementRemoteAudit.summary()')
        assert row['audit']['serverMediaSourceConnections'] == 0 and row['audit']['clientDecodes'] == 0
        assert row['audit']['fullMediaReads'] == 0 and row['audit']['credentials_memory_only']
        assert not row['errors'], row['errors']
        diagnostics.capture(args.out / f'{width}-{frames}.png')
        row.update(passed=True, checks=['no autoplay', 'actual native head/tail PCM and source marker spacing',
                                      'natural EOF restores Play without editing the project', 'no Web Audio media node or client decode'])
        if getattr(args, 'suffix_only', False): row['checks'][1] = 'actual native PCM from the selected 20 ms source suffix'
        if frames == 60000: row['checks'].append('normal paused seek to the final 20 ms plays its real tail marker, preserves source bounds and reports codec-only padding')
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc())
        diagnostics.capture(args.out / f'{width}-{frames}-failure.png')
        raise
    finally:
        row['status'] = page.locator('#s-status').inner_text()
        row['controls'] = {'play': page.locator('#s-play').inner_text(), 'seek': page.locator('#s-seek').input_value()}
        row['streams'] = page.evaluate('()=>arrangementRemoteAudit.streams')
        row['remote_events'] = page.evaluate('()=>arrangementRemoteAudit.events')
        row['media'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
        row['events'] = page.evaluate('()=>arrangementRemoteAudit.mediaEvents')
        try: row['cleanup'] = page.evaluate('()=>arrangementRemoteAudit.cleanup()')
        except Exception as error: row['cleanup_error'] = str(error)
        context.close()


def main():
    parser = argparse.ArgumentParser()
    for name in ['site', 'out', 'cli', 'worker', 'ffmpeg']:
        parser.add_argument('--' + name, type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='webkit')
    parser.add_argument('--server-source', type=Path, help='Frozen matching backend source for qualification')
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True); args.arrangement = True
    sources = []
    for frames in [5760, 60000]:
        path = args.out / f'markers-{frames}.wav'
        markers, digest = marked_source(path, frames); sources.append((path, frames, markers, digest))
    result = {'scope': 'Normal responsive Server On UI + native Pulse PCM; no Web Audio tap or physical device claim', 'cases': []}
    try:
        with RemoteBrowserFixture(args) as owned, native_browser_audio_environment(args.out) as audio, native_media_capture(args.out) as monitor, sync_playwright() as p:
            result.update(fixture=owned.provenance, requests=owned.requests, audio_environment=audio, native_capture=monitor.evidence)
            launch = {'headless': True}
            if args.browser == 'chromium': launch['ignore_default_args'] = ['--mute-audio']
            result['browser_launch'] = launch
            browser = getattr(p, args.browser).launch(**launch); result['browser'] = f'{args.browser} {browser.version}'
            try:
                for width in [320, 390]:
                    for source, frames, markers, digest in sources:
                        verify(browser, owned, args, monitor, result, source, frames, markers, width)
                        assert hashlib.sha256(source.read_bytes()).hexdigest() == digest
            finally: browser.close()
        result['passed'] = True
    finally:
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__': main()
