"""Exercise the static browser app and actual AudioWorklet PCM, with no audio uploads."""
import argparse
import base64
import functools
import http.server
import json
import io
import math
from pathlib import Path
import subprocess
import threading
import struct
import wave
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright, expect
from browser_capture import BrowserDiagnostics

ROOT = Path(__file__).resolve().parents[2]


def reference_fixture(frequency):
    buffer = io.BytesIO()
    with wave.open(buffer, 'wb') as writer:
        writer.setnchannels(1)
        writer.setsampwidth(2)
        writer.setframerate(48000)
        writer.writeframes(b''.join(struct.pack('<h',int(3000*math.sin(2*math.pi*frequency*i/48000))) for i in range(96000)))
    return {'name':'reference.wav','mimeType':'audio/wav','buffer':buffer.getvalue()}
parser = argparse.ArgumentParser()
parser.add_argument('--site', type=Path, default=ROOT/'web/dist')
parser.add_argument('--out', type=Path, required=True)
parser.add_argument('--executable')
parser.add_argument('--browser', choices=['chromium','firefox','webkit'], default='chromium')
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def translate_path(self, path):
        if urlparse(path).path == '/tools/browser_audit.js':
            return str(ROOT/'web/tools/browser_audit.js')
        return super().translate_path(path)


server = http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(args.site)))
thread = threading.Thread(target=server.serve_forever,daemon=True)
thread.start()
result = {'errors': [], 'requests': []}
try:
    with sync_playwright() as p:
        browser = getattr(p,args.browser).launch(headless=True, **({'executable_path':args.executable} if args.executable else {}))
        result['browser'] = args.browser+' '+browser.version
        page = browser.new_page(viewport={'width':1480,'height':1000})
        page.add_init_script("""window.dspJobs=[];const post=Worker.prototype.postMessage;Worker.prototype.postMessage=function(message,...args){window.dspJobs.push(message.action);return post.call(this,message,...args);};""")
        diagnostics=BrowserDiagnostics(page,args.browser,result)
        page.on('request',lambda r:result['requests'].append({'method':r.method,'url':r.url,'body_bytes':len(r.post_data_buffer or b'')}))
        try:
            base = f'http://127.0.0.1:{server.server_address[1]}/'
            page.goto(base,wait_until='networkidle')
            expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=30000)
            diagnostics.capture(args.out/'desktop-welcome.png',full_page=True)
            page.locator('#demo').click()
            expect(page.locator('#analyze')).to_be_enabled(timeout=30000)
            expect(page.locator('#export')).to_be_disabled()
            page.locator('#analyze').click()
            expect(page.locator('#measurements')).to_be_visible(timeout=120000)
            expect(page.locator('#master')).to_be_enabled(timeout=120000)
            page.locator('#master').click()
            expect(page.locator('#export')).to_be_enabled(timeout=240000)
            diagnostics.capture(args.out/'desktop-master.png',full_page=True)
            page.locator('#tab-eq').click()
            page.locator('#play').click()
            expect(page.locator('#play')).to_have_text('Ⅱ Pause',timeout=30000)
            baseline=page.evaluate('()=>window.dspJobs.length')
            page.locator('[data-band="2"][data-parameter="gain"]').evaluate("async el=>{for(let i=0;i<20;i++){el.value=-i/6;el.dispatchEvent(new Event('input',{bubbles:true}));await new Promise(r=>setTimeout(r,35));}el.value=-3;el.dispatchEvent(new Event('input',{bubbles:true}));}")
            page.wait_for_timeout(1200)
            assert not any(action in ['master','export'] for action in page.evaluate('n=>window.dspJobs.slice(n)',baseline))
            expect(page.locator('#play')).to_have_text('Ⅱ Pause')
            diagnostics.capture(args.out/'desktop-live-eq.png',full_page=True)
            assert 'EQ live' in page.locator('#live-state').inner_text()
            assert page.locator('#clock').inner_text() != '0:00 / 0:00'
            page.locator('#stop').click()
            with page.expect_download(timeout=180000) as download:
                page.locator('#export').click()
            download.value.save_as(str(args.out/'example-web-master.wav'))
            page.locator('#tab-report').click()
            with page.expect_download() as download:
                page.locator('#save-report').click()
            download.value.save_as(str(args.out/'example-web-master.report.json'))
            # Listening controls must have no effect on the deterministic export.
            page.locator('#volume').fill('0.1')
            page.locator('#volume').dispatch_event('input')
            page.locator('#match').uncheck()
            with page.expect_download(timeout=180000) as download:
                page.locator('#export').click()
            download.value.save_as(str(args.out/'example-low-volume.wav'))
            assert (args.out/'example-web-master.wav').read_bytes()==(args.out/'example-low-volume.wav').read_bytes()
            result['listening_volume_does_not_change_export'] = True
            page.locator('#profile').select_option('edm')
            expect(page.locator('#export')).to_be_disabled()
            page.locator('#profile').select_option('neutral')
            expect(page.locator('#export')).to_be_enabled()
            page.set_viewport_size({'width':390,'height':844})
            page.locator('#tab-eq').click()
            page.evaluate('()=>window.scrollTo(0,0)')
            page.wait_for_timeout(250)
            diagnostics.capture(args.out/'phone-eq-viewport.png')
            diagnostics.capture(args.out/'phone-eq.png',full_page=True)
            assert not page.evaluate('()=>document.documentElement.scrollWidth>innerWidth')
            page.locator('.plot-wrap').evaluate("el=>{const r=el.getBoundingClientRect(),dock=document.querySelector('.playback').getBoundingClientRect().height;window.scrollTo(0,scrollY+r.top-(innerHeight-dock-r.height)/2);}")
            diagnostics.capture(args.out/'phone-eq-graph.png')
            page.locator('#band-picker button').nth(2).click()
            page.locator('.band.active').scroll_into_view_if_needed()
            diagnostics.capture(args.out/'phone-eq-controls.png')
            player_box=page.locator('.playback').bounding_box()
            assert player_box['y']>=0 and player_box['y']+player_box['height']<=845
            page.locator('#tab-overview').click()
            page.evaluate('()=>window.scrollTo(0,0)')
            diagnostics.capture(args.out/'phone-master.png',full_page=True)
            page.locator('#output-section').evaluate("el=>{const r=el.getBoundingClientRect(),dock=document.querySelector('.playback').getBoundingClientRect().height;window.scrollTo(0,scrollY+r.top-(innerHeight-dock-r.height)/2);}")
            diagnostics.capture(args.out/'phone-master-results.png')
            page.locator('.statusbar').scroll_into_view_if_needed()
            diagnostics.capture(args.out/'phone-settings-bottom.png')
            # Actual shared DSP and AudioWorklet rendering: no mocked audio API.
            page.add_script_tag(type='module',url=base+'tools/browser_audit.js')
            page.wait_for_function('()=>typeof window.runAudit === "function"')
            result['dsp'] = page.evaluate('()=>window.runAudit()')
            for index in range(page.evaluate('()=>window.auditArtifacts.length')):
                artifact=page.evaluate('i=>{const a=window.auditArtifacts[i];let t="";for(let j=0;j<a.bytes.length;j+=8192)t+=String.fromCharCode(...a.bytes.subarray(j,j+8192));return {name:a.name,bytes:btoa(t)};}',index)
                (args.out/artifact['name']).write_bytes(base64.b64decode(artifact['bytes']))
            page.set_viewport_size({'width':1480,'height':1000})
            page.locator('#reference-file').set_input_files(reference_fixture(220))
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            expect(page.locator('#export')).to_be_disabled()
            page.locator('#master').click()
            expect(page.locator('#export')).to_be_enabled(timeout=240000)
            page.locator('#reference-file').set_input_files(reference_fixture(880))
            expect(page.locator('#master')).to_be_enabled(timeout=30000)
            expect(page.locator('#export')).to_be_disabled()
            expect(page.locator('#session-state')).to_contain_text('Direction changed')
            result['replacing_reference_with_same_filename_invalidates_master']=True
            page.evaluate('()=>{document.querySelector("#master").click();document.querySelector("#cancel").click();}')
            expect(page.locator('#status')).to_contain_text('Cancelled. Your original is ready',timeout=30000)
            expect(page.locator('#master')).to_be_enabled()
            expect(page.locator('#export')).to_be_disabled()
            page.context.set_offline(True)
            page.locator('#analyze').click()
            expect(page.locator('#analyze')).to_be_enabled(timeout=120000)
            expect(page.locator('#status')).to_contain_text('Analysis complete')
            page.context.set_offline(False)
            result['cancellation_restores_original_and_rejects_stale_master']=True
            result['analysis_works_with_network_offline_after_load']=True
            page.locator('#file').set_input_files({'name':'invalid.wav','mimeType':'audio/wav','buffer':b'not an audio file'})
            expect(page.locator('#status')).to_have_class('status-error',timeout=30000)
            expect(page.locator('#analyze')).to_be_enabled()
            result['invalid_file_reports_error_and_preserves_session']=True
            for path in args.out.glob('*.wav'):
                check=subprocess.run(['ffmpeg','-hide_banner','-nostats','-i',str(path),'-filter_complex','ebur128=peak=true','-f','null','-'],capture_output=True,text=True,check=True)
                (args.out/(path.stem+'.ffmpeg.txt')).write_text(check.stderr,encoding='utf-8')
            assert all(r['method']=='GET' and r['body_bytes']==0 and urlparse(r['url']).hostname=='127.0.0.1' for r in result['requests'])
            assert not result['errors'],result['errors']
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
    (args.out/'browser-verification.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='requests'},indent=2))
if not result.get('success'):
    raise SystemExit(1)
