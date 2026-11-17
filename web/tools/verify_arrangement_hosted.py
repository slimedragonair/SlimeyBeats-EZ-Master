"""Run existing arrangement UI/delivery probes against an exact hosted preview.

Product HTML, CSS, workers, WASM, encoding, edits and downloads use the published
site. This pass checks transport progress and exported PCM, not live output PCM.
Generated fixtures never leave the browser; no request with a body is allowed.
"""
import argparse
import hashlib
import json
from pathlib import Path
from urllib.parse import urlsplit

from playwright.sync_api import expect, sync_playwright
from verify_arrangement_browser import fixture
from verify_arrangement_project_ui import verify as verify_project
from verify_arrangement_mp3_ui import verify as verify_mp3
from verify_mobile_preview import validate_metadata


class HostedPage:
    def __init__(self, page, commit, result):
        self.page, self.commit, self.result = page, commit, result

    def __getattr__(self, name):
        return getattr(self.page, name)

    def goto(self, url, **options):
        response = self.page.goto(url, **options)
        expect(self.page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
        metadata = self.page.evaluate("async()=>{const r=await fetch('./preview-build.json',{cache:'no-store'});if(!r.ok)throw Error('Missing build identity');return r.json();}")
        validate_metadata(metadata, self.commit)
        self.result['metadata'] = metadata
        if self.page.locator('#learning-dialog[open]').is_visible():
            self.page.locator('#tutorial-decline').click()
        expect(self.page.locator('#learning-dialog')).not_to_be_visible()
        return response


class HostedBrowser:
    def __init__(self, browser, args, result):
        self.browser, self.args, self.result = browser, args, result
        self.origin = urlsplit(args.url)[:2]

    def new_page(self, **options):
        page = self.browser.new_page(**options)

        def request(value):
            parts = urlsplit(value.url)
            if parts.scheme in ('http', 'https') and (value.method != 'GET' or parts[:2] != self.origin or value.post_data_buffer):
                self.result['unexpected_requests'].append({'method': value.method, 'url': value.url})

        page.on('request', request)
        return HostedPage(page, self.args.commit, self.result)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--url', required=True)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    args = parser.parse_args()
    args.capture_live = False
    if urlsplit(args.url).scheme != 'https':
        parser.error('Hosted qualification requires an explicit HTTPS preview URL')
    args.out.mkdir(parents=True, exist_ok=False)
    sources = [args.out / 'first.wav', args.out / 'second.wav']
    hashes = [fixture(sources[0], 440, 8, 1), fixture(sources[1], 880, 6, 2)]
    result = {'scope': 'Published arrangement UI/clock/project/WAV/MP3 with independent saved-PCM checks; no live-PCM or physical listening qualification',
              'cases': [], 'unexpected_requests': [], 'fixture_sha256': hashes}
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(headless=True)
            result['browser'] = f'{args.browser} {browser.version}'
            wrapped = HostedBrowser(browser, args, result)
            try:
                for width in [320, 390]:
                    verify_project(wrapped, args.url, width, args, result, sources[0])
                    verify_mp3(wrapped, args.url, width, args, result, sources)
            finally:
                browser.close()
        assert [hashlib.sha256(path.read_bytes()).hexdigest() for path in sources] == hashes
        assert not result['unexpected_requests'], result['unexpected_requests']
        result['passed'] = True
    finally:
        (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
        print(json.dumps({'passed': result.get('passed', False), 'cases': len(result['cases']),
                          'unexpected_requests': len(result['unexpected_requests'])}))


if __name__ == '__main__':
    main()
