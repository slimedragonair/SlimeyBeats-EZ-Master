"""Actual HTTP/auth/metadata/native actor qualification; no compressed-preview claim."""
import argparse
import io
import json
import math
from pathlib import Path
import struct
import sys
import wave

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload, command
from hybrid.tests.support import mint_guest
from hybrid.server.project_retention import retain_history_assets


def fixture(frames):
    data = io.BytesIO()
    with wave.open(data, 'wb') as writer:
        writer.setparams((1, 2, 8000, 0, 'NONE', 'not compressed'))
        values = [round(math.sin(i * math.tau * 220 / 8000) * 3276) for i in range(frames)]
        writer.writeframes(struct.pack('<' + 'h' * frames, *values))
    return data.getvalue()


def verify(worker, ffmpeg):
    with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True,
                         public_enabled=True, v2_max_decoded_bytes=16 * 1024**2,
                         v2_memory_bytes=2 * 1024**3) as (service, client):
        project = create_project(client)
        original_ids = []
        for frames in (16000, 12000):
            entry = upload(client, project['id'], fixture(frames), filename='Repeated.wav')
            project = command(client, project, 'import', upload_ids=[entry])
            assert project['command']['state'] == 'completed', project['command']
            original_ids.append(project['original_id'])
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': 8000, 'channels': 2})
        assert status == 200, (status, state)
        epoch = state['epoch']
        status, _, state = client.request('POST', path, {'operation': 'add_assets', 'epoch': epoch, 'expected_revision': 0, 'audio_ids': original_ids})
        assert status == 200 and state['revision'] == 1, (status, state)
        assert sorted(asset['frames'] for asset in state['assets']) == [12000, 16000]
        assert len(state['arrangement']['lanes']) == 2 and state['capabilities']['playback'] is True
        private = service.store.projects.projects[project['id']]
        actor = private.arrangement.native
        pid = actor.process.pid
        first, other = state['arrangement']['lanes']
        status, _, changed = client.request('POST', path, {'operation': 'edit', 'epoch': epoch, 'expected_revision': 1,
            'edits': [{'kind': 'set_lane', 'lane_id': first['id'], 'gain_db': -6}, {'kind': 'set_lane', 'lane_id': other['id'], 'mute': True}]})
        assert status == 200 and changed['revision'] == 2, (status, changed)
        assert actor.process.pid == pid and not (private.directory / 'arrangement-output').exists()
        actor.call({'operation': 'prepare', 'expected_revision': 2, 'side': 'after', 'frame': 0})
        block = actor.call({'operation': 'block', 'frames': 2048})
        gain = 10 ** (-6 / 20)
        expected = [round(math.sin(i * math.tau * 220 / 8000) * 3276) / 32768 * gain for i in range(2048)]
        assert max(abs(block['samples'][i * 2] - expected[i]) for i in range(2048)) < 1e-6
        for request, code in [
            ({'operation': 'edit', 'epoch': epoch, 'expected_revision': 1, 'edits': []}, 409),
            ({'operation': 'edit', 'epoch': epoch, 'expected_revision': 2, 'edits': [{'kind': 'unknown'}]}, 400),
            ({'operation': 'edit', 'epoch': epoch, 'expected_revision': 2, 'edits': [], 'path': '../private.wav'}, 400),
        ]:
            status, _, _ = client.request('POST', path, request)
            assert status == code
        status, _, current = client.request('GET', path)
        assert status == 200 and current['revision'] == 2
        foreign, _ = mint_guest(client)
        status, _, _ = foreign.request('GET', path)
        assert status == 404
        with service.store.lock:
            retain_history_assets(private)
            assert set(original_ids) <= set(private.assets)
        status, _, result = client.request('DELETE', f'/v2/projects/{project["id"]}')
        assert status == 200 and result['deleted'] and actor.process.poll() is not None
        return {'passed': True, 'scope': 'authenticated metadata HTTP and actual persistent native PCM; compressed preview still pending',
                'checks': ['real imported source IDs', 'different durations and repeated labels', 'metadata-only batch/no render',
                    'same native process after edit', 'HTTP gain changes actual native frame PCM', 'stale/schema/path preservation',
                    'foreign principal rejected', 'owned source retention', 'delete reaps actor before storage cleanup']}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    from hybrid.tools.native_fixture import frozen_worker
    worker = frozen_worker(args.worker, args.out)
    result = verify(worker, args.ffmpeg.resolve(strict=True))
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
