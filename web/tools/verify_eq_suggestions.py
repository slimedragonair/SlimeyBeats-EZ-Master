"""Exercise actual compact proposal/apply controls against a fresh local WASM build.

Uses synthetic audio and private browser contexts; no exports or uploads are requested.
Browser output is diagnostic, not physical-device or musical-quality acceptance.
"""
import argparse
import functools
import hashlib
import http.server
import json
from pathlib import Path
import threading
import traceback
import wave

import numpy as np
from playwright.sync_api import expect, sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]
AUDIT = Path(__file__).with_name('eq_suggestion_audit.js')


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_unused):
        pass


def fixture(path, bright=True):
    rate, frames = 48000, 48000 * 6
    random = np.random.default_rng(731)
    spectrum = np.fft.rfft(random.standard_normal(frames))
    frequencies = np.fft.rfftfreq(frames, 1 / rate)

    def noise(low, high):
        values = np.fft.irfft(spectrum * ((frequencies >= low) & (frequencies <= high)), n=frames)
        return values / np.sqrt(np.mean(values * values))

    samples = (0.08 if bright else 0.002) * noise(10000, 16000)
    samples += 0.014 * noise(2000, 8000) + 0.015 * np.sin(np.arange(frames) * (2 * np.pi * 220 / rate))
    assert np.max(np.abs(samples)) < 0.9
    stereo = np.column_stack((samples, samples * 0.9))
    path.parent.mkdir(parents=True, exist_ok=True)
    with wave.open(str(path), 'wb') as writer:
        writer.setparams((2, 2, rate, 0, 'NONE', 'not compressed'))
        writer.writeframes(np.round(stereo * 32767).astype('<i2').tobytes())
    return hashlib.sha256(path.read_bytes()).hexdigest()


def band_state(page):
    return page.locator('#bands').evaluate("el=>[...el.querySelectorAll('input,select')].map(e=>e.type==='checkbox'?e.checked:e.value)")


def edit_warmth(page):
    page.locator('#band-picker button').nth(2).click()
    gain = page.locator('[data-band="2"][data-parameter="gain"]')
    gain.focus()
    gain.press('ArrowUp')


def check_width(browser, base, width, args, result, source, replacement):
    context = browser.new_context(viewport={'width': width, 'height': 844}, device_scale_factor=1)
    page = context.new_page()
    row = {'width': width, 'checks': [], 'errors': [], 'downloads': []}
    result['cases'].append(row)
    diagnostics = BrowserDiagnostics(page, args.browser, row, interface_profile='fresh')
    page.add_init_script("localStorage.setItem('slimeybeats.interface-preferences',JSON.stringify({version:1,advanced:false,tutorial:'declined',step:0}));")
    page.add_init_script(path=str(AUDIT))
    page.on('download', lambda download: row['downloads'].append(download.suggested_filename))
    try:
        page.goto(base, wait_until='networkidle')
        expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        # Hosted branch previews namespace storage after init scripts. Dismiss
        # their fresh welcome through the real control instead of forcing a click.
        if page.locator('#learning-dialog').is_visible():
            page.locator('#tutorial-decline').click()
        expect(page.locator('#learning-dialog')).not_to_be_visible()
        page.locator('#file').set_input_files(str(source))
        expect(page.locator('#master')).to_be_enabled(timeout=30000)
        page.locator('#master:visible, #mobile-master:visible').first.click()
        expect(page.locator('#export')).to_be_enabled(timeout=120000)
        page.locator('#tab-eq').click()
        edit_warmth(page)
        before = band_state(page)
        page.locator('#suggest').click()
        expect(page.locator('#suggest')).to_have_text('Apply curve', timeout=30000)
        proposal = page.evaluate('()=>eqSuggestionAudit.proposals.at(-1)')
        assert proposal['high_frequency']['excess_candidate'], proposal
        assert len(proposal['bands']) == 7 and proposal['bands'][6]['gain'] < 0
        assert band_state(page) == before
        expect(page.locator('#play')).to_have_text('Play')
        assert not page.locator('#eq-suggestion-details').evaluate('el=>el.open')
        summary = page.locator('#eq-note').inner_text()
        assert 'Not applied' in summary and len(summary) < 180, summary
        layout = page.evaluate('''()=>({client:document.documentElement.clientWidth,
            scroll:document.documentElement.scrollWidth,viewport:visualViewport.width,
            note:document.querySelector('#eq-note').getBoundingClientRect().height,
            plot:document.querySelector('#eq-plot').getBoundingClientRect().height})''')
        assert layout['scroll'] <= layout['client'] + 1 <= layout['viewport'] + 1, layout
        assert layout['note'] < 140 and layout['plot'] >= 120, layout
        page.locator('#eq-plot').scroll_into_view_if_needed()
        diagnostics.capture(args.out / f'{width}-proposal.png')
        row['checks'].append('candidate proposal preserves edits; compact collapsed explanation and no horizontal overflow')
        page.locator('#eq-note').click()
        for note in proposal['notes']:
            assert note in page.locator('#eq-suggestion-notes').inner_text()
        page.locator('#eq-note').click()
        baseline_actions = page.evaluate('()=>eqSuggestionAudit.actions.length')
        page.locator('#suggest').click()
        expect(page.locator('#suggest')).to_have_text('✦ Suggest EQ')
        for i, band in enumerate(proposal['bands']):
            for field in ['freq', 'gain', 'q']:
                assert abs(float(page.locator(f'[data-band="{i}"][data-parameter="{field}"]').input_value()) - band[field]) < 1e-6
        page.locator('#play').click()
        expect(page.locator('#play')).to_have_text('Pause')
        page.wait_for_function('()=>eqSuggestionAudit.peak()>0.00001', timeout=10000)
        for selector in ['#original', '#master-source']:
            page.locator(selector).click()
            expect(page.locator(selector)).to_have_class('selected')
            expect(page.locator('#play')).to_have_text('Pause')
        page.locator('#stop').click()
        actions = page.evaluate('start=>eqSuggestionAudit.actions.slice(start)', baseline_actions)
        assert not any(action in ['master', 'export', 'export_mp3', 'session-export'] for action in actions), actions
        row['checks'].append('explicit Apply updates all seven bands; real running A/B retained; no master or export action')
        page.evaluate('()=>{eqSuggestionAudit.hold=true}')
        page.locator('#suggest').click()
        page.wait_for_function('()=>!!eqSuggestionAudit.held', timeout=30000)
        edit_warmth(page)
        edited = band_state(page)
        page.evaluate('()=>eqSuggestionAudit.release()')
        expect(page.locator('#suggest')).to_be_enabled(timeout=30000)
        expect(page.locator('#suggest')).to_have_text('✦ Suggest EQ')
        assert band_state(page) == edited
        expect(page.locator('#eq-note')).to_contain_text('changed while measuring')
        row['checks'].append('late real WASM metadata cannot replace edits made while analysis is pending')
        page.locator('#suggest').click()
        expect(page.locator('#suggest')).to_have_text('Apply curve', timeout=30000)
        page.locator('#file').set_input_files(str(replacement))
        expect(page.locator('#master')).to_be_enabled(timeout=30000)
        expect(page.locator('#suggest')).to_have_text('✦ Suggest EQ')
        assert 'Repeated or sustained broadband' not in page.locator('#eq-suggestion-notes').inner_text()
        assert band_state(page) == edited
        row['checks'].append('same-name same-layout replacement clears source-specific proposal and metadata without resetting EQ')
        assert not row['errors'] and not row['downloads'], row
        row.update(passed=True, layout=layout, summary=summary, high_frequency=proposal['high_frequency'])
    except Exception as error:
        row.update(passed=False, failure=str(error), traceback=traceback.format_exc())
        diagnostics.capture(args.out / f'{width}-failure.png')
        raise
    finally:
        context.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', default='chromium', choices=['chromium', 'firefox', 'webkit'])
    parser.add_argument('--executable')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    source, replacement = args.out / 'original' / 'upper-treble.wav', args.out / 'replacement' / 'upper-treble.wav'
    hashes = {'original': fixture(source), 'replacement': fixture(replacement, False)}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    result = {'cases': [], 'fixture_sha256': hashes, 'scope': 'synthetic browser output; no physical device or listening verdict'}
    try:
        with sync_playwright() as p:
            options = {'headless': True}
            if args.executable:
                options['executable_path'] = args.executable
            browser = getattr(p, args.browser).launch(**options)
            try:
                result['browser'] = f'{args.browser} {browser.version}'
                for width in [320, 390]:
                    check_width(browser, f'http://127.0.0.1:{server.server_address[1]}/', width, args, result, source, replacement)
            finally:
                browser.close()
        result['passed'] = True
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        result['source_unchanged'] = hashlib.sha256(source.read_bytes()).hexdigest() == hashes['original']
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
