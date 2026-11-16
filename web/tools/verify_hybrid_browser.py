"""Exercise the unified editor using real WASM/native DSP and synthetic audio.

Only the exact public API transport is adapted to the owned service fixture.
No DSP replies are simulated; manual listening remains explicitly not_run.
"""
import argparse
from contextlib import closing
import hashlib
import http.client
import json
import math
import os
from pathlib import Path
import re
import secrets
import socket
import struct
import subprocess
import sys
import time
from urllib.parse import parse_qs, urlsplit
import wave
import zipfile

from playwright.sync_api import sync_playwright, expect
from hybrid_test_proxy import start_site

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'rust/tools'))
from verify_exports import independent_measurements

PUBLIC_API = 'https://slimeybeats-preview.tailf8de2d.ts.net'
FRAMES, RATE, CHANNELS = 192000, 48000, 2
TIMEOUT = 180000
SETTINGS = dict(profile='neutral:0.6+edm:0.4', mode='streaming_safe', target_lufs=-16,
                ceiling_dbtp=-2, intensity=0.65, safety=False)
CHECKS = ('normal_controls', 'local_import_analysis', 'reference_and_direction', 'local_master',
          'local_live_eq', 'local_exports', 'local_session_zip', 'server_selection_no_upload',
          'server_master', 'switch_off_retains_result', 'server_live_eq', 'server_exports',
          'server_session_zip', 'outage_no_upload', 'failed_master_keeps_original',
          'malformed_master_keeps_original', 'failed_import_keeps_original', 'remaster_original',
          'responsive_captures', 'source_immutable', 'credentials_not_persisted')

# Observe the actual worker and worklet. Forward messages unchanged, including
# transfer lists; retain only measurements/settings, never replacement audio.
OBSERVE_APP = r'''(() => {
    const state = globalThis.__hybridVerify = {calls: [], replies: [], starts: [], worklets: []};
    const hash = bytes => crypto.subtle.digest('SHA-256', bytes).then(value =>
        Array.from(new Uint8Array(value), x => x.toString(16).padStart(2, '0')).join(''));
    const NativeWorker = globalThis.Worker;
    globalThis.Worker = class extends NativeWorker {
        constructor(url, options) {
            super(url, options);
            if (!new URL(url, location.href).pathname.endsWith('/worker.js')) return;
            const actions = new Map(), send = this.postMessage.bind(this);
            this.postMessage = (message, ...rest) => {
                const entry = {action: message.action, id: message.id, rate: message.rate,
                    channels: message.channels, samples: message.samples?.length,
                    settings: message.settings ? JSON.parse(message.settings) : null};
                if (message.samples) hash(new Uint8Array(message.samples.buffer,
                    message.samples.byteOffset, message.samples.byteLength)).then(value => entry.sha256 = value);
                state.calls.push(entry); actions.set(message.id, message.action);
                return send(message, ...rest);
            };
            this.addEventListener('message', ({data}) => {
                if (data.progress !== undefined) return;
                state.replies.push({action: actions.get(data.id), error: data.error,
                    samples: data.result?.samples?.length,
                    report: data.result?.report ? structuredClone(data.result.report) : null,
                    reference: actions.get(data.id) === 'reference-analysis' ? structuredClone(data.result) : null});
            });
        }
    };
    const ids = new WeakMap(); let nextId = 0;
    const start = AudioBufferSourceNode.prototype.start;
    AudioBufferSourceNode.prototype.start = function(...args) {
        const buffer = this.buffer;
        if (buffer) {
            if (!ids.has(buffer)) ids.set(buffer, ++nextId);
            const entry = {id: ids.get(buffer), frames: buffer.length, rate: buffer.sampleRate,
                channels: buffer.numberOfChannels};
            Promise.all(Array.from({length: buffer.numberOfChannels}, (_, ch) => {
                const plane = buffer.getChannelData(ch);
                return hash(new Uint8Array(plane.buffer, plane.byteOffset, plane.byteLength));
            })).then(values => entry.hashes = values);
            state.starts.push(entry);
        }
        return start.apply(this, args);
    };
    const NativeWorklet = globalThis.AudioWorkletNode;
    globalThis.AudioWorkletNode = class extends NativeWorklet {
        constructor(context, name, options) {
            super(context, name, options);
            if (name !== 'slimey-eq') return;
            const tap = context.createAnalyser(); tap.fftSize = 2048; this.connect(tap);
            const entry = {tap, sent: [], received: []}; state.worklets.push(entry);
            const send = this.port.postMessage.bind(this.port);
            this.port.postMessage = (data, ...rest) => {
                entry.sent.push(structuredClone(data)); return send(data, ...rest);
            };
            this.port.addEventListener('message', ({data}) => entry.received.push(data)); this.port.start();
        }
    };
    state.pcm = () => {
        const entry = state.worklets.at(-1), samples = new Float32Array(2048);
        if (!entry) return null;
        entry.tap.getFloatTimeDomainData(samples);
        let energy = 0, peak = 0;
        for (const value of samples) { if (!Number.isFinite(value)) throw Error('Nonfinite live PCM');
            energy += value * value; peak = Math.max(peak, Math.abs(value)); }
        return {rms: Math.sqrt(energy / samples.length), peak, frames: samples.length,
            ready: entry.received.some(value => value.ready), updates: entry.sent.length,
            acknowledgements: entry.received.filter(value => value.revision !== undefined).length,
            worklets: state.worklets.length, starts: state.starts.length};
    };
})()'''


def unused_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def synthetic_audio(path, reference=False):
    with wave.open(str(path), 'wb') as writer:
        writer.setparams((2, 2, 16000, 0, 'NONE', 'not compressed'))
        writer.writeframes(b''.join(struct.pack('<hh',
            round(4500 * math.sin(2 * math.pi * (660 if reference else 220) * frame / 16000)
                  + 1000 * math.sin(2 * math.pi * 1100 * frame / 16000)),
            round(3000 * math.sin(2 * math.pi * (880 if reference else 330) * frame / 16000)
                  + 800 * math.sin(2 * math.pi * 80 * frame / 16000))) for frame in range(16000 * 4)))


def uploaded_pcm(data, frames):
    assert data[:4] == b'RIFF' and data[8:12] == b'WAVE'
    assert struct.unpack_from('<I', data, 4)[0] + 8 == len(data)
    offset, layout, pcm = 12, None, None
    while offset < len(data):
        tag, count = struct.unpack_from('<4sI', data, offset)
        offset += 8
        assert count <= len(data) - offset
        if tag == b'fmt ':
            fmt, channels, rate, byte_rate, align, bits = struct.unpack_from('<HHIIHH', data, offset)
            assert (fmt, bits, byte_rate, align) == (3, 32, rate * channels * 4, channels * 4)
            layout = (rate, channels)
        elif tag == b'data':
            assert pcm is None
            pcm = memoryview(data)[offset:offset + count]
        offset += count + (count & 1)
    assert layout == (RATE, CHANNELS) and pcm is not None and len(pcm) == frames * CHANNELS * 4
    assert all(math.isfinite(value) for value, in struct.iter_unpack('<f', pcm))
    return dict(rate=RATE, channels=CHANNELS, frames=frames, sha256=hashlib.sha256(pcm).hexdigest())


class ApiTransport:
    """Forward the exact API to a real fixture; observe and fault transport."""
    def __init__(self, origin):
        self.origin, self.status = origin, 'online'
        self.frames = FRAMES
        self.fail_create = self.corrupt_audio = False
        self.requests, self.jobs, self.reports, self.errors, self.tokens = [], {}, [], [], []

    def writes(self):
        return sum(method in ('POST', 'PUT') and path != '/v1/session' for method, path in self.requests)

    def route(self, route):
        request, parsed = route.request, urlsplit(route.request.url)
        request_origin = f'{parsed.scheme}://{parsed.netloc}'
        if request_origin != PUBLIC_API:
            if request.method == 'GET' and (request_origin == self.origin or request.url.startswith(f'blob:{self.origin}/')):
                route.continue_()
            else:
                self.errors.append(f'Unexpected destination or method: {request.method} {request_origin}')
                route.abort()
            return
        path = parsed.path
        self.requests.append((request.method, path))
        try:
            if path == '/v1/status' and self.status != 'online':
                route.fulfill(status=503, json={'error': 'owned fixture outage'})
                return
            if path == '/v1/jobs' and request.method == 'POST' and self.fail_create:
                route.fulfill(status=503, json={'error': 'owned fixture creation failure'})
                return
            payload = request.post_data_buffer
            if request.method in ('POST', 'PUT') and payload is None:
                raise AssertionError(f'API adapter lost actual {request.method} body: {path}')
            remote = route.fetch(url=f'{self.origin}{path}' + (f'?{parsed.query}' if parsed.query else ''),
                headers={**request.headers, 'Origin': self.origin}, post_data=payload)
            if path == '/v1/session' and remote.ok:
                self.tokens.append(remote.json()['token'])
            if path == '/v1/jobs' and request.method == 'POST' and remote.ok:
                self.jobs[remote.json()['id']] = {'request': json.loads(payload), 'uploaded': bytearray(), 'deleted': False}
            match = re.fullmatch(r'/v1/jobs/([a-f0-9]{32})/(upload|run)', path)
            if match and remote.ok:
                job = self.jobs[match[1]]
                if match[2] == 'upload':
                    assert int(parse_qs(parsed.query)['offset'][0]) == len(job['uploaded'])
                    job['uploaded'].extend(payload)
                elif request.method == 'POST':
                    assert len(job['uploaded']) == job['request']['bytes']
                    job['pcm'] = uploaded_pcm(job['uploaded'], self.frames)
                    job['uploaded'].clear()
            if request.method == 'DELETE' and remote.ok:
                assert remote.json().get('deleted') is True
                self.jobs[path.rsplit('/', 1)[-1]]['deleted'] = True
            if '/artifacts/' in path and remote.ok:
                kind = remote.headers.get('content-type', '').split(';')[0]
                if kind == 'application/json':
                    self.reports.append(remote.json())
                elif kind == 'audio/wav' and self.corrupt_audio:
                    # Damage genuine native output only at the transport boundary.
                    data = bytearray(remote.body()); data[:4] = b'RIFX'
                    route.fulfill(response=remote, body=bytes(data))
                    return
            route.fulfill(response=remote)
        except Exception as error:
            self.errors.append(str(error))
            route.abort()


def control(page, selector):
    locator = page.locator(selector)
    for parent in reversed(locator.locator('xpath=ancestor::details[not(@open)]').all()):
        parent.locator(':scope > summary').click()
    return locator


def slider(page, selector, value):
    item = control(page, selector)
    expect(item).to_be_visible()
    item.evaluate('(node, value) => {node.value = value; node.dispatchEvent(new Event("input", {bubbles:true}));}', str(value))


def primary(page, action):
    mobile = page.locator(f'#mobile-{action}')
    return mobile if mobile.is_visible() else page.locator(f'#{action}')


def snapshot(page):
    return page.evaluate('''() => Object.fromEntries(['profile','blend-profile','blend','target','ceiling','intensity',
        'custom-target','safety'].map(id => {const node = document.getElementById(id);
        return [id, node.type === 'checkbox' ? node.checked : node.value]}))''')


def latest_report(page, action):
    return page.evaluate('(action) => __hybridVerify.replies.filter(item => item.action === action && item.report).at(-1)', action)


def assert_report(report, settings, frames=FRAMES):
    for key, value in settings.items():
        assert report['configuration'][key] == value, (key, report['configuration'][key], value)
    assert report['ceiling_dbtp'] == settings['ceiling_dbtp']
    for name in ('input', 'output'):
        analysis = report[name]
        assert analysis['sample_rate'] == RATE and analysis['channels'] == CHANNELS
        assert abs(analysis['duration_s'] - frames / RATE) <= 1 / RATE


def download(page, selector, destination):
    with page.expect_download(timeout=TIMEOUT) as event:
        control(page, selector).click()
    event.value.save_as(destination)
    expect(page.locator('#job')).to_be_hidden(timeout=TIMEOUT)
    assert destination.stat().st_size > 0


def verify_audio(path, lossy=False, frames=FRAMES):
    probe = subprocess.run(['ffprobe', '-v', 'error', '-show_streams', '-of', 'json', str(path)],
                           capture_output=True, check=True, text=True, timeout=30)
    info = json.loads(probe.stdout)['streams'][0]
    assert (int(info['sample_rate']), int(info['channels'])) == (RATE, CHANNELS)
    decoded = subprocess.run(['ffmpeg', '-v', 'error', '-nostdin', '-i', str(path),
        '-f', 'f32le', '-acodec', 'pcm_f32le', 'pipe:1'], capture_output=True, check=True, timeout=30).stdout
    assert len(decoded) % (CHANNELS * 4) == 0
    decoded_frames = len(decoded) // (CHANNELS * 4)
    assert (frames <= decoded_frames <= frames + RATE // 4) if lossy else decoded_frames == frames
    assert all(math.isfinite(value) for value, in struct.iter_unpack('<f', decoded))
    lufs, peak = independent_measurements(path)
    assert math.isfinite(lufs) and math.isfinite(peak) and peak <= SETTINGS['ceiling_dbtp'] + 0.15, (lufs, peak)
    return dict(frames=decoded_frames, rate=RATE, channels=CHANNELS, lufs=lufs, true_peak_dbtp=peak,
                decoded_pcm_sha256=hashlib.sha256(decoded).hexdigest())


def live_eq(page, transport):
    control(page, '#tab-eq').click()
    control(page, '#reset-eq').click()
    writes = transport.writes()
    jobs = page.evaluate('__hybridVerify.calls.filter(x => ["master","adopt-master","export"].includes(x.action)).length')
    page.locator('#stop').click(); page.locator('#master-source').click(); page.locator('#play').click()
    page.wait_for_function('() => {const pcm = __hybridVerify.pcm(); return pcm?.ready && pcm.rms > 0.001}', timeout=15000)
    page.wait_for_timeout(180)
    before = page.evaluate('__hybridVerify.pcm()')
    page.locator('#band-picker button').nth(2).click()
    slider(page, '#bands input[data-band="2"][data-parameter="gain"]', -3)
    slider(page, '#eq-output', -6)
    page.wait_for_function('''() => {const item = __hybridVerify.worklets.at(-1);
        const revision = item.sent.filter(value => value.settings).at(-1)?.revision;
        return revision !== undefined && item.received.some(value => value.revision === revision)}''', timeout=15000)
    page.wait_for_timeout(300)
    after = page.evaluate('__hybridVerify.pcm()')
    assert 0 < after['rms'] < before['rms'] * 0.8, (before, after)
    assert after['worklets'] == before['worklets'] and after['starts'] == before['starts']
    assert transport.writes() == writes
    assert page.evaluate('__hybridVerify.calls.filter(x => ["master","adopt-master","export"].includes(x.action)).length') == jobs
    page.locator('#stop').click()
    return {'before': before, 'after': after, 'physical_listening': False}


def exports(page, transport, out, label, settings):
    writes, audio = transport.writes(), {}
    for fmt, extension in [('wav32f', 'wav'), ('mp3', 'mp3')]:
        control(page, '#format').select_option(fmt)
        destination = out / f'{label}-master.{extension}'
        download(page, '#mobile-export' if page.locator('#mobile-export').is_visible() else '#export', destination)
        audio[fmt] = verify_audio(destination, lossy=fmt == 'mp3', frames=transport.frames)
    control(page, '#format').select_option('wav32f')
    control(page, '#tab-report').click()
    bundle = out / f'{label}-session.zip'
    download(page, '#save-session', bundle)
    with zipfile.ZipFile(bundle) as archive:
        assert archive.testzip() is None
        names = set(archive.namelist())
        assert {'master.wav', 'settings.json', 'analysis.json', 'session.json', 'manifest.json', 'eq-profile.json'} <= names
        selected, session = json.loads(archive.read('settings.json')), json.loads(archive.read('session.json'))
        for key, value in settings.items():
            assert selected[key] == value, (key, selected[key], value)
        assert selected['eq_output_db'] == -6 and selected['manual_eq'][2]['gain'] == -3
        assert session['source']['sample_rate'] == RATE and session['source']['channels'] == CHANNELS
        assert session['source']['name'] == 'synthetic-source.wav'
        assert session['reference']['name'] == 'synthetic-reference.wav'
        assert session['original_audio_included'] is False and session['reference_audio_included'] is False
    report_path = out / f'{label}-report.json'
    download(page, '#save-report', report_path)
    assert_report(json.loads(report_path.read_text(encoding='utf-8')), settings, transport.frames)
    assert transport.writes() == writes, 'Local exports must not request another server master'
    return audio, {'entries': sorted(names), 'settings_match': True, 'report': report_path.name}


def screenshots(page, out, args, label):
    control(page, '#tab-overview').click()
    records = []
    for width, height in dict.fromkeys([(390, 844), (320, 844), (1440, 900), (args.width, args.height)]):
        page.set_viewport_size({'width': width, 'height': height})
        page.evaluate('window.scrollTo(0, 0)')
        geometry = page.evaluate('''() => ({client:document.documentElement.clientWidth,
            scroll:document.documentElement.scrollWidth, header:document.querySelector('.masthead').getBoundingClientRect().toJSON()})''')
        assert geometry['scroll'] <= geometry['client'] + 1, geometry
        prefix = out / f'{label}-{width}x{height}'
        page.locator('.masthead').screenshot(path=str(prefix) + '-header.png')
        page.screenshot(path=str(prefix) + '-viewport.png')
        page.screenshot(path=str(prefix) + '-full.png', full_page=True)
        records.append(dict(label=label, width=width, height=height, geometry=geometry,
            server_enabled=page.locator('#hybrid-enabled').is_checked(),
            server_status=page.locator('#hybrid-availability').inner_text()))
    page.set_viewport_size({'width': args.width, 'height': args.height})
    return records


def run_workflow(page, args, transport, result, source, reference):
    def passed(name, details=None):
        result['checks'][name] = {'passed': True, 'status': 'passed', 'details': details}
        print(f'PASS {name}', flush=True)

    page.goto(transport.origin, wait_until='networkidle')
    if page.locator('#learning-dialog[open]').is_visible():
        page.locator('#tutorial-decline').click()
    if args.text_scale != 1:
        page.evaluate('(scale) => document.documentElement.style.fontSize = `${16 * scale}px`', args.text_scale)
    expect(page.locator('#add')).to_be_enabled(timeout=30000)
    expect(page.locator('#hybrid-enabled')).not_to_be_checked()
    assert not transport.requests
    for selector in ('#hybrid-consent', '#hybrid-file', '#hybrid-endpoint', '#hybrid-token', '#hybrid-submit'):
        assert page.locator(selector).count() == 0, selector
    assert page.locator('#hybrid-studio input[type="file"], #hybrid-studio select').count() == 0
    passed('normal_controls', 'No operator panel, second audio picker or consent form')
    result['screenshots'] = screenshots(page, args.out, args, 'local')
    with page.expect_file_chooser() as chooser:
        page.locator('#add').click()
    chooser.value.set_files(source)
    expect(page.locator('#filename')).to_have_text(source.name, timeout=30000)
    expect(page.locator('#filemeta')).to_contain_text('48,000')
    page.locator('#analyze').click()
    expect(page.locator('#status')).to_contain_text('Analysis complete', timeout=30000)
    page.wait_for_function('() => __hybridVerify.calls.some(x => x.action === "load" && x.sha256)')
    loaded = page.evaluate('__hybridVerify.calls.find(x => x.action === "load")')
    result['decoded_source'] = loaded
    assert (loaded['rate'], loaded['channels']) == (RATE, CHANNELS), loaded
    assert loaded['samples'] % CHANNELS == 0, loaded
    frames = loaded['samples'] // CHANNELS
    # Linux WebKit 26's 16 -> 48 kHz decoder drops two tail frames for this
    # fixture. Record that import behavior; masters must still match its exact
    # immutable AudioBuffer, with no trimming/padding at the exchange boundary.
    assert frames in ((FRAMES - 2, FRAMES) if args.browser == 'webkit' else (FRAMES,)), loaded
    transport.frames = frames
    assert not transport.requests
    passed('local_import_analysis', dict(encoded_rate=16000, decoded_rate=RATE, frames=frames,
        nominal_frames=FRAMES, decoder_frame_delta=frames - FRAMES, original_sha256=loaded['sha256']))
    control(page, '#controls-advanced').click()
    control(page, '#profile').select_option('neutral')
    control(page, '#blend-profile').select_option('edm'); slider(page, '#blend', 40)
    control(page, '#custom-target').check()
    page.locator('#target').fill('-16'); page.locator('#target').dispatch_event('input')
    slider(page, '#ceiling', -2); slider(page, '#intensity', 0.65)
    control(page, '#safety').uncheck()
    with page.expect_file_chooser() as chooser:
        control(page, '#reference').click()
    chooser.value.set_files(reference)
    expect(page.locator('#reference-name')).to_contain_text(reference.name, timeout=30000)
    expect(page.locator('#job')).to_be_hidden(timeout=30000)
    passed('reference_and_direction', SETTINGS)
    primary(page, 'master').click()
    expect(page.locator('#export')).to_be_enabled(timeout=TIMEOUT)
    local = latest_report(page, 'master')
    assert local['samples'] == frames * CHANNELS
    assert_report(local['report'], SETTINGS, frames)
    assert not transport.requests
    passed('local_master', 'Real WASM; exact geometry and selected settings')
    passed('local_live_eq', live_eq(page, transport))
    audio, bundle = exports(page, transport, args.out, 'local', SETTINGS)
    passed('local_exports', audio); passed('local_session_zip', bundle)
    page.locator('#hybrid-enabled').check()
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=15000)
    expect(page.locator('#hybrid-check')).to_be_hidden(timeout=15000)
    page.locator('#analyze').click()
    expect(page.locator('#status')).to_contain_text('Analysis complete', timeout=30000)
    assert transport.writes() == 0 and not transport.jobs
    passed('server_selection_no_upload', 'Guest connection and Analyze never send audio')
    primary(page, 'master').click()
    expect(page.locator('#status')).to_contain_text('Master ready', timeout=TIMEOUT)
    expect(page.locator('#export')).to_be_enabled(timeout=TIMEOUT)
    adopted = latest_report(page, 'adopt-master')
    assert adopted['samples'] == frames * CHANNELS
    assert_report(adopted['report'], SETTINGS, frames)
    job = next(iter(transport.jobs.values()))
    assert job['pcm']['sha256'] == loaded['sha256'], 'Upload must use original, not local master'
    assert job['request']['filename'] == 'browser-track.wav' and job['request']['format'] == 'wav32f'
    for key, value in SETTINGS.items():
        assert job['request'][key] == value, (key, job['request'][key], value)
        assert transport.reports[-1][key] == value, (key, transport.reports[-1])
    measured_reference = page.evaluate('__hybridVerify.replies.filter(x => x.action === "reference-analysis").at(-1).reference')
    assert job['request']['reference_analysis'] == measured_reference
    assert transport.reports[-1]['reference_supplied'] is True and job['deleted']
    passed('server_master', dict(native_report_matches=True, reference_matches=True, upload_pcm=job['pcm'], deletion_confirmed=True))
    expect(page.locator('#hybrid-enabled')).to_be_checked()
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]')
    result['screenshots'] += screenshots(page, args.out, args, 'server-enabled-complete')
    output = page.locator('#output-metrics').inner_text()
    page.locator('#hybrid-enabled').uncheck()
    expect(page.locator('#master-source')).to_be_enabled(); expect(page.locator('#export')).to_be_enabled()
    assert page.locator('#output-metrics').inner_text() == output
    passed('switch_off_retains_result', 'Server master stays editable and exportable locally')
    passed('server_live_eq', live_eq(page, transport))
    audio, bundle = exports(page, transport, args.out, 'server', SETTINGS)
    passed('server_exports', audio); passed('server_session_zip', bundle)
    result['screenshots'] += screenshots(page, args.out, args, 'server-retained-local')
    passed('responsive_captures', result['screenshots'])
    page.locator('#hybrid-enabled').check()
    expect(page.locator('#hybrid-check')).to_be_hidden(timeout=15000)
    writes = transport.writes(); transport.status = 'down'
    page.evaluate('document.dispatchEvent(new Event("visibilitychange"))')
    expect(page.locator('#hybrid-availability')).to_have_text('[Server down]', timeout=15000)
    transport.status = 'online'
    page.evaluate('document.dispatchEvent(new Event("visibilitychange"))')
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=15000)
    assert transport.writes() == writes
    passed('outage_no_upload', 'Health failure/recovery never creates, uploads or runs a job')
    original_settings = snapshot(page)
    prior_adoptions = page.evaluate('__hybridVerify.replies.filter(x => x.action === "adopt-master" && x.report).length')
    transport.fail_create = True
    primary(page, 'master').click()
    expect(page.locator('#status')).to_contain_text('failed', timeout=30000)
    expect(page.locator('#job')).to_be_hidden(timeout=30000)
    transport.fail_create = False
    assert snapshot(page) == original_settings
    expect(page.locator('#filename')).to_have_text(source.name); expect(page.locator('#master-source')).to_be_enabled()
    assert page.evaluate('__hybridVerify.replies.filter(x => x.action === "adopt-master" && x.report).length') == prior_adoptions
    passed('failed_master_keeps_original', 'Creation failure keeps source, previous preview and settings')
    control(page, '#hybrid-check').click()
    expect(page.locator('#hybrid-check')).to_be_hidden(timeout=15000)
    transport.corrupt_audio = True
    primary(page, 'master').click()
    expect(page.locator('#status')).to_contain_text('RIFF/WAVE', timeout=TIMEOUT)
    expect(page.locator('#job')).to_be_hidden(timeout=30000)
    transport.corrupt_audio = False
    assert snapshot(page) == original_settings
    expect(page.locator('#filename')).to_have_text(source.name)
    assert page.evaluate('__hybridVerify.replies.filter(x => x.action === "adopt-master" && x.report).length') == prior_adoptions
    control(page, '#hybrid-delete').click()
    expect(page.locator('#hybrid-delete')).to_be_hidden(timeout=15000)
    passed('malformed_master_keeps_original', 'Corrupted native WAV rejected before adoption; deletion confirmed')
    writes = transport.writes()
    # Loaded mobile layouts collapse the library; use its ordinary file input
    # for a rejected replacement without introducing a second picker.
    page.locator('#file').set_input_files({'name': 'synthetic-invalid.wav',
        'mimeType': 'audio/wav', 'buffer': b'invalid synthetic WAV'})
    expect(page.locator('#import-error')).to_be_visible(timeout=30000)
    expect(page.locator('#filename')).to_have_text(source.name)
    assert snapshot(page) == original_settings and transport.writes() == writes
    passed('failed_import_keeps_original', 'Rejected replacement import retains source and settings')
    if page.locator('#hybrid-check').is_visible():
        page.locator('#hybrid-check').click()
    expect(page.locator('#hybrid-check')).to_be_hidden(timeout=15000)
    control(page, '#safety').check()
    remaster_settings = {**SETTINGS, 'safety': True}
    primary(page, 'master').click()
    expect(page.locator('#status')).to_contain_text('Master ready', timeout=TIMEOUT)
    expect(page.locator('#job')).to_be_hidden(timeout=TIMEOUT)
    assert_report(latest_report(page, 'adopt-master')['report'], remaster_settings, frames)
    final_job = list(transport.jobs.values())[-1]
    assert final_job['pcm']['sha256'] == loaded['sha256'] == job['pcm']['sha256']
    assert final_job['request']['safety'] is True and transport.reports[-1]['safety'] is True
    assert all(item['deleted'] for item in transport.jobs.values())
    passed('remaster_original', dict(original_sha256=loaded['sha256'], safety_snapshot=True, all_jobs_deleted=True))
    page.locator('#hybrid-enabled').uncheck(); page.locator('#original').click()
    previous_starts = page.evaluate('__hybridVerify.starts.length')
    page.locator('#play').click()
    page.wait_for_function('''count => __hybridVerify.starts.length > count &&
        __hybridVerify.starts.slice(count).every(item => item.hashes)''', arg=previous_starts)
    page.locator('#stop').click()
    starts = page.evaluate('__hybridVerify.starts')
    result['playback_sources'] = starts
    originals = [item for item in starts if item['id'] == starts[0]['id']]
    assert len(originals) >= 3 and all(item.get('hashes') == originals[0]['hashes'] for item in originals)
    passed('source_immutable', dict(buffer_identity=originals[0]['id'], planar_sha256=originals[0]['hashes']))
    storage = page.evaluate('JSON.stringify({local:{...localStorage},session:{...sessionStorage}})')
    assert transport.tokens and all(token not in storage for token in transport.tokens)
    assert not transport.errors and not result['errors'], (transport.errors, result['errors'])
    passed('credentials_not_persisted', 'Guest credentials absent from localStorage/sessionStorage')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--guest', action='store_true', default=True, help='Compatibility flag; always uses a guest')
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--width', type=int, default=390)
    parser.add_argument('--height', type=int, default=844)
    parser.add_argument('--text-scale', type=float, default=1)
    args = parser.parse_args()
    if args.width < 320 or args.height < 400 or not math.isfinite(args.text_scale) or args.text_scale <= 0:
        parser.error('Use width >= 320, height >= 400 and a positive finite text scale')
    args.out.mkdir(parents=True, exist_ok=True)
    result = {'success': False, 'browser': args.browser, 'access': 'guest', 'site': str(args.site.resolve()),
        'api_transport_adapter': PUBLIC_API, 'text_scale': args.text_scale, 'errors': [],
        'checks': {name: {'passed': False, 'status': 'not_run'} for name in CHECKS},
        'manual_listening': {'passed': False, 'status': 'not_run', 'reason': 'No human or physical-device audition'},
        'native_unintercepted_transport': {'passed': False, 'status': 'not_run', 'reason': 'Exact-origin API adapter used'}}
    process = site = transport = None
    token = secrets.token_urlsafe(36)
    try:
        policy = (args.site / 'hybrid/endpoint-policy.js').read_text(encoding='utf-8')
        index = (args.site / 'index.html').read_text(encoding='utf-8')
        build = re.search(r'hybrid/studio\.js\?v=([a-f0-9]+)', index)
        result['site_build'] = build[1] if build else 'unknown'
        configured = re.search(r'export const PUBLIC_API_ORIGIN\s*=\s*"([^"]+)"', policy)
        assert configured and configured[1] == PUBLIC_API, 'Candidate must use the exact Funnel pin'
        help_text = subprocess.run([str(args.cli.resolve()), '--help'], capture_output=True, check=True,
                                   text=True, timeout=20).stdout
        assert '--reference-analysis' in help_text, 'Native CLI needs --reference-analysis; rebuild it first'
        result['cli_sha256'] = hashlib.sha256(args.cli.read_bytes()).hexdigest()
        source, reference = args.out / 'synthetic-source.wav', args.out / 'synthetic-reference.wav'
        synthetic_audio(source); synthetic_audio(reference, reference=True)
        digests = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in (source, reference)}
        api_port = unused_port(); site = start_site(args.site.resolve(), api_port)
        origin = f'http://127.0.0.1:{site.server_port}'; transport = ApiTransport(origin)
        env = dict(os.environ, SLIMEYBEATS_HYBRID_TOKEN=token, SLIMEYBEATS_HYBRID_PUBLIC='1',
                   SLIMEYBEATS_HYBRID_ORIGINS=origin, PYTHONDONTWRITEBYTECODE='1')
        with (args.out / 'service.log').open('wb') as log:
            process = subprocess.Popen([sys.executable, '-m', 'hybrid.server', '--cli', str(args.cli.resolve()),
                '--data-dir', str((args.out / 'private-jobs').resolve()), '--port', str(api_port)], cwd=ROOT,
                env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            deadline = time.monotonic() + 20
            while True:
                try:
                    with closing(http.client.HTTPConnection('127.0.0.1', api_port, timeout=1)) as remote:
                        remote.request('GET', '/health')
                        if remote.getresponse().status == 200:
                            break
                except OSError:
                    pass
                if time.monotonic() > deadline or process.poll() is not None:
                    raise RuntimeError('Owned hybrid fixture failed to start; inspect service.log')
                time.sleep(0.1)
            with sync_playwright() as playwright:
                browser = getattr(playwright, args.browser).launch(headless=True)
                result['browser_version'] = browser.version
                try:
                    context = browser.new_context(viewport={'width': args.width, 'height': args.height}, accept_downloads=True)
                    context.add_init_script(OBSERVE_APP)
                    if args.browser == 'webkit':
                        # Intercepted WebKit Blob PUT bodies can be absent. Adapt
                        # only the real app Blob at the exact configured API.
                        context.add_init_script(r'''(() => {const send = fetch.bind(globalThis);
                            globalThis.fetch = (input, options) => {
                                const url = new URL(typeof input === 'string' ? input : input.url, location.href);
                                if (url.origin === %s && /^\/v1\/jobs\/[a-f0-9]{32}\/upload$/.test(url.pathname)
                                    && options?.method === 'PUT' && options.body instanceof Blob)
                                    return options.body.arrayBuffer().then(body => send(input, {...options, body}));
                                return send(input, options);
                            };})()''' % json.dumps(PUBLIC_API))
                    context.route('**/*', transport.route)
                    page = context.new_page(); page.set_default_timeout(15000)
                    page.on('pageerror', lambda error: result['errors'].append(str(error)))
                    try:
                        run_workflow(page, args, transport, result, source, reference)
                    except Exception:
                        try:
                            result['failure_ui'] = {key: page.locator(f'#{key}').inner_text(timeout=1000)
                                for key in ('status', 'hybrid-availability', 'hybrid-status')}
                            page.screenshot(path=str(args.out / 'failure.png'), full_page=True, timeout=10000)
                        except Exception:
                            pass
                        raise
                finally:
                    browser.close()
        assert all(hashlib.sha256(path.read_bytes()).hexdigest() == digest for path, digest in digests.items())
        result['success'] = all(item['passed'] for item in result['checks'].values())
    except Exception as error:
        result['failure'] = f'{type(error).__name__}: {error}'
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=5)
        if site is not None:
            site.shutdown(); site.server_close()
        if transport:
            result.update(api_requests=transport.requests, adapter_errors=transport.errors, native_reports=transport.reports)
        encoded = json.dumps(result, indent=2, allow_nan=False)
        for secret in [token, *(transport.tokens if transport else [])]:
            encoded = encoded.replace(secret, '[credential removed]')
        (args.out / 'results.json').write_text(encoded + '\n', encoding='utf-8')
    print(encoded)
    return 0 if result['success'] else 1


if __name__ == '__main__':
    sys.exit(main())
