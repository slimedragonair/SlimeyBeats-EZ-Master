"""Qualify existing local work moving to v2 through normal UI actions.

Uses the owned native HTTP fixture. Observers retain hashes and metadata only;
server mode permits local worker init/response and the explicit migration
recovery-snapshot, and rejects every other browser worker operation.
"""
import argparse
from array import array
import hashlib
import json
import math
from pathlib import Path
import shutil
import sys
import time
import wave

from playwright.sync_api import expect, sync_playwright

from remote_browser_fixture import RemoteBrowserFixture
from verify_remote_media import sha, synthetic
from verify_remote_studio import AUDIT, UiRun, pcm, range_value


OBSERVER = r'''(() => {
    const state={calls:[],completed:[],terminated:[],snapshots:[],clears:[],playback:[],digests:[],violations:[]};
    const active=()=>!!document.getElementById('hybrid-enabled')?.checked;
    const digest=async samples=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',samples.buffer)),
        n=>n.toString(16).padStart(2,'0')).join('');
    const hash=(samples,row)=>{const task=digest(samples.slice()).then(value=>row.pcm_sha256=value);state.digests.push(task);};
    const W=Worker;
    globalThis.Worker=class extends W {
        constructor(url,...rest){super(url,...rest);const name=new URL(url,location.href).pathname, pending=new Map();
            const post=this.postMessage.bind(this),terminate=this.terminate.bind(this);
            this.postMessage=(message,...transfer)=>{
                const row={worker:name,action:message.action,server:active()};state.calls.push(row);
                if(active()&&!['init','response','recovery-snapshot'].includes(message.action)){
                    state.violations.push(row);throw Error('Unexpected browser DSP during server migration: '+message.action);
                }
                pending.set(message.id,row);
                if(message.action==='load'&&message.samples){row.geometry={frames:message.samples.length/message.channels,
                    sample_rate:message.rate,channels:message.channels};hash(message.samples,row);}
                if(message.action==='stems'&&message.stems?.length){
                    const sum=new Float32Array(message.stems[0].samples.length);
                    for(let index=0;index<sum.length;index++)sum[index]=message.stems.reduce((n,stem)=>n+stem.samples[index],0);
                    row.origin={};hash(sum,row.origin);
                }
                return post(message,...transfer);
            };
            this.terminate=()=>{state.terminated.push({worker:name,server:active()});return terminate();};
            this.addEventListener('message',({data})=>{
                const row=pending.get(data.id);if(!row||data.progress!==undefined)return;
                pending.delete(data.id);const completed={...row,error:data.error||null};state.completed.push(completed);
                if(row.action==='master'&&data.result?.samples)hash(data.result.samples,completed);
                if(row.action==='recovery-snapshot'&&data.result){const recipe=data.result;
                    state.snapshots.push({server:active(),source:!!recipe.source,edits:recipe.edits,
                        stems:recipe.stems.map(s=>({kind:s.kind,name:s.name,rate:s.audio.rate,channels:s.audio.channels,
                            frames:s.audio.frames,bytes:s.audio.data.size}))});}
            });
        }
    };
    globalThis.__migrationProbe={state,hash,async settle(){await Promise.all(state.digests);},
        async summary(){await Promise.all(state.digests);const {digests,...value}=state;return value;}};
})();'''


def install_playback_observer(page, fixture):
    router=(fixture.site/'hybrid/studio-playback-router.js').read_text(encoding='utf-8')
    import re
    target=re.search(r'from "(\.\./studio-player\.js[^\"]*)"',router)[1]
    page.evaluate('''async target=>{
        const {StudioPlayback}=await import(new URL(target,new URL('hybrid/',location.href)).href);
        const {state,hash}=__migrationProbe,install=StudioPlayback.prototype.setAudio,clear=StudioPlayback.prototype.clear;
        StudioPlayback.prototype.setAudio=function(...args){const value=install.apply(this,args);
            if(this.after){const b=this.after,samples=new Float32Array(b.length*b.numberOfChannels);
                for(let ch=0;ch<b.numberOfChannels;ch++){const input=b.getChannelData(ch);
                    for(let frame=0;frame<b.length;frame++)samples[frame*b.numberOfChannels+ch]=input[frame];}
                const row={frames:b.length,sample_rate:b.sampleRate,channels:b.numberOfChannels};
                state.playback.push(row);hash(samples,row);}
            return value;};
        StudioPlayback.prototype.clear=function(...args){const before={before:!!this.before,after:!!this.after,stems:this.stems?.size||0};
            const value=clear.apply(this,args);state.clears.push({server:!!document.getElementById('hybrid-enabled')?.checked,before,
                released:!this.before&&!this.after&&!this.stems&&!this.settings&&!this.node&&!this.mixer});return value;};
    }''',target)


def observed(page):
    return page.evaluate('__migrationProbe.summary()')


def local_click(page, selector, action):
    start=len(observed(page)['completed'])
    page.locator(selector).click()
    page.wait_for_function('''({start,action})=>__migrationProbe.state.completed.slice(start).some(row=>row.action===action)''',
                           arg={'start':start,'action':action},timeout=120000)
    rows=[row for row in observed(page)['completed'][start:] if row['action']==action]
    assert rows and not rows[-1]['error'],rows


def asset_audio(fixture, audio_id, filename):
    paths=list(fixture.data.rglob('waveform-'+audio_id+'.json'))
    assert len(paths)==1,('Missing private fixture asset',audio_id)
    return paths[0].parent/filename


def pcm_hash(path):
    data,rate,channels=pcm(path)
    return hashlib.sha256(data.tobytes()).hexdigest(),(len(data)//channels,rate,channels)


def workflow(run, files):
    page=run.page
    page.goto(run.fixture.origin,wait_until='networkidle')
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL',timeout=45000)
    if page.locator('#learning-dialog').is_visible():page.locator('#tutorial-decline').click()
    install_playback_observer(page,run.fixture)
    page.locator('#tab-studio-tools').click()
    page.locator('.edit-import').filter(has=page.locator('#s-mode')).evaluate('(e)=>e.open=true')
    page.locator('#s-mode').select_option('two')
    for kind,name in [('vocals','stem_a'),('instrumental','stem_b')]:
        page.locator('#s-stem-files input[data-kind="'+kind+'"]').set_input_files(str(files[name]))
    local_click(page,'#s-import','stems')
    expect(page.locator('#s-export')).to_be_enabled(timeout=20000)
    page.locator('#s-view-timeline').click();page.locator('#s-editor-stem').select_option('vocals')
    page.locator('#s-edit-start').fill('0.5');page.locator('#s-edit-start').press('Tab')
    page.locator('#s-edit-end').fill('1.0');page.locator('#s-edit-end').press('Tab')
    local_click(page,'#s-edit-silence','region')
    expect(page.locator('#s-export')).to_be_enabled(timeout=20000)
    local_saved=observed(page)['playback'][-1]
    assert len(observed(page)['playback'])>=2,'Playback observer did not see local edits'
    page.locator('#s-view-mixer').click()
    fader=page.locator('[aria-label="vocals Gain / dB"]')
    range_value(page,'[aria-label="vocals Gain / dB"]',-6)
    expect(page.locator('#s-quick-mix-row')).to_be_visible()
    expect(page.locator('#s-voice')).to_be_disabled()
    run.passed('local_edited_bank_and_pending_fader',{'saved_preview':local_saved,'pending_vocals_db':-6})
    page.locator('#hybrid-enabled').check()
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]',timeout=20000)
    assert not any(row['method']=='PUT' for row in run.fixture.requests),'Toggle alone uploaded audio'
    page.locator('#s-tab-doctor').click()
    migrated=run.click('#s-doctor-scan','issues')
    expect(page.locator('#s-doctor-status')).to_contain_text('Scanning changed no audio.',timeout=20000)
    state=observed(page)
    snapshots=[row for row in state['snapshots'] if row['server']]
    assert len(snapshots)==1 and not snapshots[0]['source'] and len(snapshots[0]['stems'])==2,snapshots
    assert [edit['action'] for edit in snapshots[0]['edits']]==['region'],snapshots
    commands=[row['operation'] for row in run.events if row['state']=='completed']
    assert commands.index('import_stems')<commands.index('region_edit')<commands.index('issues'),commands
    preview=asset_audio(run.fixture,migrated['preview_id'],'mix.wav')
    actual,geometry=pcm_hash(preview)
    assert actual==local_saved['pcm_sha256'],'Migration did not reproduce the exact committed local preview'
    original=asset_audio(run.fixture,migrated['original_id'],'original.wav')
    root,_,_=pcm(original)
    # Migration preserves existing browser PCM. Browser codecs can normalize
    # integer WAV differently, so compare its actual immutable input samples.
    origin=next(row['origin']['pcm_sha256'] for row in state['calls'] if row['action']=='stems')
    assert hashlib.sha256(root.tobytes()).hexdigest()==origin,'Migration origin is not the exact original local bank sum'
    assert origin!=actual,'Fixture failed to distinguish origin from edited preview'
    expect(fader).to_have_value('-6')
    expect(page.locator('#s-quick-mix-row')).to_be_visible()
    expect(page.locator('#s-voice')).to_be_disabled()
    expect(page.locator('#s-export')).to_be_disabled()
    assert migrated['controls']['tracks'][0]['gain_db']==0,'Pending fader was committed implicitly'
    assert any(row['worker'].endswith('/studio-worker.js') and row['server'] for row in state['terminated']),state
    assert any(row['server'] and row['before']['stems']==2 and row['released'] for row in state['clears']),state['clears']
    run.passed('doctor_migrates_origin_and_recipe_retaining_pending_controls',{
        'snapshot':snapshots[0],'exact_committed_preview_sha256':actual,'geometry':geometry,
        'raw_origin_exact':True,'local_worker_terminated':True,'local_playback_pcm_references_cleared':True,
        'voice_and_export_blocked_until_commit':True})
    page.locator('#s-tab-mix').click()
    committed=run.click('#s-mix','mix')
    assert next(track for track in committed['controls']['tracks'] if track['kind']=='vocals')['gain_db']==-6
    expect(page.locator('#s-quick-mix-row')).to_be_hidden()
    page.locator('#s-tab-voice').click()
    expect(page.locator('#s-voice')).to_be_enabled()
    voiced=run.click('#s-voice','voice_cleanup')
    assert voiced['preview_id']!=committed['preview_id']
    assert not observed(page)['violations']
    run.passed('pending_mix_commits_then_voice_runs_natively',{'preview_changed':True,'vocals_db':-6})
    page.locator('#s-close').click()
    expect(page.locator('#s-info')).to_have_text('No Studio audio loaded',timeout=20000)
    page.locator('#hybrid-enabled').uncheck()
    page.locator('#tab-overview').click()
    page.locator('#file').set_input_files(str(files['source']))
    expect(page.locator('#master')).to_be_enabled(timeout=45000)
    local_click(page,'#master','master')
    expect(page.locator('#export')).to_be_enabled(timeout=45000)
    local=observed(page)
    original_load=[row for row in local['calls'] if row['worker'].endswith('/worker.js') and row['action']=='load'][-1]
    local_master=[row for row in local['completed'] if row['worker'].endswith('/worker.js') and row['action']=='master'][-1]
    assert original_load['pcm_sha256']!=local_master['pcm_sha256'],'Local fixture master did not change PCM'
    page.locator('#hybrid-enabled').check()
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]',timeout=20000)
    remote=run.click('#master','master')
    remote_original=asset_audio(run.fixture,remote['original_id'],'original.wav')
    actual,geometry=pcm_hash(remote_original)
    expected_geometry=original_load['geometry']
    with wave.open(str(files['source']),'rb') as audio:
        source_samples=array('h');source_samples.frombytes(audio.readframes(audio.getnframes()))
    if sys.byteorder!='little':source_samples.byteswap()
    original_native=array('f',(sample/32768 for sample in source_samples))
    assert actual==hashlib.sha256(original_native.tobytes()).hexdigest(),\
        'Server Re-Master did not decode the current original upload exactly'
    assert geometry==(expected_geometry['frames'],expected_geometry['sample_rate'],expected_geometry['channels'])
    master=next(asset for asset in remote['assets'] if asset['id']==remote['master_id'])
    assert (master['frames'],master['sample_rate'],master['channels'])==geometry
    expect(page.locator('#engine-status')).to_have_text('RUST · SERVER')
    run.passed('local_master_to_server_remaster_current_original',{
        'original_file_sha256':sha(files['source']),'native_original_pcm_sha256':actual,
        'prior_local_master_pcm_sha256':local_master['pcm_sha256'],
        'same_original_and_output_geometry':geometry})
    page.locator('#clear').click()
    expect(page.locator('#track-card')).to_be_hidden(timeout=20000)
    run.passed('cleanup')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('site','cli','worker','out'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--ffmpeg',type=Path,default=Path(shutil.which('ffmpeg') or 'ffmpeg'))
    parser.add_argument('--browser',choices=['chromium','firefox','webkit'],default='chromium')
    args=parser.parse_args();args.out=args.out.resolve();args.out.mkdir(parents=True,exist_ok=True)
    result={'success':False,'browser':args.browser,'checks':{},'errors':[],
            'physical_audio_device':'not_run','physical_memory_reclamation':'not_measured'}
    started=time.monotonic();fixture=page=run=None
    try:
        files={name:args.out/(name+'.wav') for name in ('source','stem_a','stem_b')}
        synthetic(files['source'])
        for name,frequency in [('stem_a',280),('stem_b',440)]:
            with wave.open(str(files[name]),'wb') as audio:
                audio.setparams((2,2,48000,0,'NONE','not compressed'))
                samples=array('h',(round(4000*math.sin(2*math.pi*frequency*i/48000)) for i in range(192000) for _ in range(2)))
                if sys.byteorder!='little':samples.byteswap()
                audio.writeframes(samples.tobytes())
        hashes={name:sha(path) for name,path in files.items()}
        with RemoteBrowserFixture(args) as fixture:
            result['fixture']=fixture.provenance
            with sync_playwright() as pw:
                browser=getattr(pw,args.browser).launch(headless=True)
                result['browser_version']=browser.version
                context=browser.new_context(viewport={'width':1440,'height':1050},accept_downloads=True)
                context.add_init_script(AUDIT+'\n'+OBSERVER)
                page=context.new_page();page.set_default_timeout(20000)
                page.on('pageerror',lambda error:result['errors'].append(str(error)))
                page.on('dialog',lambda dialog:dialog.accept())
                run=UiRun(page,fixture,result)
                try:
                    workflow(run,files)
                    result['audit']=page.evaluate('__remoteUiAudit.summary()')
                    result['migration']=observed(page)
                    assert not result['migration']['violations']
                    assert not any(result['audit'][key] for key in ('serverDecodeCalls','workerAudio','blobReads','objectUrls','fullMediaReads','workletErrors','observerErrors'))
                    assert result['audit']['credentials_memory_only'] and not result['errors']
                    assert all(sha(path)==hashes[name] for name,path in files.items())
                    result['success']=True
                except Exception as error:
                    result['failure']=page.evaluate('text=>__remoteUiAudit.redact(text)',type(error).__name__+': '+str(error))
                    result['ui']={key:page.locator('#'+key).inner_text() for key in ('status','s-status','hybrid-availability')}
                    result['audit']=page.evaluate('__remoteUiAudit.summary()');result['migration']=observed(page)
                    page.screenshot(path=str(args.out/'failure.png'),full_page=True)
                finally:
                    result['command_events']=run.events;result['http_requests']=fixture.requests
                    browser.close()
    except Exception as error:
        result.setdefault('failure',type(error).__name__+': '+str(error))
    result['elapsed_seconds']=time.monotonic()-started
    encoded=json.dumps(result,indent=2,allow_nan=False)
    if fixture:encoded=fixture.redact(encoded)
    (args.out/'results.json').write_text(encoded+'\n',encoding='utf-8')
    print(encoded)
    return 0 if result['success'] else 1


if __name__=='__main__':sys.exit(main())
