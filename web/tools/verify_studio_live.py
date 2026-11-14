"""Capture the real browser output: distinct stems, live changes, A/B and Stop."""
import argparse
import functools
import json
from pathlib import Path
import threading

from playwright.sync_api import sync_playwright, expect
from verify_studio_browser import Handler, TestServer, fixture, ROOT
from browser_capture import BrowserDiagnostics


class LiveHandler(Handler):
    def translate_path(self, path):
        probes = {'/__studio_live_probe.js': 'studio_live_probe.js',
                  '/__studio_capture_processor.js': 'studio_capture_processor.js'}
        if path in probes:
            return str(Path(__file__).with_name(probes[path]))
        return super().translate_path(path)


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    server = TestServer(('127.0.0.1', 0), functools.partial(LiveHandler, directory=str(args.site)))
    threading.Thread(target=server.serve_forever, daemon=True).start()
    result = {'errors': [], 'measurements': {}, 'scope': 'Headless real Web Audio with synthetic device; no physical-device or subjective listening claim.'}
    try:
        with sync_playwright() as engine:
            browser = getattr(engine, args.browser).launch(headless=True)
            result['browser'] = args.browser + ' ' + browser.version
            page = browser.new_page(viewport={'width': 1480, 'height': 1000})
            BrowserDiagnostics(page, args.browser, result)
            try:
                page.goto(f'http://127.0.0.1:{server.server_address[1]}/', wait_until='networkidle')
                expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL')
                page.evaluate("async () => { window.liveChecks = await import('/__studio_live_probe.js'); await liveChecks.install(); }")
                page.locator('#file').set_input_files(fixture('main-only.wav', 1320, seconds=30))
                expect(page.locator('#play')).to_be_enabled(timeout=30000)
                page.locator('#play').click()
                expect(page.locator('#play')).to_have_text('Pause')
                page.locator('#tab-studio-tools').click()
                assert page.evaluate('() => !studioProbe.main.playing')
                page.locator('.edit-import > summary').click()
                for kind, hz in [('vocals', 440), ('instrumental', 880)]:
                    page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(kind + '.wav', hz, seconds=30))
                page.locator('#s-import').click()
                expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
                page.evaluate('() => studioProbe.attach()')
                page.locator('#s-play').click()
                expect(page.locator('#s-play')).to_have_text('Pause')

                def measure(name, vocal=True, instrumental=False):
                    page.evaluate('() => studioProbe.resetCapture()')
                    page.wait_for_function('() => studioProbe.frames >= 18000', timeout=10000)
                    pcm = page.evaluate('() => studioProbe.measure()')
                    result['measurements'][name] = pcm
                    for key, wanted in [('vocal', vocal), ('instrumental', instrumental)]:
                        assert pcm[key] > .015 if wanted else pcm[key] < 1e-6, (name, key, pcm)
                    return pcm

                initial = measure('both imported stems', True, True)
                page.evaluate('() => studioProbe.remember()')
                page.get_by_role('checkbox', name='vocals solo', exact=True).check()
                solo = measure('solo excludes instrumental')
                page.evaluate('() => studioProbe.unchanged()')
                assert abs(solo['vocal'] / initial['vocal'] - 1) < .002
                page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('-6')
                lower = measure('gain changes running PCM')
                page.evaluate('() => studioProbe.unchanged()')
                assert abs(lower['vocal'] / solo['vocal'] - 10 ** (-6 / 20)) < .002
                page.get_by_role('checkbox', name='vocals mute', exact=True).check()
                silent = measure('mute wins solo', False, False)
                assert silent['peak'] < 1e-7
                page.evaluate('() => studioProbe.unchanged()')
                page.get_by_role('checkbox', name='vocals mute', exact=True).uncheck()
                page.locator('#s-before').click()
                before = measure('Before is exclusive', True, True)
                assert abs(before['vocal'] / initial['vocal'] - 1) < .002
                position = before['position']
                page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('-3')
                expect(page.locator('#s-after')).to_have_class('selected')
                live = measure('editing Before selects live B')
                assert live['position'] >= position
                assert live['position'] < position + 1
                assert abs(live['vocal'] / initial['vocal'] - 10 ** (-3 / 20)) < .002
                page.evaluate('() => studioProbe.remember()')
                page.evaluate('''() => {
                    const input = document.querySelector('[aria-label="vocals Gain / dB"]');
                    for (let i = 0; i < 80; i++) {
                        input.value = i % 2 ? -3 : -12;
                        input.dispatchEvent(new Event('input'));
                    }
                }''')
                measure('rapid edits settle without bleed')
                page.evaluate('() => studioProbe.unchanged()')
                page.get_by_role('checkbox', name='vocals solo', exact=True).uncheck()
                page.get_by_role('checkbox', name='instrumental mute', exact=True).check()
                measure('muted instrumental absent without solo')
                page.evaluate('() => studioProbe.unchanged()')
                for _ in range(2):
                    page.locator('#s-before').click()
                    measure('repeated Before route', True, True)
                    page.locator('#s-after').click()
                    after = measure('repeated B has no Before bleed')
                    assert abs(after['vocal'] / initial['vocal'] - 10 ** (-3 / 20)) < .002
                expect(page.locator('#s-export')).to_be_disabled()
                expect(page.locator('#s-pending')).to_contain_text('Stem changes are live on B')
                page.locator('#s-mix').click()
                expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
                assert page.evaluate('() => studioProbe.transfers.at(-1).changed') == []
                result['immutable_stems_reused_on_commit'] = True
                with page.expect_download(timeout=30000) as download:
                    page.locator('#s-export').click()
                download.value.save_as(str(args.out / 'committed-current-controls.wav'))
                expect(page.locator('#s-export')).to_be_enabled(timeout=30000)
                saved = page.evaluate('() => studioProbe.savedMeasure()')
                result['measurements']['decoded saved current controls'] = saved
                assert saved['instrumental'] < 1e-6
                assert abs(saved['vocal'] / initial['vocal'] - 10 ** (-3 / 20) / .7) < .002
                result['budget'] = page.evaluate('() => studioProbe.budget()')
                result['offline'] = page.evaluate('() => liveChecks.offlineChecks()')
                result['async'] = page.evaluate('() => liveChecks.asyncChecks()')
                assert not result['errors'], result['errors']
                result['passed'] = True
            except Exception as error:
                result['failure'] = repr(error)
                raise
            finally:
                (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
                browser.close()
    finally:
        server.shutdown()
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--site', type=Path, default=ROOT / 'web/dist')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    run(parser.parse_args())
