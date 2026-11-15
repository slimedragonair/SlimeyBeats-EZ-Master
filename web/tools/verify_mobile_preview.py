"""Smoke-test an explicit hosted current-work preview with synthetic audio only.

Desktop browser touch emulation is not physical iPhone/Safari qualification.
Serve a composed site separately; --url must end in /current-work/.
"""
import argparse
import hashlib
import io
import ipaddress
import json
from pathlib import Path
import re
from urllib.parse import parse_qs, urlsplit, urlunsplit
import zipfile

from playwright.sync_api import expect, sync_playwright

from browser_capture import BrowserDiagnostics
from verify_export_workflows import fixture

HEIGHT = 844
WIDTHS = (390, 320)
TIMEOUT = 120000


def preview_url(value):
    parsed = urlsplit(value)
    try:
        local = parsed.hostname == 'localhost' or ipaddress.ip_address(parsed.hostname).is_loopback
    except ValueError:
        local = False
    if (parsed.scheme not in ('http', 'https') or not parsed.hostname or
            parsed.username is not None or parsed.password is not None or
            parsed.query or parsed.fragment or (parsed.scheme != 'https' and not local)):
        raise argparse.ArgumentTypeError('Use HTTPS or loopback HTTP without credentials, query or fragment')
    path = parsed.path.rstrip('/') + '/'
    if not path.endswith('/current-work/'):
        raise argparse.ArgumentTypeError('Preview URL must end in /current-work/')
    return urlunsplit((parsed.scheme, parsed.netloc, path, '', ''))


def commit_sha(value):
    if not re.fullmatch(r'[a-fA-F0-9]{40}', value):
        raise argparse.ArgumentTypeError('--commit requires a full 40-character Git SHA')
    return value.lower()


def validate_metadata(metadata, expected):
    if not isinstance(metadata, dict) or metadata.get('branch') != 'current-work':
        raise ValueError('Hosted preview metadata must identify branch current-work')
    if metadata.get('commit') != expected:
        raise ValueError(f'Wrong preview build: expected {expected}, got {metadata.get("commit")}')
    if not isinstance(metadata.get('runtime_version'), str) or not re.fullmatch(
            r'[a-f0-9]{16}', metadata['runtime_version']):
        raise ValueError('Built preview metadata requires a 16-hex runtime_version')


def network_violation(request, base):
    parsed, allowed = urlsplit(request.url), urlsplit(base)
    if (parsed.scheme, parsed.hostname, parsed.port) != (allowed.scheme, allowed.hostname, allowed.port):
        return 'Request is outside the preview origin'
    if not parsed.path.startswith(allowed.path):
        return 'Request escaped the preview directory into the production site'
    if request.method != 'GET' or request.post_data_buffer:
        return 'Only bodyless static GET requests are permitted; audio uploads are forbidden'
    if Path(parsed.path).suffix.lower() in ('.wav', '.mp3', '.flac', '.ogg', '.m4a', '.aac'):
        return 'Smoke audio must come from the generated local WAV, never a network recording'
    return None


def reveal(page, selector):
    """Open existing mobile disclosures through their real touch controls."""
    target = page.locator(selector)
    if not target.evaluate('el => el.tagName === "SUMMARY"'):
        for detail in target.locator('xpath=ancestor::details[not(@open)]').all():
            detail.locator(':scope > summary').tap()
    target.scroll_into_view_if_needed()
    return target


def tap(page, selector):
    reveal(page, selector).tap()


def layout(page, result, state):
    measured = page.evaluate('''() => ({
        inner_width: innerWidth,
        client_width: document.documentElement.clientWidth,
        scroll_width: document.documentElement.scrollWidth,
        body_scroll_width: document.body.scrollWidth,
        touch_points: navigator.maxTouchPoints,
    })''')
    result['layouts'].append({'state': state, **measured})
    assert measured['client_width'] == page.viewport_size['width'], measured
    assert measured['scroll_width'] <= measured['client_width'], measured
    assert measured['body_scroll_width'] <= measured['client_width'], measured


def reachable(page, selector):
    target = reveal(page, selector)
    expect(target).to_be_visible()
    expect(target).to_be_enabled()
    assert target.evaluate('''el => {
        const r = el.getBoundingClientRect(), x = r.x + r.width / 2, y = r.y + r.height / 2;
        return r.width > 0 && r.height > 0 && x >= 0 && x < innerWidth &&
            y >= 0 && y < innerHeight && el.contains(document.elementFromPoint(x, y));
    }'''), f'Touch target covered or outside viewport: {selector}'


def download(page, out, selector, name):
    with page.expect_download(timeout=TIMEOUT) as pending:
        tap(page, selector)
    saved = pending.value
    assert saved.failure() is None, saved.failure()
    target = out / name
    saved.save_as(str(target))
    data = target.read_bytes()
    assert data, f'Empty download: {name}'
    return data, {'file': name, 'suggested_filename': saved.suggested_filename,
                  'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest()}


def exercise(page, diagnostics, args, result):
    response = page.goto(args.url, wait_until='networkidle', timeout=TIMEOUT)
    assert response and response.ok, f'Preview navigation failed: {response.status if response else "no response"}'
    assert page.url == args.url, f'Preview redirected to a different URL: {page.url}'
    metadata = page.evaluate('''async () => {
        const response = await fetch(new URL('./preview-build.json', location.href),
            {cache: 'no-store', credentials: 'omit', redirect: 'error'});
        if (!response.ok) throw new Error(`Preview metadata HTTP ${response.status}`);
        return response.json();
    }''')
    result['metadata'] = metadata
    validate_metadata(metadata, args.commit)
    runtime_assets = page.evaluate('''() => Array.from(document.querySelectorAll(
        'script[src], link[rel="stylesheet"][href]'), element => {
            const url = new URL(element.src || element.href);
            return {url: url.href, version: url.searchParams.get('v')};
        })''')
    result['runtime_entrypoints'] = runtime_assets
    assert runtime_assets and all(asset['version'] == metadata['runtime_version']
                                  for asset in runtime_assets), 'HTML/runtime metadata versions differ'
    assert any(urlsplit(asset['url']).path.endswith('/preview-info.js')
               for asset in runtime_assets), 'Versioned preview identity entrypoint is missing'
    result['checks'].append('hosted metadata matches branch, full commit and versioned HTML runtime entrypoints')
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
    result['capabilities'] = page.evaluate('''() => ({
        secure_context: isSecureContext,
        web_audio: typeof (window.AudioContext || window.webkitAudioContext) === 'function',
        audio_worklet: typeof AudioWorkletNode === 'function',
    })''')
    assert all(result['capabilities'].values()), f'Missing required browser audio capability: {result["capabilities"]}'
    if page.locator('#learning-dialog[open]').is_visible():
        tap(page, '#tutorial-decline')
    expect(page.locator('#preview-label')).to_have_text('current-work preview')
    expect(page.locator('#preview-label')).to_be_visible()
    expect(page.locator('#preview-identity')).to_have_text('Build ' + args.commit[:12], timeout=15000)
    tap(page, '#preview-details > summary')
    expect(page.locator('#preview-commit')).to_have_text(args.commit)
    expect(page.locator('#preview-commit')).to_be_visible()
    for width in WIDTHS:
        page.set_viewport_size({'width': width, 'height': HEIGHT})
        layout(page, result, 'expanded build identity')
        reachable(page, '#add')
    tap(page, '#preview-details > summary')
    page.set_viewport_size({'width': WIDTHS[0], 'height': HEIGHT})
    result['checks'].append('visible development label, exact build details and real RUST LOCAL engine')

    # Use the app's visible import button and browser file chooser, not a DOM click.
    generated = fixture('Mobile preview synthetic 4 seconds.wav', 440)
    with page.expect_file_chooser() as chooser:
        tap(page, '#add')
    chooser.value.set_files(generated)
    expect(page.locator('#filename')).to_have_text(generated['name'], timeout=30000)
    expect(page.locator('#mobile-master')).to_be_enabled(timeout=30000)
    tap(page, '#mobile-master')
    expect(page.locator('#export')).to_be_enabled(timeout=TIMEOUT)
    result['checks'].append('touch import and master of a generated 4-second 48 kHz stereo WAV')
    tap(page, '#controls-advanced')
    tap(page, '#tab-eq')
    tap(page, '#band-picker button:nth-child(3)')
    gain = reveal(page, '#bands input[data-band="2"][data-parameter="gain"]')
    original_gain = gain.input_value()
    gain.tap(position={'x': gain.bounding_box()['width'] * .65, 'y': gain.bounding_box()['height'] / 2})
    assert gain.input_value() != original_gain, 'Touch did not change the Manual EQ gain'
    result['manual_eq_gain_db'] = float(gain.input_value())
    tap(page, '#play')
    expect(page.locator('#play')).to_have_text('Pause', timeout=30000)
    expect(page.locator('#live-state')).to_contain_text('EQ live', timeout=30000)
    page.wait_for_function('''() => {
        const elapsed = document.querySelector('#clock').textContent.split(' / ')[0];
        return elapsed !== '0:00';
    }''', timeout=30000)
    result['playback_clock_while_running'] = page.locator('#clock').text_content()
    tap(page, '#stop')
    expect(page.locator('#play')).to_have_text('Play')
    expect(page.locator('#clock')).to_have_text(re.compile(r'0:00 / .+'))
    result['checks'].append('touch Manual EQ gain edit, real live playback clock progress and stopped transport')

    for width in WIDTHS:
        page.set_viewport_size({'width': width, 'height': HEIGHT})
        tap(page, '#tab-overview')
        layout(page, result, 'mastered overview')
        for selector in ('#mobile-master', '#mobile-export', '#play', '#stop'):
            reachable(page, selector)
        page.evaluate('scrollTo(0, 0)')
        diagnostics.capture(args.out / f'phone-{width}-overview.png', full_page=True)
        tap(page, '#tab-eq')
        layout(page, result, 'Manual EQ')
        reachable(page, '#bands input[data-band="2"][data-parameter="gain"]')
        diagnostics.capture(args.out / f'phone-{width}-manual-eq.png', full_page=True)

    page.set_viewport_size({'width': WIDTHS[0], 'height': HEIGHT})
    reveal(page, '#format').select_option('mp3')
    expect(page.locator('#mobile-export')).to_contain_text('MP3')
    audio, record = download(page, args.out, '#mobile-export', 'synthetic-master.mp3')
    result['downloads'].append(record)
    assert audio[:3] == b'ID3' or (audio[0] == 255 and audio[1] & 224 == 224), 'Download lacks MP3 header'
    expect(page.locator('#export')).to_be_enabled(timeout=TIMEOUT)
    tap(page, '#tab-report')
    for width in WIDTHS:
        page.set_viewport_size({'width': width, 'height': HEIGHT})
        layout(page, result, 'report downloads')
        for selector in ('#save-report', '#save-session'):
            reachable(page, selector)
    report_bytes, record = download(page, args.out, '#save-report', 'synthetic-master.report.json')
    result['downloads'].append(record)
    report = json.loads(report_bytes)
    exported = report['exports'][0]
    assert exported['format'] == 'mp3' and exported['bitrate_kbps'] == 320, exported
    assert exported['decoded_metrics']['true_peak_dbtp'] <= report['ceiling_dbtp'] + .005, exported
    assert report['output']['metrics'] == exported['decoded_metrics']
    bundle, record = download(page, args.out, '#save-session', 'synthetic-master.session.zip')
    result['downloads'].append(record)
    with zipfile.ZipFile(io.BytesIO(bundle)) as archive:
        assert archive.testzip() is None, 'Session ZIP failed CRC verification'
        assert set(archive.namelist()) == {'master.mp3', 'analysis.json', 'eq-profile.json',
                                          'settings.json', 'session.json', 'manifest.json'}
        assert archive.read('master.mp3') == audio, 'Session MP3 differs from the download'
        assert json.loads(archive.read('manifest.json'))['original_audio_included'] is False
        assert json.loads(archive.read('settings.json'))['formats'] == ['mp3']
        assert json.loads(archive.read('eq-profile.json'))['bands'][2]['gain'] == result['manual_eq_gain_db']
    result['checks'].append('touch MP3, measured report and CRC-valid matching session ZIP downloads')
    result['checks'].append('no horizontal overflow and uncovered touch controls at 320/390 CSS pixels')
    result['touch_start_events'] = page.evaluate('window.previewSmokeTouchCount')
    assert result['touch_start_events'] > 0, 'No actual touch events reached the app'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', type=preview_url, required=True)
    parser.add_argument('--commit', type=commit_sha, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True, exist_ok=True)
    result = {'passed': False, 'url': args.url, 'expected_commit': args.commit,
              'browser_engine': args.browser, 'checks': [], 'layouts': [], 'downloads': [],
              'requests': [], 'network_violations': [], 'failed_requests': [], 'http_errors': [],
              'errors': [], 'scope': 'desktop browser engine with touch and 320/390 CSS-pixel viewports',
              'physical_iphone': 'not_run', 'physical_audio_output': 'not_run',
              'same_origin_storage_isolation': 'not_run_here; separate preview-channel verifier'}
    page = diagnostics = None
    try:
        with sync_playwright() as engine:
            browser = getattr(engine, args.browser).launch(headless=True)
            result['browser_version'] = browser.version
            options = {'viewport': {'width': WIDTHS[0], 'height': HEIGHT},
                       'has_touch': True, 'accept_downloads': True, 'service_workers': 'block'}
            # Firefox exposes touch but Playwright does not support its is_mobile option.
            if args.browser != 'firefox':
                options['is_mobile'] = True
            context = browser.new_context(**options)
            try:
                def guard(route):
                    reason = network_violation(route.request, args.url)
                    if reason:
                        result['network_violations'].append({'url': route.request.url, 'reason': reason})
                        route.abort()
                    else:
                        route.continue_()

                def block_socket(socket):
                    result['network_violations'].append({'url': socket.url, 'reason': 'WebSockets forbidden'})
                    socket.close()

                context.route('**/*', guard)
                context.route_web_socket('**/*', block_socket)
                context.on('request', lambda request: result['requests'].append({
                    'method': request.method, 'url': request.url, 'resource_type': request.resource_type,
                    'body_bytes': len(request.post_data_buffer or b'')}))
                context.on('requestfailed', lambda request: result['failed_requests'].append({
                    'url': request.url, 'failure': request.failure}))
                context.on('response', lambda response: result['http_errors'].append({
                    'url': response.url, 'status': response.status}) if response.status >= 400 else None)
                page = context.new_page()
                page.add_init_script('''window.previewSmokeTouchCount = 0;
                    document.addEventListener('touchstart', () => window.previewSmokeTouchCount++,
                        {capture: true, passive: true});''')
                diagnostics = BrowserDiagnostics(page, args.browser, result, interface_profile='fresh')
                try:
                    exercise(page, diagnostics, args, result)
                    assert not result['network_violations'], result['network_violations']
                    assert not result['failed_requests'], result['failed_requests']
                    assert not result['http_errors'], result['http_errors']
                    assert not result['errors'], result['errors']
                    assert result['requests'], 'No static asset requests were observed'
                    for request in result['requests']:
                        address = urlsplit(request['url'])
                        if Path(address.path).suffix in ('.js', '.css', '.wasm'):
                            assert parse_qs(address.query).get('v') == [result['metadata']['runtime_version']], (
                                'Loaded runtime asset version differs from metadata', request['url'])
                    result['checks'].append('all requested JS/CSS/WASM versions match hosted runtime metadata')
                    result['checks'].append('all observed network requests were bodyless same-origin preview static GETs')
                    result['passed'] = True
                except Exception as error:
                    result['failure'] = str(error)
                    try:
                        result['status'] = page.locator('#status').inner_text(timeout=2000)
                        diagnostics.capture(args.out / 'failure.png', full_page=True)
                    except Exception as capture_error:
                        result['failure_capture_error'] = str(capture_error)
            finally:
                context.close()
                browser.close()
    except Exception as error:
        result['passed'] = False
        result['failure'] = str(error)
    finally:
        (args.out / 'mobile-preview-verification.json').write_text(
            json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({key: value for key, value in result.items() if key != 'requests'}, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
