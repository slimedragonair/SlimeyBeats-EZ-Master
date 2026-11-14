"""Real stdio source/bank provenance matrix and actual native-project reopening."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import time
import wave

from protocol_client import Client

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'plugins/host/tools'))
from verify_host import floats


class LoggedClient(Client):
    def __init__(self, command, transcript):
        self.transcript = transcript
        super().__init__(command)

    def request(self, method, params):
        response = super().request(method, params)
        with self.transcript.open('a', encoding='utf-8') as log:
            log.write(json.dumps({'method': method, 'params': params, 'response': response}) + '\n')
        return response


def fixture(path, rate, channels, frames, frequency):
    pcm = [int(3000 * math.sin(i * frequency * math.tau / rate + channel * 0.2))
           for i in range(frames) for channel in range(channels)]
    with wave.open(str(path), 'wb') as wav:
        wav.setparams((channels, 2, rate, 0, 'NONE', ''))
        wav.writeframes(struct.pack('<' + 'h' * len(pcm), *pcm))
    return tuple(sample / 32768 for sample in pcm)


def edit(client, operation, audio_id):
    if operation == 'clean_voice':
        return client.run(operation, {'audio_id': audio_id, 'settings': {
            'highpass_hz': 150, 'noise_reduction': 0, 'deess': 0}})
    return client.run(operation, {'audio_id': audio_id, 'selection': {
        'start_seconds': 0.25, 'end_seconds': 0.75, 'low_hz': 500,
        'high_hz': 900, 'reduction_db': -12, 'mode': 'attenuate'}})


def check_project(client, output, loader, tool, source_name, expected, layout,
                  bank_pcm=None, before_stem=None):
    saved = client.run(tool)
    path = output / saved['project_path']
    if tool == 'open_studio_in_desktop':
        assert saved['desktop_launched'], saved
        receipt = path.with_suffix('.native-reopen.json')
        deadline = time.monotonic() + 20
        while not receipt.exists() and time.monotonic() < deadline:
            time.sleep(0.05)
        assert receipt.exists(), ('Native handoff failed to reopen', path)
        native = json.loads(receipt.read_text(encoding='utf-8'))
    else:
        completed = subprocess.run([str(loader), str(path)], capture_output=True,
                                   text=True, timeout=30)
        path.with_suffix('.native-reopen.log').write_text(
            completed.stdout + completed.stderr, encoding='utf-8')
        assert completed.returncode == 0, completed.stderr
        native = json.loads(completed.stdout)
    document = json.loads(path.read_text(encoding='utf-8'))
    assert document['source_name'] == native['source_name'] == source_name
    assert (native['sample_rate'], native['channels'], native['frames']) == layout
    assert floats(path.parent / document['source']['name']) == expected
    assert document['before_stem'] == native['direct_stem'] == before_stem
    assert native['direct_stem_apply_verified'] == (before_stem is not None)
    actual_bank = {stem['kind']: floats(path.parent / stem['file']['name'])
                   for stem in document['stems']}
    assert actual_bank == (bank_pcm or {})
    assert native['stems'] == len(actual_bank)
    if not bank_pcm:
        assert document['mix']['tracks'] == []
    return path


def verify(server, loader, evidence):
    evidence.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix='source-context-', dir=evidence))
    output = root / 'Exports'
    expected = fixture(root / 'Source A.wav', 16000, 1, 32000, 700)
    source_hash = hashlib.sha256((root / 'Source A.wav').read_bytes()).hexdigest()
    client = LoggedClient([str(server), '--input-root', str(root), '--output-root',
                          str(output), '--desktop-exe', str(loader)], root / 'stdio.jsonl')
    projects = []
    cases = []

    def check_both(name, pcm, layout, bank_pcm=None, before_stem=None):
        for tool in ('save_studio_project', 'open_studio_in_desktop'):
            projects.append(str(check_project(client, output, loader, tool, name,
                                             pcm, layout, bank_pcm, before_stem)))

    try:
        layouts = [('all_differ', 22050, 2, 22050),
                   ('frames_differ', 16000, 1, 16000),
                   ('rate_differ', 22050, 1, 32000),
                   ('channels_differ', 16000, 2, 32000),
                   ('layout_matches_but_unrelated', 16000, 1, 32000)]
        for label, rate, channels, frames in layouts:
            raw = {kind: fixture(root / (kind + '.wav'), rate, channels, frames, hz)
                   for kind, hz in [('vocals', 440), ('instrumental', 1500)]}
            bank_mix = tuple(a + b for a, b in zip(raw['vocals'], raw['instrumental']))
            for operation in ('clean_voice', 'repair_audio'):
                source = client.run('load_audio', {'library': 0, 'path': 'Source A.wav'})
                imported = client.run('import_stems', {'files': [
                    {'kind': kind, 'library': 0, 'path': kind + '.wav'} for kind in raw]})
                bank = imported['stem_set']['stem_set_id']
                changed = edit(client, operation, source['source_id'])
                check_both('Source A.wav', expected, (16000, 1, 32000))
                # Chaining and promotion must keep A, including when the promoted
                # source is revisited after the current preview switches to B.
                chained = edit(client, 'repair_audio', changed['studio_preview_id'])
                promoted = client.run('use_studio_for_mastering', {
                    'studio_preview_id': chained['studio_preview_id']})
                edit(client, 'clean_voice', promoted['source_id'])
                check_both('Source A.wav', expected, (16000, 1, 32000))
                session = client.call('get_studio_session')
                assert session['stem_set']['stem_set_id'] == bank
                exported = client.run('export_stems', {'stem_set_id': bank})
                assert {kind: floats(output / exported['folder'] / (kind + '.wav'))
                        for kind in raw} == raw
                stem = next(asset['audio_id'] for asset in session['assets']
                            if asset['label'] == 'vocals')
                direct = edit(client, operation, stem)
                check_both('Imported stem mix.wav', bank_mix, (rate, channels, frames),
                           raw, 'vocals')
                client.run('undo_studio_edit')
                check_both('Source A.wav', expected, (16000, 1, 32000))
                client.run('redo_studio_edit')
                applied = client.run('apply_preview_to_stem', {
                    'stem_set_id': bank, 'studio_preview_id': direct['studio_preview_id'],
                    'kind': 'vocals'})
                edited_raw = dict(raw, vocals=floats(output / direct['relative_preview_path']))
                assert edited_raw['vocals'] != raw['vocals']
                exported = client.run('export_stems', {'stem_set_id': applied['stem_set_id']})
                assert {kind: floats(output / exported['folder'] / (kind + '.wav'))
                        for kind in raw} == edited_raw
                check_both('Imported stem mix.wav', bank_mix, (rate, channels, frames), edited_raw)
                edit(client, operation, promoted['source_id'])
                check_both('Source A.wav', expected, (16000, 1, 32000))
                cases.append({'layout': label, 'operation': operation, 'passed': True})
        assert hashlib.sha256((root / 'Source A.wav').read_bytes()).hexdigest() == source_hash
        return {'cases': cases, 'case_count': len(cases), 'project_count': len(projects),
                'native_reopens': len(projects), 'projects': projects,
                'source_file_unchanged': True, 'evidence': str(root)}
    finally:
        client.close()
        (root / 'server-stderr.log').write_text(''.join(client.stderr), encoding='utf-8')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    parser.add_argument('--project-loader', type=Path, required=True)
    parser.add_argument('--evidence', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve(), args.project_loader.resolve(),
                            args.evidence.resolve()), indent=2))
