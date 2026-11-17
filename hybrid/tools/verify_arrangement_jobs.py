"""Actual async HTTP/native jobs, six-format delivery and atomic project restore."""
import argparse
import array
import hashlib
import io
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import threading
import time
from unittest.mock import patch
import zipfile
import wave

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest
from hybrid.server.arrangement_native import ArrangementNative
from hybrid.tools.native_fixture import frozen_worker


def fixture(rate,frames,frequency):
    data=io.BytesIO()
    with wave.open(data,'wb') as writer:
        writer.setparams((1,2,rate,0,'NONE','not compressed'))
        values=[round(3000*math.sin(i*math.tau*frequency/rate)) for i in range(frames)]
        writer.writeframes(struct.pack('<'+'h'*frames,*values))
    return data.getvalue()


def verify(worker,ffmpeg):
    with project_service(studio_cli=worker,ffmpeg=ffmpeg,v2_arrangement_enabled=True,public_enabled=True,
                         v2_memory_bytes=2*1024**3) as (service,client):
        project=create_project(client); path=f'/v2/projects/{project["id"]}/arrangement'
        inputs=[fixture(48000,144000,220),fixture(48000,96000,880)]
        ids=[upload(client,project['id'],value,filename=f'Take{i}.wav') for i,value in enumerate(inputs)]
        status,_,state=client.request('POST',path,{'operation':'create','sample_rate':48000,'channels':2})
        assert status==200,(status,state)
        status,_,state=client.request('POST',path,{'operation':'add_uploads','epoch':state['epoch'],'expected_revision':0,'upload_ids':ids,'names':['Voice','Backing']})
        assert status==200,(status,state)
        voice=state['arrangement']['lanes'][0]; clip=voice['clips'][0]
        effect={'id':'voice-section','scope':{'kind':'clip','clip_id':clip['id']},'range':{'start_frame':48000,'end_frame':96000},
                'bands':state['defaults']['bands'],'output_gain_db':-6,'bypass':False}
        status,_,state=client.request('POST',path,{'operation':'edit','epoch':state['epoch'],'expected_revision':state['revision'],
            'edits':[{'kind':'move_clip','clip_id':clip['id'],'lane_id':voice['id'],'start_frame':48000},{'kind':'set_eq','effect':effect}]})
        assert status==200,(status,state)
        private=service.store.projects.projects[project['id']]; primary=private.arrangement.native
        native_pid=primary.process.pid; original_doc=json.dumps(state['arrangement'],sort_keys=True)

        def submit(operation,**fields):
            status,_,job=client.request('POST',path+'/jobs',{'operation':operation,'epoch':state['epoch'],'expected_revision':state['revision'],**fields})
            assert status==202,(status,job)
            return job

        def finish(job,expected='completed'):
            deadline=time.monotonic()+40
            while job['state'] in ('queued','running'):
                assert time.monotonic()<deadline,job
                time.sleep(.025)
                status,_,job=client.request('GET',path+'/jobs/'+job['job_id'])
                assert status==200,(status,job)
            detail=private.arrangement.jobs[job['job_id']].failure
            assert job['state']==expected,(job,detail)
            return job.get('result')

        proposed=finish(submit('suggest_eq',target={'scope':effect['scope'],'range':effect['range']},exclude_eq_id=effect['id'],profile='neutral'))
        assert len(proposed['bands'])==7 and proposed['notes'] and 'high_frequency' in proposed
        status,_,_=client.request('POST',path+'/jobs',{'operation':'suggest_eq','epoch':state['epoch'],'expected_revision':state['revision'],
            'target':{'scope':{'kind':'mix'},'range':None},'exclude_eq_id':effect['id']})
        assert status==400
        formats=[]
        for format in ('wav16','wav24','wav32f','flac16','flac24','mp3'):
            result=finish(submit('export',format=format,sample_rate=0,dither=False))
            artifact=result['artifact']
            status,headers,payload=client.request('GET',f'/v2/projects/{project["id"]}/artifacts/{artifact["id"]}')
            assert status==200 and len(payload)==artifact['bytes'] and headers['Content-Type']==artifact['content_type']
            decoded=subprocess.run([str(ffmpeg),'-nostdin','-v','error','-i','pipe:0','-f','f32le','-ar','48000','-ac','2','pipe:1'],input=payload,
                                   stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True,timeout=20).stdout
            assert len(decoded)>=192000*2*4
            pcm=array.array('f');pcm.frombytes(decoded)
            assert all(math.isfinite(value) for value in pcm)
            formats.append({'format':format,'bytes':len(payload)})
        saved=finish(submit('save_project'))
        status,_,archive_bytes=client.request('GET',f'/v2/projects/{project["id"]}/artifacts/{saved["artifact"]["id"]}')
        assert status==200
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
            manifest=json.loads(archive.read('arrangement.json'))
            assert manifest['format']=='slimeybeats-studio' and manifest['version']==2
            assert manifest['history']['current']==state['arrangement']
            for source in manifest['sources']:
                assert hashlib.sha256(archive.read(source['file'])).hexdigest()==source['identity']['sha256']
        # Cancellation is deliberately held after spawning the real disposable
        # actor, so this exercises process reaping rather than a queued no-op.
        entered,release=threading.Event(),threading.Event(); shadows=[]
        def delayed(config,directory):
            actor=ArrangementNative(config,directory);shadows.append(actor);entered.set()
            assert release.wait(10)
            return actor
        with patch('hybrid.server.arrangement_job_execution.ArrangementNative',delayed):
            job=submit('export',format='wav32f')
            assert entered.wait(10)
            status,_,_=client.request('POST',path+'/jobs/'+job['job_id']+'/cancel',{})
            assert status==200
            release.set();finish(job,'cancelled')
        assert shadows[0].process.poll() is not None and primary.process.pid==native_pid and primary.process.poll() is None
        status,_,preserved=client.request('GET',path)
        assert status==200 and json.dumps(preserved['arrangement'],sort_keys=True)==original_doc
        assert private.arrangement.active_job is None and service.store.arrangement_active is None
        bad_id=upload(client,project['id'],b'not a project zip',role='arrangement_project',filename='Bad.zip')
        finish(submit('open_project',upload_id=bad_id),'failed')
        assert private.arrangement.native is primary
        status,_,preserved=client.request('GET',path)
        assert status==200 and json.dumps(preserved['arrangement'],sort_keys=True)==original_doc
        project_upload=upload(client,project['id'],archive_bytes,role='arrangement_project',filename='Saved.zip')
        opened=finish(submit('open_project',upload_id=project_upload))['snapshot']
        assert opened['epoch']!=state['epoch'] and opened['revision']>state['revision']
        assert json.dumps(opened['arrangement'],sort_keys=True)==original_doc and primary.process.poll() is not None
        assert opened['render_options']['max_output_bytes']<=512*1024**2
        state=opened
        status,_,undone=client.request('POST',path,{'operation':'history','epoch':state['epoch'],'expected_revision':state['revision'],'redo':False})
        assert status==200 and not undone['arrangement']['eq']
        guest,_=mint_guest(client)
        assert guest.request('GET',path+'/jobs/'+job['job_id'])[0]==404
        status,_,_=client.request('DELETE',f'/v2/projects/{project["id"]}')
        assert status==200
        return {'passed':True,'formats':formats,'checks':['async immediate202/status poll','explicit scoped suggestion/exclusion guard',
            'six native encoders and independent decode','canonical v2 ZIP+source checksums','cancel reaps shadow and preserves primary/history',
            'bad project preserves primary','atomic project adoption and restored Undo','restored output budget bounded','foreign job refused','cleanup releases worker lease']}


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--worker',type=Path,required=True);parser.add_argument('--ffmpeg',type=Path,required=True);parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
    worker=frozen_worker(args.worker,args.out)
    result=verify(worker,args.ffmpeg.resolve(strict=True))
    result['native_fixture']=str(worker.parent)
    (args.out/'results.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result,indent=2))


if __name__=='__main__':main()
