"""Regress feedback writes during native ResizeObserver delivery, without capture."""
import argparse
import functools
import hashlib
import http.server
import json
from pathlib import Path
import threading
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]
FIXTURE_PATH = '/__job_feedback_resize.html'
FIXTURE = b'''<!doctype html><meta charset="utf-8"><title>Feedback resize regression</title>
<section id="job-feedback">
  <div id="job-feedback-master"><button id="job-cancel-master">Cancel main</button><progress id="job-master-progress"></progress></div>
  <div id="job-feedback-studio"><span id="job-studio-status"></span><button id="job-cancel-studio">Cancel Studio</button></div>
</section>
<div><div><div id="resize-trigger">Native resize trigger</div></div></div>
<div id="job" hidden></div><button id="cancel" disabled>Main handler</button>
<button id="s-cancel">Studio handler</button><progress id="progress" value="0.4" max="1"></progress>
<p id="s-status">Preparing</p>'''


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def do_GET(self):
        if urlparse(self.path).path != FIXTURE_PATH:
            return super().do_GET()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(FIXTURE)))
        self.end_headers()
        self.wfile.write(FIXTURE)


def run(args):
    args.out.mkdir(parents=True, exist_ok=True)
    source = args.site / 'job-feedback.js'
    result = {'browser': args.browser, 'errors': [], 'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest()}
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(args.site)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with sync_playwright() as playwright:
            browser = getattr(playwright, args.browser).launch(headless=True)
            try:
                result['browser'] += ' ' + browser.version
                page = browser.new_page(viewport={'width': 320, 'height': 740})
                BrowserDiagnostics(page, args.browser, result, interface_profile='fresh')
                page.goto(f'http://127.0.0.1:{server.server_port}' + FIXTURE_PATH, wait_until='domcontentloaded')
                page.evaluate('''async () => {
                    const { wireJobFeedback } = await import('/job-feedback.js');
                    const qa = window.qa = { busy: false, triggered: false, heights: [], mainCancels: 0, studioCancels: 0 };
                    qa.nodes = [...document.querySelectorAll('[id]')];
                    document.getElementById('cancel').onclick = () => ++qa.mainCancels;
                    document.getElementById('s-cancel').onclick = () => ++qa.studioCancels;
                    qa.feedback = wireJobFeedback(() => qa.busy);
                    qa.shallow = new ResizeObserver(entries => qa.heights.push(entries[0].contentRect.height));
                    qa.deep = new ResizeObserver(() => {
                        if (qa.triggered) return;
                        qa.triggered = true; qa.busy = true;
                        document.getElementById('s-status').textContent = 'Opening arrangement';
                        qa.feedback.refresh(); qa.feedback.refresh();
                    });
                    // A deeper notification can advance async workspace state
                    // after the shallower overlay was already measured.
                    qa.shallow.observe(document.getElementById('job-feedback'));
                    qa.deep.observe(document.getElementById('resize-trigger'));
                }''')
                page.wait_for_function("qa.triggered && !document.getElementById('job-feedback').hidden && document.getElementById('job-studio-status').textContent === 'Opening arrangement'")
                page.evaluate('() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))')
                result['native_observed_heights'] = page.evaluate('qa.heights')
                assert any(height == 0 for height in result['native_observed_heights'])
                assert any(height > 0 for height in result['native_observed_heights'])
                assert not result['errors'], result['errors']
                page.evaluate('qa.shallow.disconnect(); qa.deep.disconnect()')
                page.locator('#job-cancel-studio').click()
                assert page.evaluate('qa.studioCancels') == 1

                page.evaluate('''() => {
                    qa.busy = false; qa.feedback.refresh();
                    qa.busy = true; document.getElementById('s-status').textContent = 'Newest state'; qa.feedback.refresh();
                }''')
                page.wait_for_function("!document.getElementById('job-feedback-studio').hidden && document.getElementById('job-studio-status').textContent === 'Newest state'")
                page.evaluate('''() => {
                    qa.busy = false; document.getElementById('job').hidden = false;
                    document.getElementById('cancel').disabled = false;
                    document.getElementById('progress').value = 0.7; qa.feedback.refresh();
                }''')
                page.wait_for_function("!document.getElementById('job-feedback-master').hidden && !document.getElementById('job-cancel-master').disabled && document.getElementById('job-master-progress').value === 0.7")
                page.locator('#job-cancel-master').click()
                assert page.evaluate('qa.mainCancels') == 1
                page.evaluate("document.getElementById('job').hidden = true; qa.feedback.refresh()")
                page.wait_for_function("document.getElementById('job-feedback').hidden")
                assert page.evaluate('qa.nodes.every(node => document.getElementById(node.id) === node)')
                assert not result['errors'], result['errors']
                result['success'] = True
            finally:
                browser.close()
    except Exception as error:
        result.update(success=False, failure=str(error))
        raise
    finally:
        server.shutdown()
        server.server_close()
        thread.join()
        (args.out / 'job-feedback-resize.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site', type=Path, default=ROOT / 'web/site')
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='webkit')
    run(parser.parse_args())
