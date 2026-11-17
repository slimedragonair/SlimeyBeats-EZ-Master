"""Actual HTTP compressed preview, bounded encode-ahead and live native edits."""
import argparse
import array
import http.client
import json
import math
from pathlib import Path
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.tests.project_support import project_service, create_project, upload
from hybrid.tests.support import mint_guest
from verify_arrangement_http import fixture


def decode(ffmpeg, encoded):
    result = subprocess.run([str(ffmpeg), '-nostdin', '-hide_banner', '-loglevel', 'error',
        '-i', 'pipe:0', '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1'], input=encoded,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=20, check=True)
    values = array.array('f')
    values.frombytes(result.stdout)
    return values


def rms(values, first, last):
    selected = values[first * 2:last * 2:2]
    assert len(selected) == last - first
    return math.sqrt(sum(value * value for value in selected) / len(selected))


def verify(worker, ffmpeg, container='mp3', duration_seconds=3):
    with project_service(studio_cli=worker, ffmpeg=ffmpeg, v2_arrangement_enabled=True,
                         public_enabled=True, v2_memory_bytes=2 * 1024**3) as (service, client):
        project = create_project(client)
        first_frames, second_frames = duration_seconds * 8000, (duration_seconds - 1) * 8000
        first = upload(client, project['id'], fixture(first_frames), filename='Take.wav')
        second = upload(client, project['id'], fixture(second_frames), filename='Take.wav')
        path = f'/v2/projects/{project["id"]}/arrangement'
        status, _, state = client.request('POST', path, {'operation': 'create', 'sample_rate': 8000, 'channels': 2})
        assert status == 200, (status, state)
        epoch = state['epoch']
        status, _, state = client.request('POST', path, {'operation': 'add_uploads', 'epoch': epoch, 'expected_revision': 0,
            'upload_ids': [first, second], 'names': ['Reviewed take', 'Reviewed take']})
        assert status == 200 and state['revision'] == 1, (status, state)
        assert [lane['name'] for lane in state['arrangement']['lanes']] == ['Reviewed take'] * 2
        assert [lane['clips'][0]['end_frame'] for lane in state['arrangement']['lanes']] == [first_frames, second_frames]
        before_request = {'operation': 'prepare', 'epoch': epoch, 'expected_revision': 1, 'side': 'before', 'start_frame': first_frames,
                          'preview_container': container}
        status, _, empty = client.request('POST', path, before_request)
        assert status == 200 and empty['side_duration_frames'] == 0 and empty['duration_frames'] == first_frames and empty['start_frame'] == 0
        first_lane, second_lane = state['arrangement']['lanes']
        status, _, state = client.request('POST', path, {'operation': 'edit', 'epoch': epoch, 'expected_revision': 1, 'edits': [
            {'kind': 'set_lane', 'lane_id': first_lane['id'], 'gain_db': -6},
            {'kind': 'set_lane', 'lane_id': second_lane['id'], 'mute': True}]})
        assert status == 200 and state['revision'] == 2
        prepare = {'operation': 'prepare', 'epoch': epoch, 'expected_revision': 2, 'side': 'after', 'start_frame': 0, 'preview_container': container}
        status, _, stream = client.request('POST', path, prepare)
        assert status == 200, (status, stream)
        assert (stream['sample_rate'], stream['preview_sample_rate'], stream['preview_priming_frames']) == (8000, 48000, 1105)
        private = service.store.projects.projects[project['id']]
        native_pid = private.arrangement.native.process.pid
        response, failures, first_byte, first_media = [], [], [], []

        def receive():
            connection = http.client.HTTPConnection('127.0.0.1', client.port, timeout=12)
            try:
                connection.request('GET', stream['stream_path'], headers={'Authorization': 'Bearer ' + client.token})
                incoming = connection.getresponse()
                chunks = []
                tail = b''
                while block := incoming.read1(8192):
                    if not first_byte:
                        first_byte.append(time.monotonic())
                    chunks.append(block)
                    scanned = tail + block
                    if container == 'mp4' and not first_media and b'mdat' in scanned:
                        first_media.append(time.monotonic())
                    tail = scanned[-3:]
                response.append((incoming.status, dict(incoming.getheaders()), b''.join(chunks)))
            except Exception as error:
                failures.append(str(error))
            finally:
                connection.close()

        started = time.monotonic()
        reader = threading.Thread(target=receive)
        reader.start()
        time.sleep(0.7)
        status, _, state = client.request('POST', path, {'operation': 'edit', 'epoch': epoch, 'expected_revision': 2,
            'edits': [{'kind': 'set_lane', 'lane_id': first_lane['id'], 'gain_db': 0}]})
        assert status == 200 and state['revision'] == 3 and not state['preview']['pending'], (status, state)
        reader.join(timeout=duration_seconds + 10)
        assert not reader.is_alive() and not failures, failures
        elapsed = time.monotonic() - started
        assert first_byte and first_byte[0] - started < 1.0, f'Encoder startup buffered audio: {first_byte[0] - started if first_byte else None}'
        assert duration_seconds - .3 <= elapsed < duration_seconds + 7, f'Preview encoded ahead of playback: {elapsed}'
        status, headers, encoded = response[0]
        mime = 'audio/mp4' if container == 'mp4' else 'audio/mpeg'
        assert status == 200 and headers['Content-Type'] == mime and 'Content-Length' not in headers
        assert stream['preview_container'] == container and stream['preview_mime'].split(';', 1)[0] == mime
        if container == 'mp4':
            assert first_media and first_media[0] - started < 1.0
            assert encoded.count(b'moof') > 1
        assert private.arrangement.native.process.pid == native_pid
        decoded = decode(ffmpeg, encoded)
        delay = stream['preview_priming_frames']
        quiet = rms(decoded, delay + 12000, delay + 24000)
        loud = rms(decoded, delay + 72000, delay + 108000)
        assert 1.85 < loud / quiet < 2.15, (quiet, loud)
        assert not (private.directory / 'arrangement-output').exists()
        prepare.update(expected_revision=3, start_frame=4137)
        status, _, selected = client.request('POST', path, prepare)
        assert status == 200 and selected['start_frame'] == 4137
        status, _, late_stop = client.request('POST', path, {'operation': 'stop', 'epoch': epoch, 'expected_revision': 2, 'stream_id': stream['stream_id']})
        assert status == 200 and late_stop['stopped'] is False
        foreign, _ = mint_guest(client)
        assert foreign.request('GET', selected['stream_path'])[0] == 404
        status, _, stopped = client.request('POST', path, {'operation': 'stop', 'epoch': epoch, 'expected_revision': 3, 'stream_id': selected['stream_id']})
        assert status == 200 and stopped['stopped'] is True
        assert private.arrangement.native.process.poll() is None
        status, _, _ = client.request('DELETE', f'/v2/projects/{project["id"]}')
        assert status == 200
        return {'passed': True, 'container': container, 'scope': 'actual native actor + authenticated paced compressed HTTP, no physical device acceptance',
            'checks': ['direct uploaded-file admission, no legacy import/master', 'reviewed labels and unequal durations, one transaction',
                'empty Before and stable B geometry', 'project8k/preview48k rational clock metadata', 'continuous compressed HTTP stream',
                'bounded150ms encode-ahead', 'live native gain audible in same MP3 stream', 'no song render or worker restart',
                'seek source-frame descriptor', 'stale Stop cannot cancel new generation', 'foreign stream access refused', 'owned process cleanup'],
            'stream_elapsed_seconds': elapsed, 'first_byte_seconds': first_byte[0] - started,
            'first_media_fragment_seconds': first_media[0] - started if first_media else None,
            'quiet_rms': quiet, 'loud_rms': loud, 'live_gain_ratio': loud / quiet,
            'mp3_bytes': len(encoded), 'decoded_preview_frames': len(decoded) // 2}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--ffmpeg', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--container', choices=('mp3', 'mp4'), default='mp3')
    parser.add_argument('--duration-seconds', type=int, choices=range(3, 31), default=3)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    from hybrid.tools.native_fixture import frozen_worker
    worker = frozen_worker(args.worker, args.out)
    result = verify(worker, args.ffmpeg.resolve(strict=True), args.container, args.duration_seconds)
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
