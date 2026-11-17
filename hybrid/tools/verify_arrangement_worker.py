"""Qualify the actual persistent native arrangement actor through its supervisor."""
import argparse
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
from types import SimpleNamespace
import wave

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.server.arrangement_native import ArrangementNative, ArrangementCommandError


def verify(worker, directory):
    rate, frames = 48000, 48000
    source = directory / 'source.wav'
    values = [round(math.sin(i * math.tau * 220 / rate) * 3276) for i in range(frames)]
    with wave.open(str(source), 'wb') as writer:
        writer.setparams((1, 2, rate, 0, 'NONE', 'not compressed'))
        writer.writeframes(struct.pack('<' + 'h' * frames, *values))
    config = SimpleNamespace(studio_cli=worker, memory_bytes=2 * 1024**3, cpu_seconds=120,
                             max_job_disk_bytes=64 * 1024**2, worker_threads=2, worker_timeout_seconds=10)
    actor = ArrangementNative(config, directory)
    try:
        state = actor.call({'operation': 'create', 'sample_rate': rate, 'channels': 2, 'epoch': 'server-epoch',
            'limits': {'max_pcm_bytes': 8 * 1024**2, 'max_bank_pcm_bytes': 16 * 1024**2,
                       'max_duration_seconds': 60, 'decode_timeout_seconds': 5, 'waveform_bins': 128}})
        assert state['revision'] == 0
        state = actor.call({'operation': 'add_assets', 'expected_revision': 0,
            'assets': [{'id': 'source-id', 'name': 'Source', 'file': str(source)}],
            'edits': [{'kind': 'add_lane', 'lane': {'id': 'lane-id', 'name': 'Source', 'clips': [
                {'id': 'clip-id', 'source_id': 'source-id', 'start_frame': 0, 'source_start_frame': 0, 'end_frame': frames}]}}]})
        assert state['revision'] == 1
        after = actor.call({'operation': 'prepare', 'expected_revision': 1, 'side': 'after', 'frame': 0})
        assert after['frames'] == frames
        before = actor.call({'operation': 'prepare', 'expected_revision': 1, 'side': 'before', 'frame': 0})
        assert before['frames'] == 0
        actor.call({'operation': 'prepare', 'expected_revision': 1, 'side': 'after', 'frame': 0})
        block = actor.call({'operation': 'block', 'frames': 2048})
        assert block['frames'] == 2048 and block['channels'] == 2
        assert max(abs(block['samples'][i * 2] - values[i] / 32768) for i in range(2048)) < 1e-7
        original = actor.call({'operation': 'project_manifest', 'expected_revision': 1})['sources']
        state = actor.call({'operation': 'edit', 'expected_revision': 1,
                            'edits': [{'kind': 'set_lane', 'lane_id': 'lane-id', 'gain_db': -6}]})
        assert state['revision'] == 2 and state['preview']['prepared_revision'] == 2
        position = actor.call({'operation': 'playback_info'})['position_frame']
        assert position == 2048
        actor.call({'operation': 'block', 'frames': 2048})
        changed = actor.call({'operation': 'block', 'frames': 2048})
        gain = 10 ** (-6 / 20)
        assert max(abs(changed['samples'][i * 2] - values[4096 + i] / 32768 * gain) for i in range(2048)) < 1e-6
        immutable = actor.call({'operation': 'project_manifest', 'expected_revision': 2})['sources']
        assert immutable == original
        for invalid in [
            {'operation': 'edit', 'expected_revision': 0, 'edits': []},
            {'operation': 'edit', 'expected_revision': 2, 'edits': [{'kind': 'set_lane', 'lane_id': 'lane-id', 'gain_db': 99}]},
            {'operation': 'edit', 'expected_revision': 2, 'edits': [{'kind': 'unknown'}]},
            {'operation': 'open_project', 'expected_revision': 2, 'file': str(directory.parent / 'outside.json'), 'epoch': 'escape'},
        ]:
            try:
                actor.call(invalid)
                raise AssertionError('Invalid or stale command was accepted')
            except ArrangementCommandError:
                pass
            assert actor.call({'operation': 'read'})['revision'] == 2
        exported = actor.call({'operation': 'export', 'expected_revision': 2, 'output_id': 'a' * 32,
                               'format': 'wav32f', 'sample_rate': 0, 'dither': False})
        native_output = Path(exported['exports'][0]['path'])
        # Rust canonical Windows paths may carry the extended-length prefix.
        # Resolve only the expected generated member inside our owned output.
        output = directory / 'arrangement-output' / ('a' * 32) / native_output.name
        assert output.is_file() and output.is_relative_to(directory) and output.samefile(native_output)
        payload = output.read_bytes()
        assert b'data' in payload and len(payload) >= frames * 8 + 44
        return {'passed': True, 'checks': ['actual owned supervisor/native process', 'persistent immutable sources',
            'metadata admission', 'bounded actual stereo frame blocks', 'mono-to-stereo unchanged PCM',
            'live gain without render/reprepare', 'empty Before geometry', 'stale/invalid schema/range/path preservation',
            'unchanged project source checksums', 'explicit native Float32 export']}
    finally:
        actor.close()
        assert actor.process.poll() is not None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    from hybrid.tools.native_fixture import frozen_worker
    worker = frozen_worker(args.worker, args.out)
    with tempfile.TemporaryDirectory(prefix='arrangement-native-', dir=args.out) as directory:
        result = verify(worker, Path(directory).resolve())
    result['native_fixture'] = str(worker.parent)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
