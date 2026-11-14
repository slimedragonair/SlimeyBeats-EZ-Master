"""Bounded real-UI import/master/live-EQ/export soak; never claims a device limit."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import shutil
import subprocess
import threading
import time
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics
from browser_memory import BrowserMemory, worker_heap

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--site',type=Path,default=ROOT/'web/dist')
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--executable')
parser.add_argument('--tracks',type=int,default=40)
parser.add_argument('--seconds',type=int,default=30)
parser.add_argument('--stop-mib',type=int,default=3072)
args = parser.parse_args()
assert args.tracks > 0 and 0 < args.seconds <= 900
args.out.mkdir(parents=True,exist_ok=True)
fixtures = (args.out/'.memory-fixtures').resolve()
assert fixtures.is_relative_to(args.out.resolve())
fixtures.mkdir(exist_ok=True)
source = fixtures/'source.wav'
subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-f','lavfi','-i',
    f'aevalsrc=0.1*sin(2*PI*80*t)*exp(-12*mod(t\\,0.5))+0.025*sin(2*PI*1700*t):s=48000:d={args.seconds}',
    '-ac','2','-c:a','pcm_s16le',str(source)],check=True)


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*unused):
        pass


server = http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(args.site)))
thread = threading.Thread(target=server.serve_forever,daemon=True)
thread.start()
result = {'errors':[],'cycles':[],'requests':[],'seconds_per_track':args.seconds,
          'requested_tracks':args.tracks,'forced_gc':False,
          'scope':'One active mix replaced without Clear; each cycle masters, plays, adjusts EQ and exports.'}
try:
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=True,**({'executable_path':args.executable} if args.executable else {}))
        result['browser'] = browser.version
        page = browser.new_page(viewport={'width':1480,'height':1000})
        diagnostics = BrowserDiagnostics(page,'chromium',result)
        page.on('request',lambda r:result['requests'].append({'method':r.method,'url':r.url,'body_bytes':len(r.post_data_buffer or b'')}))
        memory = BrowserMemory(browser)
        try:
            page.goto(f'http://127.0.0.1:{server.server_address[1]}/',wait_until='networkidle')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=30000)
            memory.refresh()
            result['initial_private_commit_MiB'] = memory.current()
            for cycle in range(1,args.tracks+1):
                start = time.monotonic()
                track = fixtures/f'track-{cycle:03d}.wav'
                shutil.copyfile(source,track)
                page.locator('#file').set_input_files(str(track))
                expect(page.locator('#filename')).to_have_text(track.name,timeout=180000)
                expect(page.locator('#master')).to_be_enabled(timeout=180000)
                expect(page.locator('#export')).to_be_disabled()
                page.locator('#master').click()
                expect(page.locator('#export')).to_be_enabled(timeout=600000)
                page.locator('#tab-eq').click()
                if page.locator('#playback-details').get_attribute('open') is None:
                    page.locator('#playback-details > summary').click()
                page.locator('#volume').fill('0')
                page.locator('#volume').dispatch_event('input')
                page.locator('#play').click()
                expect(page.locator('#play')).to_have_text('Pause',timeout=30000)
                band = page.locator('[data-band="2"][data-parameter="gain"]')
                band.fill(str(-1.5 if cycle%2 else -2))
                band.dispatch_event('input')
                page.wait_for_timeout(300)
                expect(page.locator('#live-state')).to_contain_text('EQ live')
                page.locator('#stop').click()
                with page.expect_download(timeout=240000) as download:
                    page.locator('#export').click()
                target = args.out/'latest-master.wav'
                download.value.save_as(str(target))
                download.value.delete()  # Release the browser-managed download too.
                assert target.stat().st_size > args.seconds*48000*2*2
                expect(page.locator('#master')).to_be_enabled(timeout=30000)
                memory.refresh()
                entry = {'track':cycle,'cumulative_minutes':cycle*args.seconds/60,
                         'elapsed_seconds':time.monotonic()-start,'wasm_heap_MiB':worker_heap(page),
                         'private_commit_MiB':memory.current()}
                result['cycles'].append(entry)
                print(json.dumps(entry),flush=True)
                track.unlink()
                if entry['private_commit_MiB'] and entry['private_commit_MiB'] > args.stop_mib:
                    raise RuntimeError(f'Test safety budget {args.stop_mib} MiB reached at cycle {cycle}; not a browser memory-limit measurement')
            page.locator('#clear').click()
            expect(page.locator('#track-card')).to_be_hidden()
            page.wait_for_timeout(1000)
            memory.refresh()
            result['after_clear_private_commit_MiB'] = memory.current()
            assert all(r['method']=='GET' and not r['body_bytes'] and urlparse(r['url']).hostname=='127.0.0.1' for r in result['requests'])
            assert not result['errors'],result['errors']
            result['success'] = True
        except Exception as error:
            result['success'] = False
            result['failure'] = str(error)
            result['status'] = page.locator('#status').inner_text()
            diagnostics.capture(args.out/'failure.png',full_page=True)
        finally:
            memory.close()
            result['peak_sampled_private_commit_MiB'] = memory.peak/1048576 if memory.enabled else None
            result['memory_samples'] = memory.samples
            browser.close()
finally:
    server.shutdown()
    server.server_close()
    thread.join()
    assert fixtures.is_relative_to(args.out.resolve())
    shutil.rmtree(fixtures)
    (args.out/'memory-verification.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k not in ['requests','memory_samples','cycles']},indent=2))
if not result.get('success'):
    raise SystemExit(1)
