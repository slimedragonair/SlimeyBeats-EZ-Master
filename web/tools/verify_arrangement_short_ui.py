"""Actual Server On UI short-preview playback and automatic EOF."""
import argparse
import json
from pathlib import Path
import shutil
from playwright.sync_api import expect, sync_playwright
from browser_audio_environment import browser_audio_environment
from arrangement_remote_diagnostics import ArrangementRemoteDiagnostics
from remote_browser_fixture import RemoteBrowserFixture
from verify_arrangement_browser import fixture


def main():
    parser = argparse.ArgumentParser()
    for name in ['site', 'out', 'cli', 'worker', 'ffmpeg']: parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True); args.arrangement = True
    result = {'scope': 'Actual Server On short-source controls and media EOF', 'errors': []}
    source = args.out / 'short.wav'; fixture(source, 440, .12, 1)
    try:
        with RemoteBrowserFixture(args) as owned, browser_audio_environment(args.out) as audio, sync_playwright() as p:
            result.update(fixture=owned.provenance, requests=owned.requests, audio_environment=audio)
            shutil.copy2(Path(__file__).with_name('studio_capture_processor.js'), owned.site / '__arrangement_capture.js')
            browser = p.webkit.launch(headless=True)
            try:
                page = browser.new_page(); diag = ArrangementRemoteDiagnostics(page, 'webkit', result)
                for name in ['arrangement_audio_audit.js', 'arrangement_remote_audit.js']: page.add_init_script(path=str(Path(__file__).with_name(name)))
                page.goto(owned.origin, wait_until='networkidle')
                page.locator('#tab-studio-tools').click(); page.locator('#hybrid-enabled').check()
                expect(page.locator('#hybrid-availability')).to_have_text('[Server online]', timeout=30000)
                page.locator('#s-arrange-entry').click()
                a = lambda name: page.locator(f'#s-arrangement-server-page [data-arr="{name}"]')
                expect(a('workspace')).to_be_visible(timeout=30000); expect(a('add-files')).to_be_enabled(timeout=30000)
                a('files').set_input_files(str(source)); expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(1)
                a('import-commit').click()
                expect(page.locator('#s-play')).to_be_enabled(timeout=30000)
                page.locator('#s-play').click()
                page.wait_for_function('()=>arrangementRemoteAudit.mediaEvents.some(value=>value.event==="ended")', timeout=10000)
                expect(page.locator('#s-play')).to_have_text('Play')
                result['media'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
                result['events'] = page.evaluate('()=>arrangementRemoteAudit.mediaEvents')
                assert not result['errors'], result['errors']; result['passed'] = True
                page.evaluate('()=>arrangementRemoteAudit.cleanup()')
            finally:
                if 'page' in locals():
                    result['media'] = page.evaluate('()=>arrangementRemoteAudit.mediaState()')
                    result['events'] = page.evaluate('()=>arrangementRemoteAudit.mediaEvents')
                browser.close()
    finally: (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__': main()
