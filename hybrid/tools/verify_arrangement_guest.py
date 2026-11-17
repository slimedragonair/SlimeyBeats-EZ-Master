"""One ordinary guest completes ZIP editing, delivery, project recovery and mastering."""
import argparse
import array
import copy
import hashlib
import io
import json
import math
from pathlib import Path
import subprocess
import sys
import time
import uuid
import zipfile

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload, command
from hybrid.tests.support import mint_guest
from hybrid.tools.native_fixture import frozen_worker
from verify_arrangement_jobs import fixture


def verify(worker, ffmpeg):
    cli = worker.with_name('slimeybeats-ez-master-cli' + worker.suffix)
    with project_service(cli=cli, studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True,
                         public_enabled=True, v2_memory_bytes=2*1024**3) as (service, owner):
        client, _ = mint_guest(owner)
        principal = service.store.sessions.authenticate(client.token)
        assert not principal.is_owner
        status, _, caps = client.request('GET', '/v2/capabilities')
        assert status == 200 and caps['limits']['guest_jobs_per_session'] == 16
        assert caps['arrangement']['archive_admission_uses_heavy_quota'] is False
        inputs = [fixture(48000, 144000, 440), fixture(48000, 96000, 880)]
        data = io.BytesIO()
        with zipfile.ZipFile(data, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
            for name, audio in zip(('Voice.wav', 'Backing.wav'), inputs):
                archive.writestr('WAV/' + name, audio)
        project = create_project(client)
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': 48000, 'channels': 2})
        assert status == 200, (status, state)
        time.sleep(.3)  # The HTTP creator thread must be allowed to exit.
        private = service.store.projects.projects[project['id']].arrangement
        quota = service.store.sessions._session(principal, time.monotonic())
        started = []

        def job(operation, **fields):
            status, _, value = client.request('POST', path + '/jobs', {'operation': operation, 'epoch': state['epoch'],
                'expected_revision': state['revision'], **fields})
            assert status == 202, (operation, status, value)
            deadline = time.monotonic() + 60
            while value['state'] in ('queued', 'running'):
                assert time.monotonic() < deadline, value
                time.sleep(.025)
                status, _, value = client.request('GET', path + '/jobs/' + value['job_id'])
                assert status == 200, value
            assert value['state'] == 'completed', (value, private.jobs[value['job_id']].failure)
            started.append(operation)
            return value['result']

        def edit(edits):
            status, _, value = client.request('POST', path, {'operation': 'edit', 'epoch': state['epoch'],
                'expected_revision': state['revision'], 'edits': edits})
            assert status == 200, (status, value)
            return value

        def download(artifact):
            status, headers, payload = client.request('GET', f'/v2/projects/{project["id"]}/artifacts/{artifact["id"]}')
            assert status == 200 and len(payload) == artifact['bytes'] and headers['Content-Type'] == artifact['content_type']
            return payload

        uploaded = upload(client, project['id'], data.getvalue(), role='arrangement_archive', filename='Parts.zip')
        review = job('review_archive', upload_id=uploaded)
        state = job('commit_import', review_id=review['review_id'], selection=[{
            'part_id': part['id'], 'version_id': part['default_version_id']} for part in review['parts']])['snapshot']
        time.sleep(.3)  # Promoted actors must survive the completed job thread.
        assert quota.command_starts == 2 and quota.job_starts == 0
        voice = next(lane for lane in state['arrangement']['lanes'] if lane['name'] == 'Voice')
        scope = {'kind': 'clip', 'clip_id': voice['clips'][0]['id']}
        selected_range = {'start_frame': 48000, 'end_frame': 96000}
        proposal = job('suggest_eq', target={'scope': scope, 'range': selected_range})
        assert len(proposal['bands']) == 7 and proposal['notes']
        bands = copy.deepcopy(state['defaults']['bands'])
        bands[-1].update(type='high_shelf', freq=10000, gain=-3, on=True)
        effect = {'id': 'voice-section', 'scope': scope, 'range': selected_range,
                  'bands': bands, 'output_gain_db': 0, 'bypass': False}
        state = edit([{'kind': 'move_clip', 'clip_id': scope['clip_id'], 'lane_id': voice['id'], 'start_frame': 96000},
                      {'kind': 'set_eq', 'effect': effect}])
        delivered = []
        for format in ('wav32f', 'mp3'):
            result = job('export', format=format, sample_rate=0, dither=False)
            payload = download(result['artifact'])
            decoded = subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', 'pipe:0', '-f', 'f32le',
                '-ar', '48000', '-ac', '2', 'pipe:1'], input=payload, stdout=subprocess.PIPE,
                stderr=subprocess.PIPE, timeout=20, check=True).stdout
            pcm = array.array('f'); pcm.frombytes(decoded)
            assert len(pcm) >= 240000*2 and all(math.isfinite(value) for value in pcm)
            delivered.append({'format': format, 'bytes': len(payload)})
        saved = download(job('save_project')['artifact'])
        with zipfile.ZipFile(io.BytesIO(saved)) as archive:
            manifest = json.loads(archive.read('arrangement.json'))
            assert manifest['version'] == 2 and manifest['history']['current'] == state['arrangement']
            for source in manifest['sources']:
                assert hashlib.sha256(archive.read(source['file'])).hexdigest() == source['identity']['sha256']
        # Local browser sources use UUIDs. Load the same canonical schema with
        # those model IDs, preserving every source WAV/hash and history link.
        aliases = {source['id']: str(uuid.uuid4()) for source in manifest['sources']}
        def browser_ids(value):
            if isinstance(value, dict):
                return {key: browser_ids(item) for key, item in value.items()}
            if isinstance(value, list):
                return [browser_ids(item) for item in value]
            return aliases.get(value, value) if isinstance(value, str) else value
        before = browser_ids(state['arrangement'])
        changed = io.BytesIO()
        with zipfile.ZipFile(io.BytesIO(saved)) as source, zipfile.ZipFile(changed, 'w') as target:
            for name in source.namelist():
                target.writestr(name, json.dumps(browser_ids(manifest)) if name == 'arrangement.json' else source.read(name))
        saved_upload = upload(client, project['id'], changed.getvalue(), role='arrangement_project', filename='Saved.zip')
        state = job('open_project', upload_id=saved_upload)['snapshot']
        time.sleep(.3)
        assert state['arrangement'] == before and state['render_options']['max_output_bytes'] <= 512*1024**2
        source_id = state['assets'][0]['id']
        assert source_id in aliases.values()
        status, _, waveform = client.request('POST', path, {'operation': 'waveform', 'epoch': state['epoch'],
            'expected_revision': state['revision'], 'asset_id': source_id, 'range': {'start_frame': 0, 'end_frame': 2048}, 'bins': 32})
        assert status == 200 and waveform, (status, waveform)
        state = edit([{'kind': 'set_lane', 'lane_id': voice['id'], 'gain_db': -2}])
        job('suggest_eq', target={'scope': scope, 'range': selected_range}, exclude_eq_id=effect['id'])
        remaster_input = download(job('export', format='wav32f')['artifact'])
        # Main uses the existing v2 project pipeline in this browser workflow.
        # Its bounded command budget and project cap remain independent of the
        # retained Tracks workspace; no legacy-v1 retention rule is weakened.
        main = create_project(client)
        original = upload(client, main['id'], remaster_input)
        main = command(client, main, 'import', upload_ids=[original])
        assert main['command']['state'] == 'completed', main
        main = command(client, main, 'master', settings={'intensity': .5, 'safety': True})
        assert main['command']['state'] == 'completed', main
        main = command(client, main, 'export', format='wav32f')
        assert main['command']['state'] == 'completed', main
        assert client.request('GET', f'/v2/projects/{main["id"]}/artifacts/{main["command"]["artifact_id"]}')[0] == 200
        status, _, current = client.request('GET', path)
        assert status == 200 and current['arrangement'] == state['arrangement']
        assert quota.job_starts == 7 and quota.command_starts == 5
        assert client.request('DELETE', f'/v2/projects/{main["id"]}')[0] == 200
        assert client.request('DELETE', f'/v2/projects/{project["id"]}')[0] == 200
        return {'passed': True, 'principal': 'guest', 'sessions_minted': 1, 'owner_workflow_requests': 0,
            'heavy_jobs_used': quota.job_starts, 'admission_commands_used': quota.command_starts,
            'heavy_jobs_per_session': caps['limits']['guest_jobs_per_session'], 'delivered': delivered,
            'operations': started + ['main_master'], 'checks': ['ordinary guest ZIP review/atomic import',
                'admission separate from heavy quota', 'explicit SOURCE scoped suggestion/edit', 'actual WAV+MP3 downloads/decode',
                'canonical Save/Open+exact source hashes', 'browser UUID asset IDs/history/waveform preserved',
                'another tweak/suggestion/export', 'same-guest explicit Main remaster of the edited mix',
                'HTTP/job thread exit preserves actors', 'owned cleanup'], 'scope': 'actual guest HTTP/native workflow; browser UI qualified separately'}


def main():
    parser = argparse.ArgumentParser()
    for name in ('worker', 'ffmpeg', 'out'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args(); args.out.mkdir(parents=True, exist_ok=True)
    worker = frozen_worker(args.worker, args.out)
    result = verify(worker, args.ffmpeg.resolve(strict=True)); result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
