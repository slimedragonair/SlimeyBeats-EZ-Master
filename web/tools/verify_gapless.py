"""Cross-check actual browser MP3 timing against independently decoded FFmpeg PCM."""
import argparse
import functools
import http.server
import json
from pathlib import Path
import shutil
import subprocess
import threading
from urllib.parse import urlparse
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--browser',choices=['chromium','firefox','webkit'],default='chromium')
parser.add_argument('--executable')
parser.add_argument('--out',type=Path,required=True)
args = parser.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
fixtures = (args.out/'.gapless-fixtures').resolve()
assert fixtures.is_relative_to(args.out.resolve())
fixtures.mkdir(exist_ok=True)
for rate in [44100,48000]:
    for channels in [1,2]:
        stem = f'{rate}-{channels}'
        subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-f','lavfi','-i',
            f'anoisesrc=amplitude=0.1:seed=1234:sample_rate={rate}:duration=3',
            '-af','lowpass=f=6000:p=2','-ac',str(channels),'-c:a','libmp3lame','-b:a','64k',str(fixtures/(stem+'.mp3'))],check=True)
        subprocess.run(['ffmpeg','-v','error','-nostdin','-y','-i',str(fixtures/(stem+'.mp3')),
            '-ar','48000','-ac',str(channels),'-f','f32le',str(fixtures/(stem+'.f32'))],check=True)


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self,*unused):
        pass

    def translate_path(self,path):
        name = urlparse(path).path
        if name == '/mp3-gapless.js':
            return str(ROOT/'web/site/audio/mp3-gapless.js')
        return super().translate_path(path)


server = http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(Handler,directory=str(fixtures)))
thread = threading.Thread(target=server.serve_forever,daemon=True)
thread.start()
result = {'errors':[]}
try:
    with sync_playwright() as p:
        browser = getattr(p,args.browser).launch(headless=True,**({'executable_path':args.executable} if args.executable else {}))
        result['browser'] = args.browser+' '+browser.version
        try:
            page = browser.new_page()
            page.on('pageerror',lambda error:result['errors'].append(str(error)))
            page.goto(f'http://127.0.0.1:{server.server_address[1]}/')
            result['cases'] = page.evaluate('''async()=>{
                const {readMp3Timing,normalizeMp3Timing}=await import('./mp3-gapless.js');
                const ctx=new AudioContext({sampleRate:48000}),cases=[];
                try{
                    for(const rate of [44100,48000])for(const channels of [1,2]){
                        const stem=`${rate}-${channels}`;
                        const encoded=await(await fetch(stem+'.mp3')).arrayBuffer();
                        const timing=readMp3Timing(encoded);
                        if(!timing)throw Error('Missing fixture timing');
                        const raw=await ctx.decodeAudioData(encoded);
                        const corrected=normalizeMp3Timing(ctx,raw,timing,900);
                        const reference=new Float32Array(await(await fetch(stem+'.f32')).arrayBuffer());
                        if(corrected.length!==144000||corrected.numberOfChannels!==channels||reference.length!==144000*channels)
                            throw Error('Gapless duration/layout mismatch');
                        const alignment=[];
                        for(let ch=0;ch<channels;ch++){
                            const actual=corrected.getChannelData(ch);
                            if(!actual.every(Number.isFinite))throw Error('Nonfinite decoded audio');
                            const candidates=[];
                            for(let lag=-4;lag<=4;lag++){
                                let aa=0,bb=0,ab=0;
                                for(let i=4096;i<8192;i++){
                                    const a=actual[i],b=reference[(i+lag)*channels+ch];
                                    aa+=a*a;bb+=b*b;ab+=a*b;
                                }
                                candidates.push({lag,correlation:ab/Math.sqrt(aa*bb)});
                            }
                            const best=candidates.reduce((a,b)=>a.correlation>b.correlation?a:b);
                            if(Math.abs(best.lag)>(rate===48000?0:1)||best.correlation<.99)
                                throw Error('Unexpected MP3 alignment: '+JSON.stringify({rate,channels,ch,best}));
                            alignment.push(best);
                        }
                        cases.push({rate,channels,raw_frames:raw.length,normalized_frames:corrected.length,
                            already_gapless:raw===corrected,alignment});
                    }
                    return cases;
                }finally{await ctx.close();}
            }''')
            assert not result['errors'],result['errors']
            result['success'] = True
        except Exception as error:
            result['success'] = False
            result['failure'] = str(error)
        finally:
            browser.close()
finally:
    server.shutdown();server.server_close();thread.join()
    assert fixtures.is_relative_to(args.out.resolve())
    shutil.rmtree(fixtures)
    (args.out/'gapless-verification.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps(result,indent=2))
if not result.get('success'):
    raise SystemExit(1)
