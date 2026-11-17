"""Real stdio diagnostics/repair checks with generated WAVs and independent PCM reads."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import tempfile

from protocol_client import LegacyClient as Client
from stem_import_fixtures import wav_data


RATE = 16000
SECONDS = 12


def write_fixture(path, *, damaged=False):
    values = []
    for frame in range(RATE * SECONDS):
        t = frame / RATE
        if damaged:
            value = 0.0 if 4 <= t < 6 else (
                sum(0.04 * math.sin(math.tau * (210 + band * 13) * t + band * 0.7)
                    for band in range(16)) +
                sum(0.025 * math.sin(math.tau * (1000 + band * 173) * t + band * 0.3)
                    for band in range(6)))
            if 0.5 <= t < 0.52:
                value = max(-1.0, min(1.0, 1.2 * math.sin(math.tau * 1000 * t)))
        else:
            value = 0.25 * math.sin(math.tau * 50 * t)
        values.extend((value, value * 0.75))
    # Float WAV permits exact full scale, signed zero and bitwise outside-region checks.
    pcm = struct.pack(f'<{len(values)}f', *values)
    fmt = struct.pack('<HHIIHH', 3, 2, RATE, RATE * 8, 8, 32)
    body = b'fmt ' + struct.pack('<I', len(fmt)) + fmt + b'data' + struct.pack('<I', len(pcm)) + pcm
    path.write_bytes(b'RIFF' + struct.pack('<I', len(body) + 4) + b'WAVE' + body)
    return pcm


def discovery(client):
    tools = {tool['name']: tool for tool in client.request('tools/list', {})['result']['tools']}
    caps = client.call('get_capabilities')
    assert len(tools) == caps['tool_count'] == 58
    assert tools['scan_audio_issues']['annotations']['readOnlyHint'] is True
    assert tools['scan_audio_issues']['annotations']['openWorldHint'] is False
    assert tools['preview_stem_doctor']['annotations']['destructiveHint'] is False
    schema = tools['preview_stem_doctor']['inputSchema']
    assert schema['additionalProperties'] is False
    assert schema['$defs']['RepairRequest']['additionalProperties'] is False
    assert caps['issue_detector']['automatic_trim'] is False
    help_text = json.dumps(client.call('get_help', {'topic': 'doctor'}))
    for term in ('scan_audio_issues', 'preview_stem_doctor', 'apply_preview_to_stem',
                 'undo_studio_edit', 'truncated', '0..1'):
        assert term in help_text
    assert 'scan_audio_issues' in client.initialized['result']['instructions']


def verify(server, destination):
    checks = []
    with tempfile.TemporaryDirectory(prefix='SlimeyBeats Doctor stdio ') as temporary:
        root = Path(temporary)
        library, output = root / 'Audio library', root / 'Exports'
        library.mkdir()
        clean, damaged = library / 'Clean bass.wav', library / 'Review fixture.wav'
        clean_pcm = write_fixture(clean)
        original = write_fixture(damaged, damaged=True)
        hashes = {path: hashlib.sha256(path.read_bytes()).hexdigest() for path in (clean, damaged)}
        client = Client([str(server), '--input-root', str(library), '--output-root', str(output)])
        try:
            discovery(client)
            checks.append('58-tool discovery, strict shared request schema, annotations and self-contained help')
            clean_id = client.run('load_audio', {'library': 0, 'path': clean.name})['source_id']
            before = client.call('get_studio_session')
            report = client.run('scan_audio_issues', {'audio_id': clean_id})['report']
            assert not any(issue['kind'] == 'clipping' for issue in report['issues']), report
            assert client.call('get_studio_session') == before
            checks.append('Clean -12 dBFS 50 Hz sine does not trigger clipping; scan leaves Studio unchanged')
            loaded = client.run('load_audio', {'library': 0, 'path': damaged.name})['source_id']
            before = client.call('get_studio_session')
            first = client.run('scan_audio_issues', {'audio_id': loaded})
            second = client.run('scan_audio_issues', {'audio_id': loaded})
            assert first == second
            assert client.call('get_studio_session') == before
            report = first['report']
            kinds = {issue['kind'] for issue in report['issues']}
            assert {'clipping', 'silence', 'possible_ending', 'low_mid_buildup'} <= kinds, report
            assert report['schema_version'] == 1 and report['sample_rate'] == RATE
            assert isinstance(report['thresholds'], dict) and report['truncated'] is False
            assert all(0 <= issue['start_seconds'] < issue['end_seconds'] <= SECONDS for issue in report['issues'])
            checks.append('Deterministic read-only scan locates four issue kinds with finite timestamps/evidence')
            for action in ('declip', 'reduce_low_mids'):
                issue = next(issue for issue in report['issues'] if issue['repair'] == action)
                repair = {key: issue[key] for key in ('start_seconds', 'end_seconds')}
                repair.update(action=action, strength=0.5)
                result = client.run('preview_stem_doctor', {'audio_id': loaded, 'repair': repair})
                assert result['doctor']['changed_samples'] > 0, result
                decoded = wav_data((output / result['relative_preview_path']).read_bytes())
                assert len(decoded) == len(original)
                start = int(repair['start_seconds'] * RATE + 0.5) * 8
                end = int(repair['end_seconds'] * RATE + 0.5) * 8
                assert decoded[:start] == original[:start] and decoded[end:] == original[end:]
                assert decoded[start:end] != original[start:end]
                preview = result['studio_preview_id']
                assert client.run('scan_audio_issues', {'audio_id': preview})['audio_id'] == preview
                client.run('undo_studio_edit')
                client.run('redo_studio_edit')
                assert client.call('get_studio_session')['preview']['studio_preview_id'] == preview
                checks.append(f'{action}: temporary float preview, exact outside selection, rescan and undo/redo')
            snapshot = client.call('get_studio_session')
            for change in ({'strength': -0.1}, {'strength': 1.1}, {'start_seconds': -1},
                           {'end_seconds': SECONDS + 1}, {'end_seconds': 0.1}, {'action': 'remove_silence'},
                           {'start_seconds': '1'}, {'auto_export': True}):
                repair = dict(action='reduce_low_mids', start_seconds=1., end_seconds=2., strength=0.5)
                repair.update(change)
                client.call('preview_stem_doctor', {'audio_id': loaded, 'repair': repair}, error=True)
                assert client.call('get_studio_session') == snapshot
            client.call('scan_audio_issues', {'audio_id': 'stale-source'}, error=True)
            client.call('scan_audio_issues', {'audio_id': clean_id}, error=True)
            checks.append('Invalid/unknown/stale requests preserve the current preview')
            imported = client.run('import_stems', {'files': [
                {'kind': 'vocals', 'library': 0, 'path': damaged.name},
                {'kind': 'instrumental', 'library': 0, 'path': clean.name}]})
            bank_id = imported['stem_set']['stem_set_id']
            client.run('mix_stems', {'stem_set_id': bank_id, 'options': {'protect_peaks': False}})
            bank_before = client.call('get_studio_session')['stem_set']
            stem_id = f'{bank_id}:vocals'
            scanned = client.run('scan_audio_issues', {'audio_id': stem_id})['report']
            issue = next(issue for issue in scanned['issues'] if issue['repair'] == 'declip')
            repair = {key: issue[key] for key in ('start_seconds', 'end_seconds')}
            repair.update(action='declip', strength=0.5)
            fixed = client.run('preview_stem_doctor', {'audio_id': stem_id, 'repair': repair})
            fixed_pcm = wav_data((output / fixed['relative_preview_path']).read_bytes())
            assert client.call('get_studio_session')['stem_set'] == bank_before
            applied = client.run('apply_preview_to_stem', {'stem_set_id': bank_id,
                'studio_preview_id': fixed['studio_preview_id'], 'kind': 'vocals'})
            assert applied['stem_set_id'] != bank_id
            mixed = wav_data((output / applied['relative_preview_path']).read_bytes())
            fixed_values = struct.unpack(f'<{len(fixed_pcm) // 4}f', fixed_pcm)
            other_values = struct.unpack(f'<{len(clean_pcm) // 4}f', clean_pcm)
            expected = struct.pack(f'<{len(fixed_values)}f', *(a + b for a, b in zip(fixed_values, other_values)))
            assert mixed == expected, 'Applied preview does not match edited stem plus untouched other stem'
            client.call('scan_audio_issues', {'audio_id': stem_id}, error=True)
            client.run('undo_studio_edit')
            assert client.call('get_studio_session')['stem_set'] == bank_before
            client.run('redo_studio_edit')
            assert client.call('get_studio_session')['stem_set']['stem_set_id'] == applied['stem_set_id']
            checks.append('Direct-stem preview leaves bank unchanged until explicit apply; exact independently summed mix, refreshed IDs and bank undo/redo')
            # The companion creates a private temporary cache; none of these calls exports final audio.
            assert not any(path.is_file() for path in output.iterdir())
            assert all(hashlib.sha256(path.read_bytes()).hexdigest() == digest for path, digest in hashes.items())
            checks.append('Source WAVs unchanged; no final export or optional download requested')
        finally:
            client.close()
    result = {'passed': True, 'checks': checks, 'scope': 'Generated audio, actual local MCP stdio and independent exported float PCM; no physical listening or model inference.'}
    if destination:
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    verify(args.server.resolve(), args.out)
