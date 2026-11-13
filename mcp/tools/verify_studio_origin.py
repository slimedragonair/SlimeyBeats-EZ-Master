"""Studio origins/labels survive chained edits, promotion and self-contained save."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import wave

from protocol_client import Client

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'plugins/host/tools'))
from verify_host import floats


def write(path, hz):
    samples = [int(3000 * math.sin(i * hz * math.tau / 16000)) for i in range(64000)]
    with wave.open(str(path), 'wb') as wav:
        wav.setparams((1, 2, 16000, 0, 'NONE', ''))
        wav.writeframes(struct.pack('<' + 'h' * len(samples), *samples))
    return tuple(sample / 32768 for sample in samples)


def verify(server):
    with tempfile.TemporaryDirectory(prefix='Studio immutable origin QA ') as temporary:
        root = Path(temporary)
        original = root / 'Original recording.wav'
        expected = write(original, 750)
        vocal = write(root / 'vocals.wav', 440)
        instrumental = write(root / 'instrumental.wav', 2000)
        source_hash = hashlib.sha256(original.read_bytes()).hexdigest()
        output = root / 'Exports'
        client = Client([str(server), '--input-root', str(root), '--output-root', str(output)])
        try:
            loaded = client.run('load_audio', {'library': 0, 'path': original.name})
            first = client.run('clean_voice', {'audio_id': loaded['source_id'], 'settings': {'noise_reduction': 0, 'deess': 0}})
            second = client.run('repair_audio', {'audio_id': first['studio_preview_id'], 'selection': {'start_seconds': 1, 'end_seconds': 2, 'low_hz': 600, 'high_hz': 900, 'reduction_db': -12, 'mode': 'attenuate'}})
            promoted = client.run('use_studio_for_mastering', {'studio_preview_id': second['studio_preview_id']})
            assert promoted['source_id'] != loaded['source_id']
            session = client.call('get_studio_session')
            assert any(asset['audio_id'] == loaded['source_id'] and asset['label'] == 'Studio original' for asset in session['assets'])
            assert client.run('analyze_studio_audio', {'audio_id': loaded['source_id']})['audio_id'] == loaded['source_id']
            saved = client.run('save_studio_project')
            project = output / saved['project_path']
            document = json.loads(project.read_text())
            assert document['source_name'] == original.name
            assert floats(project.parent / document['source']['name']) == expected
            assert floats(project.parent / document['before']['name']) != expected

            # A later unrelated mastering source must not rename an imported bank.
            client.run('load_audio', {'library': 0, 'path': original.name})
            imported = client.run('import_stems', {'files': [{'kind': kind, 'library': 0, 'path': kind + '.wav'} for kind in ['vocals', 'instrumental']]})
            client.run('use_studio_for_mastering', {'studio_preview_id': imported['preview']['studio_preview_id']})
            saved = client.run('save_studio_project')
            project = output / saved['project_path']
            document = json.loads(project.read_text())
            assert document['source_name'] == 'Imported stem mix.wav'
            assert floats(project.parent / document['source']['name']) == tuple(a + b for a, b in zip(vocal, instrumental))
            assert hashlib.sha256(original.read_bytes()).hexdigest() == source_hash
            return {'original_retained_after_chaining_and_promotion': True, 'original_id_analyzable': True, 'project_original_PCM_exact': True, 'bank_name_independent_of_mastering_source': True, 'source_file_unchanged': True}
        finally:
            client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--server', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
