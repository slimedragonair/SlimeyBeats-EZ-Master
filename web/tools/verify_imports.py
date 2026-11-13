"""Exercise real long-file decoding and import, separately from DSP benchmarks."""
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

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--site',type=Path,default=ROOT/'web/dist')
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--browser',choices=['chromium','firefox','webkit'],default='chromium')
parser.add_argument('--executable')
args=parser.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
fixtures=(args.out/'.import-fixtures').resolve()
assert fixtures.is_relative_to(args.out.resolve())
fixtures.mkdir(exist_ok=True)


def create_fixture(name,seconds,rate=44100,channels=2):
    wav=fixtures/(name+'.wav')
    mp3=fixtures/(name+'.mp3')
    # Vary the level like music: a constant full-length sine forces the offline
    # reconstructed-peak detector through its worst case at every frame. That is
    # a separate performance case, not this file-import integration contract.
    signal=f'aevalsrc=0.1*sin(2*PI*220*t)*exp(-12*mod(t\\,0.5))+0.004*sin(2*PI*1700*t):s={rate}:d={seconds}'
    subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-f','lavfi','-i',signal,
        '-ac',str(channels),'-c:a','pcm_s16le',str(wav),'-ac',str(channels),'-c:a','libmp3lame','-b:a','64k',str(mp3)],check=True)
    return wav,mp3


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*unused):
        pass


server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(args.site)))
thread=threading.Thread(target=server.serve_forever,daemon=True)
thread.start()
result={'errors':[],'imports':[],'requests':[]}
try:
    six=create_fixture('six-minute-stereo',360)
    fifteen=create_fixture('fifteen-minute-stereo',900)
    over=create_fixture('over-limit',901,8000,1)[0]
    short=create_fixture('short',2)[0]
    with sync_playwright() as p:
        browser=getattr(p,args.browser).launch(headless=True,**({'executable_path':args.executable} if args.executable else {}))
        result['browser']=args.browser+' '+browser.version
        page=browser.new_page(viewport={'width':390,'height':844})
        diagnostics=BrowserDiagnostics(page,args.browser,result)
        page.on('request',lambda r:result['requests'].append({'method':r.method,'url':r.url,'body_bytes':len(r.post_data_buffer or b'')}))
        try:
            page.goto(f'http://127.0.0.1:{server.server_address[1]}/',wait_until='networkidle')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=30000)
            expect(page.locator('#import-limits')).to_contain_text('15:00')
            diagnostics.capture(args.out/'phone-import-limits.png')
            with page.expect_file_chooser() as chooser:page.locator('#add').click()
            chooser.value.set_files(str(over))
            expect(page.locator('#import-error')).to_be_visible(timeout=120000)
            expect(page.locator('#import-error')).to_contain_text('15:00')
            box=page.locator('#import-error').bounding_box()
            assert box['y']>=0 and box['y']+box['height']<page.locator('.playback').bounding_box()['y']
            diagnostics.capture(args.out/'phone-duration-error.png')
            result['over_limit_error_visible_near_import_controls']=True
            for fixture in [*six,*fifteen]:
                with page.expect_file_chooser() as chooser:page.locator('#add').click()
                chooser.value.set_files(str(fixture))
                expect(page.locator('#filename')).to_have_text(fixture.name,timeout=180000)
                expect(page.locator('#analyze')).to_be_enabled(timeout=180000)
                expect(page.locator('#import-error')).to_be_hidden()
                metadata=page.locator('#filemeta').inner_text()
                assert ('15:00' if 'fifteen' in fixture.name else '6:00') in metadata,metadata
                result['imports'].append({'file':fixture.name,'source_bytes':fixture.stat().st_size,'metadata':metadata})
                print(json.dumps(result['imports'][-1]),flush=True)
                page.locator('#clear').click()
                expect(page.locator('#track-card')).to_be_hidden()
            with page.expect_file_chooser() as chooser:page.locator('#add').click()
            chooser.value.set_files(str(short))
            expect(page.locator('#filename')).to_have_text(short.name,timeout=30000)
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            started=time.monotonic()
            page.locator('#reference-file').set_input_files(str(fifteen[1]))
            expect(page.locator('#reference-name')).to_contain_text(fifteen[1].name,timeout=600000)
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            result['fifteen_minute_reference_import']=True
            result['reference_measurement_seconds']=time.monotonic()-started
            assert not result['errors'],result['errors']
            assert all(r['method']=='GET' and not r['body_bytes'] and urlparse(r['url']).hostname=='127.0.0.1' for r in result['requests'])
            result['success']=True
        except Exception as error:
            result['success']=False
            result['failure']=str(error)
            result['status']=page.locator('#status').inner_text()
            diagnostics.capture(args.out/'failure.png',full_page=True)
        finally:
            browser.close()
finally:
    server.shutdown()
    server.server_close()
    thread.join()
    # Only generated fixtures inside this run's output directory are removed.
    assert fixtures.is_relative_to(args.out.resolve())
    shutil.rmtree(fixtures)
    (args.out/'long-imports.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='requests'},indent=2))
if not result.get('success'):
    raise SystemExit(1)
