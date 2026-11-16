"""Full actual-UI v2 mastering/Studio acceptance, with real native DSP.

Uses synthetic files and an owned backend. The only build adaptation is its
copied default API origin; HTTP requests are streamed without Playwright routing.
Server-mode decodeAudioData/full media buffering are forbidden. Physical audio
devices and maximum-size inputs are outside this functional browser qualification.
"""
import argparse
from array import array
import io
import json
import math
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import time
import wave
import zipfile

from playwright.sync_api import sync_playwright, expect

from remote_browser_fixture import RemoteBrowserFixture
from verify_remote_media import sha, synthetic


AUDIT = r'''(() => {
    const secrets=new Set(),state={decodeCalls:0,serverDecodeCalls:0,blobReads:0,objectUrls:0,
        fullMediaReads:0,workerAudio:[],workletReady:0,workletErrors:0,meters:[],events:[],manifests:{},observerErrors:[]};
    const active=()=>!!document.getElementById('hybrid-enabled')?.checked;
    const send=fetch.bind(globalThis);
    globalThis.fetch=async(input,options)=>{
        const response=await send(input,options);
        const url=new URL(typeof input==='string'||input instanceof URL?input:input.url,location.href);
        if(url.pathname==='/v1/session')response.clone().json().then(v=>{if(v.token)secrets.add(v.token);}).catch(()=>{});
        if(url.pathname.startsWith('/v2/projects')&&response.headers.get('Content-Type')?.startsWith('application/json'))
            response.clone().json().then(value=>{if(value.api_version===2&&Array.isArray(value.assets)){
                state.manifests[value.id]=value;const command=value.command||{};
                state.events.push({project:value.id,revision:value.revision,operation:command.operation,state:command.state,
                    command:command.id,preview_id:value.preview_id,original_id:value.original_id,before_id:value.before_id,
                    preview_is_mix:value.preview_is_mix,can_undo:value.can_undo,can_redo:value.can_redo});
            }}).catch(()=>state.observerErrors.push('Could not observe v2 metadata JSON'));
        return response;
    };
    const A=globalThis.AudioContext||globalThis.webkitAudioContext;
    if(A){const decode=A.prototype.decodeAudioData;A.prototype.decodeAudioData=function(...args){
        ++state.decodeCalls;if(active()){++state.serverDecodeCalls;throw Error('Full PCM decode forbidden in server UI acceptance');}
        return decode.apply(this,args);
    };}
    const blob=Response.prototype.blob,buffer=Response.prototype.arrayBuffer;
    const media=response=>/\/remote-media\/|\/v2\/projects\/[^/]+\/(assets|artifacts)\//.test(response.url);
    Response.prototype.blob=function(){if(active()&&media(this)){++state.blobReads;throw Error('Full media Blob forbidden');}return blob.call(this);};
    Response.prototype.arrayBuffer=function(){if(active()&&media(this)){++state.fullMediaReads;throw Error('Full media buffering forbidden');}return buffer.call(this);};
    const objectURL=URL.createObjectURL;
    URL.createObjectURL=function(value){if(active()&&value instanceof Blob&&/audio|zip/.test(value.type)){
        ++state.objectUrls;throw Error('Server export must use native streamed download');}return objectURL.call(URL,value);};
    if(globalThis.AudioNode){const connect=AudioNode.prototype.connect;
        AudioNode.prototype.connect=function(target,...rest){
            if(target===this.context.destination){const meter=this.context.createAnalyser();meter.fftSize=4096;
                state.meters.push(meter);connect.call(this,meter,...rest);return connect.call(meter,target);}
            return connect.call(this,target,...rest);
        };
    }
    const W=globalThis.Worker;
    globalThis.Worker=class extends W{constructor(...args){super(...args);const post=this.postMessage.bind(this);
        this.postMessage=(message,...rest)=>{if(active()&&(message.samples||message.before||
            ['load','analyze','master','export','voice','doctor','repair','region','mix','stage_stem','commit_stems'].includes(message.action)))
                state.workerAudio.push({action:message.action,pcm:!!message.samples});return post(message,...rest);};}};
    const N=globalThis.AudioWorkletNode;
    if(N)globalThis.AudioWorkletNode=new Proxy(N,{construct(target,args){const node=Reflect.construct(target,args);
        node.port.addEventListener('message',({data})=>{if(data.ready)++state.workletReady;if(data.error)++state.workletErrors;});
        node.addEventListener('processorerror',()=>++state.workletErrors);return node;}});
    const level=()=>Math.max(0,...state.meters.map(meter=>{const v=new Float32Array(meter.fftSize);meter.getFloatTimeDomainData(v);
        return Math.sqrt(v.reduce((sum,n)=>sum+n*n,0)/v.length);}));
    globalThis.__remoteUiAudit={level,events:()=>state.events,manifests:()=>state.manifests,
        summary(){const stored=JSON.stringify({local:{...localStorage},session:{...sessionStorage},cookie:document.cookie});
        const urls=[location.href,...Array.from(document.querySelectorAll('[src],[href]'),e=>e.src||e.href||'')];
        return {decodeCalls:state.decodeCalls,serverDecodeCalls:state.serverDecodeCalls,blobReads:state.blobReads,
            objectUrls:state.objectUrls,fullMediaReads:state.fullMediaReads,workerAudio:state.workerAudio,
            workletReady:state.workletReady,workletErrors:state.workletErrors,observerErrors:state.observerErrors,
            credentials_memory_only:[...secrets].every(s=>!stored.includes(s)&&urls.every(u=>!String(u?.baseVal||u).includes(s))),level:level()};},
        redact(text){for(const secret of secrets)text=text.split(secret).join('[credential removed]');return text;}};
})();'''


def range_value(page, selector, value):
    page.locator(selector).evaluate("(element,value)=>{element.value=String(value);element.dispatchEvent(new Event('input',{bubbles:true}));element.dispatchEvent(new Event('change',{bubbles:true}));}",value)


def pcm(path):
    blob=path.read_bytes()
    assert blob[:4]==b"RIFF" and blob[8:12]==b"WAVE"
    offset=12
    while offset+8<=len(blob):
        tag,size=struct.unpack_from("<4sI",blob,offset);start=offset+8
        if tag==b"fmt ":
            encoding,channels,rate,_,_,bits=struct.unpack_from("<HHIIHH",blob,start)
            if encoding==65534:encoding=struct.unpack_from("<I",blob,start+24)[0]
        if tag==b"data":
            assert encoding==3 and bits==32
            data=array('f');data.frombytes(blob[start:start+size])
            if sys.byteorder!='little':data.byteswap()
            return data,rate,channels
        offset=start+size+size%2
    raise AssertionError('Missing float audio')


class UiRun:
    def __init__(self,page,fixture,result):
        self.page,self.fixture,self.result=page,fixture,result
        self.requests=[]

    @property
    def events(self):
        return self.page.evaluate('__remoteUiAudit.events()')

    @property
    def manifests(self):
        return self.page.evaluate('__remoteUiAudit.manifests()')

    def complete(self,operation,start):
        deadline=time.monotonic()+120
        while time.monotonic()<deadline:
            for event in self.events[start:]:
                if event['operation']==operation and event['state'] in ('completed','failed','cancelled'):
                    assert event['state']=='completed',event
                    return self.manifests[event['project']]
            error=self.page.evaluate("document.querySelector('#import-error:not([hidden])')?.textContent || document.querySelector('#s-status.error')?.textContent || ''")
            if error:
                raise AssertionError('Application rejected '+operation+': '+error)
            self.page.wait_for_timeout(50)
        raise TimeoutError('No completed actual UI command: '+operation)

    def click(self,selector,operation):
        start=len(self.events);self.page.locator(selector).click();return self.complete(operation,start)

    def passed(self,name,details=True):
        self.result['checks'][name]={'passed':True,'details':details}
        print('PASS '+name,flush=True)

    def level(self):
        return self.page.evaluate('__remoteUiAudit.level()')

    def download(self,selector,name,operation):
        start=len(self.events)
        with self.page.expect_download(timeout=120000) as event:
            self.page.locator(selector).click()
        manifest=self.complete(operation,start)
        download=event.value;target=self.fixture.out/name;download.save_as(str(target))
        assert download.failure() is None
        assert any(file.stat().st_size==target.stat().st_size and sha(file)==sha(target)
                   for file in self.fixture.data.rglob('*') if file.is_file()),'Download hash differs from server artifact'
        return target,manifest


def workflow(run,args,files):
    page=run.page
    page.goto(run.fixture.origin,wait_until='networkidle')
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=45000)
    if page.locator('#learning-dialog').is_visible():page.locator('#tutorial-decline').click()
    page.locator('#hybrid-enabled').check()
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]',timeout=20000)
    start=len(run.events);page.locator('#file').set_input_files(str(files['clipped']))
    run.complete('import',start)
    clipped=run.click('#analyze','analyze')
    assert any(issue.get('key')=='clipping' and issue.get('severity',0)>=20 for issue in clipped['analysis']['issues'])
    expect(page.locator('#insight')).to_contain_text('clipping',ignore_case=True,timeout=15000)
    run.passed('normal_analyze_clipping_warning',{'insight':page.locator('#insight').inner_text()})
    page.locator('#clear').click();expect(page.locator('#track-card')).to_be_hidden(timeout=20000)
    start=len(run.events);page.locator('#file').set_input_files(str(files['source']))
    main=run.complete('import',start)
    expect(page.locator('#filename')).to_have_text(files['source'].name,timeout=15000)
    run.passed('server_import',{'frames':main['assets'][0]['frames'],'project':main['id']})
    run.click('#analyze','analyze');main=run.click('#master','master')
    expect(page.locator('#export')).to_be_enabled(timeout=15000)
    run.passed('analyze_master',{'master_id':main['master_id'],'report_analysis_keys':list(main['report']['output'])})
    page.locator('#playback-details').evaluate('(e)=>e.open=true')
    page.locator('#match').uncheck()
    page.locator('#play').click();expect(page.locator('#play')).to_have_text('Pause',timeout=20000)
    page.wait_for_timeout(300);before=max(run.level() for _ in range(3));assert before>1e-4
    page.locator('#tab-eq').click();range_value(page,'#eq-output',-6);page.wait_for_timeout(450)
    after=run.level();assert .4<after/before<.65,(before,after)
    range_value(page,'#eq-output',0);page.wait_for_timeout(300)
    page.locator('#stop').click();page.wait_for_timeout(350);assert run.level()<1e-7
    run.passed('actual_main_stream_live_eq',{'before_rms':before,'cut_rms':after,'ratio':after/before,'stopped_rms':run.level()})
    for fmt,suffix in [('wav24','wav'),('mp3','mp3')]:
        page.locator('#format').select_option(fmt)
        target,_=run.download('#export','main-export.'+suffix,'export')
        if suffix=='wav':
            with wave.open(str(target),'rb') as audio:assert (audio.getnframes(),audio.getframerate(),audio.getnchannels())==(384000,48000,2)
        else:
            subprocess.run([str(args.ffmpeg),'-v','error','-i',str(target),'-f','null','-'],check=True,capture_output=True,timeout=30)
        run.passed('main_'+suffix+'_stream_export',{'sha256':sha(target),'bytes':target.stat().st_size})
    page.locator('#tab-report').click()
    archive,_=run.download('#save-session','main-session.zip','bundle')
    with zipfile.ZipFile(archive) as bundle:
        assert any(name.endswith(('.wav','.mp3')) for name in bundle.namelist())
        assert 'settings.json' in bundle.namelist()
    run.passed('main_session_zip',{'bytes':archive.stat().st_size})
    page.locator('#tab-studio-tools').click()
    studio=run.click('#s-use-main','clone')
    expect(page.locator('#s-info')).to_contain_text('Mastering mix',timeout=15000)
    run.passed('studio_use_main',{'project':studio['id'],'main_original_preserved':main['original_id']})
    page.locator('#s-tab-voice').click();studio=run.click('#s-voice','voice_cleanup')
    expect(page.locator('#s-export')).to_be_enabled(timeout=15000)
    run.passed('studio_voice',{'before_id':studio.get('before_id'),'preview_id':studio['preview_id']})
    page.locator('#s-tab-doctor').click();studio=run.click('#s-doctor-scan','issues')
    assert isinstance(studio.get('issues',{}).get('issues'),list)
    run.passed('studio_doctor_scan',{'issues':len(studio['issues']['issues'])})
    page.locator('#s-tab-repair').click();studio=run.click('#s-spectrum','spectrogram')
    assert len(studio['spectrogram']['levels_dbfs'])==studio['spectrogram']['columns']*studio['spectrogram']['rows']
    run.passed('studio_spectrogram',{'columns':studio['spectrogram']['columns'],'rows':studio['spectrogram']['rows']})
    page.locator('#s-tab-mix').click()
    page.locator('.edit-import').filter(has=page.locator('#s-mode')).evaluate('(e)=>e.open=true')
    page.locator('#s-mode').select_option('two')
    page.locator('#s-stem-files input[data-kind="vocals"]').set_input_files(str(files['stem_a']))
    page.locator('#s-stem-files input[data-kind="instrumental"]').set_input_files(str(files['stem_b']))
    previous_bank_root=studio['original_id']
    studio=run.click('#s-import','import_stems')
    assert studio['original_id']!=previous_bank_root,'Importing a new bank retained the previous Studio A/original root'
    assert len(studio['stems'])==2 and studio['preview_is_mix']
    assert studio['preview_id'] not in {s['id'] for s in studio['stems']}
    with wave.open(str(files['stem_a']),'rb') as a,wave.open(str(files['stem_b']),'rb') as b:
        a_samples=array('h');a_samples.frombytes(a.readframes(a.getnframes()))
        b_samples=array('h');b_samples.frombytes(b.readframes(b.getnframes()))
    if sys.byteorder!='little':a_samples.byteswap();b_samples.byteswap()
    expected_mix=array('f',((a+b)/32768 for a,b in zip(a_samples,b_samples)))
    root_waveform=list(run.fixture.data.rglob('waveform-'+studio['original_id']+'.json'))
    assert len(root_waveform)==1
    actual_root,_,_=pcm(root_waveform[0].parent/'original.wav')
    assert actual_root==expected_mix,'New bank A/original is not its exact raw sum'
    run.passed('studio_two_stem_import',{'stems':[s['kind'] for s in studio['stems']],
        'replaced_prior_A_root':True,'A_exact_full_bank_sum':True})
    before=studio['preview_id']
    page.locator('#s-view-timeline').click();page.locator('#s-editor-stem').select_option('vocals')
    page.locator('#s-edit-start').fill('0.5');page.locator('#s-edit-start').press('Tab')
    page.locator('#s-edit-end').fill('1.0');page.locator('#s-edit-end').press('Tab')
    studio=run.click('#s-edit-silence','region_edit');edited=studio['preview_id']
    assert edited!=before and studio['preview_is_mix']
    assert studio['preview_id'] not in {s['id'] for s in studio['stems']}
    run.passed('studio_region_full_mix',{'before':before,'after':edited,'ab_before':studio.get('before_id')})
    studio=run.click('#s-undo','history');assert studio['preview_id']==before
    studio=run.click('#s-redo','history');assert studio['preview_id']==edited
    run.passed('studio_undo_redo',{'restored_preview':studio['preview_id']})
    page.locator('#s-format').select_option('wav32f')
    target,studio=run.download('#s-export','studio-full-mix.wav','export')
    samples,rate,channels=pcm(target);assert (len(samples)//channels,rate,channels)==(384000,48000,2)
    # Vocals and accompaniment have different tones. During the silenced vocal
    # selection the accompaniment remains; the whole mix must never become solo.
    middle=samples[24000*2:48000*2]
    assert math.sqrt(sum(x*x for x in middle)/len(middle))>.02
    expected_edit=array('f',((b if 24000<=index//2<48000 else a+b)/32768
                            for index,(a,b) in enumerate(zip(a_samples,b_samples))))
    assert samples==expected_edit,'Committed region export differs from the exact complete bank mix'
    run.passed('studio_full_mix_export',{'sha256':sha(target),'selection_rms':math.sqrt(sum(x*x for x in middle)/len(middle)),
        'full_pcm_exact_to_edited_bank':True})
    archive,_=run.download('#s-export-session','studio-session.zip','bundle')
    with zipfile.ZipFile(archive) as bundle:
        assert len([name for name in bundle.namelist() if name.startswith('stems/') and name.endswith('.wav')])==2
    run.passed('studio_session_zip',{'bytes':archive.stat().st_size})
    page.locator('#s-view-mixer').click()
    page.locator('#s-play').click();expect(page.locator('#s-play')).to_have_text('Pause',timeout=15000)
    page.wait_for_timeout(350);level_before=run.level();assert level_before>1e-4
    page.get_by_role('checkbox',name='vocals mute',exact=True).check()
    page.get_by_role('checkbox',name='instrumental mute',exact=True).check()
    page.wait_for_timeout(350);level_muted=run.level();assert level_muted<1e-7
    page.get_by_role('checkbox',name='vocals mute',exact=True).uncheck()
    page.get_by_role('checkbox',name='instrumental mute',exact=True).uncheck()
    page.wait_for_timeout(300);assert run.level()>1e-4
    page.locator('#s-stop').click();page.wait_for_timeout(350)
    studio=run.click('#s-mix','mix')
    run.passed('studio_live_stem_controls',{'before_rms':level_before,'both_muted_rms':level_muted})
    start=len(run.fixture.requests);page.locator('#s-close').click()
    expect(page.locator('#s-info')).to_have_text('No Studio audio loaded',timeout=20000)
    page.locator('#clear').click();expect(page.locator('#track-card')).to_be_hidden(timeout=20000)
    assert len([r for r in run.fixture.requests[start:] if r['method']=='DELETE' and r['status']==200])>=2
    run.passed('cleanup',{'deleted_projects':2})


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--site',type=Path,required=True)
    parser.add_argument('--cli',type=Path,required=True)
    parser.add_argument('--worker',type=Path,required=True)
    parser.add_argument('--ffmpeg',type=Path,default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    parser.add_argument('--browser',choices=['chromium','firefox','webkit'],default='chromium')
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();args.out=args.out.resolve();args.out.mkdir(parents=True,exist_ok=True)
    result={'success':False,'browser':args.browser,'checks':{},'errors':[],
            'physical_device_audio':'not_run','large_file_acceptance':'not_run'}
    started=time.monotonic();page=fixture=run=None
    try:
        files={name:args.out/(name+'.wav') for name in ('source','stem_a','stem_b','clipped')}
        for path in files.values():synthetic(path)
        # Distinct parts make loss of accompaniment detectable in the region export.
        for name,frequency in [('stem_a',280),('stem_b',440)]:
            with wave.open(str(files[name]),'wb') as audio:
                audio.setparams((2,2,48000,0,'NONE','not compressed'))
                for offset in range(0,384000,48000):
                    values=[round(4000*math.sin(2*math.pi*frequency*(offset+i)/48000)) for i in range(48000) for _ in range(2)]
                    audio.writeframesraw(struct.pack('<'+'h'*len(values),*values))
        clipped=array('f',(max(-1.,min(1.,1.8*math.sin(2*math.pi*440*i/48000))) for i in range(96000) for _ in range(2)))
        if sys.byteorder!='little':clipped.byteswap()
        payload=clipped.tobytes()
        files['clipped'].write_bytes(struct.pack('<4sI4s4sIHHIIHH4sI',b'RIFF',36+len(payload),b'WAVE',b'fmt ',16,
            3,2,48000,48000*8,8,32,b'data',len(payload))+payload)
        digests={name:sha(path) for name,path in files.items()}
        with RemoteBrowserFixture(args) as fixture:
            result['fixture']=fixture.provenance
            with sync_playwright() as pw:
                browser=getattr(pw,args.browser).launch(headless=True)
                result['browser_version']=browser.version
                context=browser.new_context(viewport={'width':1440,'height':1000},accept_downloads=True)
                context.add_init_script(AUDIT)
                page=context.new_page();page.set_default_timeout(15000)
                page.on('pageerror',lambda error:result['errors'].append(str(error)))
                page.on('dialog',lambda dialog:dialog.accept())
                run=UiRun(page,fixture,result)
                try:
                    workflow(run,args,files)
                    result['audit']=page.evaluate('__remoteUiAudit.summary()')
                    assert result['audit']['serverDecodeCalls']==0 and not result['audit']['workerAudio'],result['audit']
                    assert not any(result['audit'][key] for key in ('blobReads','objectUrls','fullMediaReads','workletErrors'))
                    assert result['audit']['credentials_memory_only'] and result['audit']['workletReady']>0 and not result['audit']['observerErrors']
                    assert not result['errors'],result['errors']
                    assert all(sha(path)==digests[name] for name,path in files.items())
                    result['success']=True
                except Exception as error:
                    result['failure']=page.evaluate('text=>__remoteUiAudit.redact(text)',str(error))
                    result['ui']={key:page.locator('#'+key).inner_text() for key in ('status','s-status','hybrid-availability')}
                    result['audit']=page.evaluate('__remoteUiAudit.summary()')
                    page.screenshot(path=str(args.out/'failure.png'),full_page=True)
                finally:
                    result['command_events']=run.events
                    result['http_requests']=fixture.requests
                    browser.close()
    except Exception as error:
        result.setdefault('failure',type(error).__name__+': '+str(error))
    finally:
        result['elapsed_seconds']=time.monotonic()-started
        encoded=json.dumps(result,indent=2,allow_nan=False)
        if fixture:encoded=fixture.redact(encoded)
        (args.out/'results.json').write_text(encoded+'\n',encoding='utf-8')
        print(encoded)
    return 0 if result['success'] else 1


if __name__=='__main__':
    sys.exit(main())
