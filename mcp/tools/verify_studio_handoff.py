"""Full import/edit/promote/project/desktop handoff workflow using actual MCP and GUI."""
import argparse,json,math,os,signal,struct,subprocess,tempfile,wave
from pathlib import Path
from protocol_client import Client

def verify(server,desktop,evidence):
    with tempfile.TemporaryDirectory(prefix='Studio handoff with spaces ') as temporary:
        root=Path(temporary);library=root/'Input stems';library.mkdir();output=root/'Output'
        for name,frequency in [('vocals',750),('instrumental',3000)]:
            with wave.open(str(library/(name+'.wav')),'wb') as wav:
                wav.setparams((2,2,16000,0,'NONE',''))
                wav.writeframes(b''.join(struct.pack('<hh',int(math.sin(i/16000*math.tau*frequency)*4000),int(math.sin(i/16000*math.tau*frequency)*4000)) for i in range(32000)))
        client=Client([str(server),'--input-root',str(library),'--output-root',str(output),'--desktop-exe',str(desktop)])
        pid=None
        try:
            imported=client.run('import_stems',{'files':[{'kind':name,'library':0,'path':name+'.wav'} for name in ['vocals','instrumental']]})
            bank=imported['stem_set']['stem_set_id']
            vocal=next(a['audio_id'] for a in imported['assets'] if a['label']=='vocals')
            cleaned=client.run('clean_voice',{'audio_id':vocal,'settings':{'noise_reduction':0,'deess':0}})
            applied=client.run('apply_preview_to_stem',{'stem_set_id':bank,'studio_preview_id':cleaned['studio_preview_id'],'kind':'vocals'})
            current=client.call('get_studio_session');assert current['stem_set']['stem_set_id']!=bank
            bank=current['stem_set']['stem_set_id']
            raw=client.run('export_stems',{'stem_set_id':bank})
            assert (output/raw['folder']/'vocals.wav').is_file()
            preview=current['preview']['studio_preview_id']
            promoted=client.run('use_studio_for_mastering',{'studio_preview_id':preview})
            assert client.call('get_studio_session')['stem_set']['stem_set_id']==bank
            mastered=client.run('master_preview',{'source_id':promoted['source_id'],'options':{'intensity':0.5}})
            assert mastered['preview_id']
            project=client.run('save_studio_project')
            project_path=output/project['project_path'];document=json.loads(project_path.read_text())
            assert len(document['stems'])==2
            launched=client.run('open_studio_in_desktop');assert launched['desktop_launched'];pid=launched['process_id']
            opened=output/launched['project_path']
            os.kill(pid,signal.SIGTERM);pid=None
        finally:
            if pid is not None:os.kill(pid,signal.SIGTERM)
            client.close()
        # After server shutdown and removal of external source stems, the saved
        # project must remain sufficient for the matching desktop to reopen.
        for path in library.glob('*.wav'):path.unlink()
        capture=evidence/'studio-handoff.png'
        result=subprocess.run([str(desktop),'--studio-session',str(opened),'--capture-ui',str(capture)],capture_output=True,text=True,timeout=90)
        assert result.returncode==0,result.stderr
        state=json.loads(capture.with_suffix('.state.json').read_text())
        assert state['error'] is None and state['preview_ready'] and state['playback_ready'],state
        return {'import':True,'stem_edit_apply':True,'raw_stem_export':True,'promote_and_master':True,'bank_retained':True,'self_contained_project':True,'desktop_state':state}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--server',type=Path,required=True);parser.add_argument('--desktop',type=Path,required=True);parser.add_argument('--evidence',type=Path,required=True);args=parser.parse_args();args.evidence.mkdir(parents=True,exist_ok=True)
    print(json.dumps(verify(args.server.resolve(),args.desktop.resolve(),args.evidence.resolve()),indent=2))
