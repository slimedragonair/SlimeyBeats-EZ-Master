"""Actual MCP nine-stem discovery, edit/mix, export and self-contained handoff."""
import argparse
import json
import math
from pathlib import Path
import struct
import subprocess
import tempfile
import wave
from protocol_client import Client


def verify(server, desktop=None, evidence=None):
    with tempfile.TemporaryDirectory(prefix='Nine stem MCP QA ') as temporary:
        root = Path(temporary)
        output = root / 'Exports'
        kinds = ['vocals', 'drums', 'bass', 'guitar', 'piano', 'synth', 'strings', 'percussion', 'part_9']
        for index, kind in enumerate(kinds):
            with wave.open(str(root / (kind + '.wav')), 'wb') as wav:
                wav.setparams((2, 2, 16000, 0, 'NONE', ''))
                wav.writeframes(b''.join(struct.pack('<hh', int(700 * math.sin(i * (200 + index * 150) * math.tau / 16000)), 0) for i in range(32000)))
        client = Client([str(server), '--input-root', str(root), '--output-root', str(output)])
        try:
            caps = client.call('get_capabilities')['stem_import']
            assert caps['maximum'] is None and 'part_9' in caps['kinds'] and 'guitar' in caps['kinds']
            tools = client.request('tools/list', {})['result']['tools']
            tool = next(t for t in tools if t['name'] == 'import_stems')
            assert 'at least two' in tool['description'] and 'part_N' in json.dumps(tool['inputSchema'])
            files = [{'kind': kind, 'library': 0, 'path': kind + '.wav'} for kind in kinds]
            imported = client.run('import_stems', {'files': files})
            bank = imported['stem_set']['stem_set_id']
            raw = client.run('export_stems', {'stem_set_id': bank})
            before_folder = output / raw['folder']
            before = {kind: (before_folder / (kind + '.wav')).read_bytes() for kind in kinds}
            assert len(raw['manifest']['stems']) == 9
            ninth = next(a['audio_id'] for a in imported['assets'] if a['label'] == 'part_9')
            cleaned = client.run('clean_voice', {'audio_id': ninth, 'settings': {'highpass_hz': 150, 'noise_reduction': 0, 'deess': 0}})
            applied = client.run('apply_preview_to_stem', {'stem_set_id': bank, 'studio_preview_id': cleaned['studio_preview_id'], 'kind': 'part_9'})
            bank = applied['stem_set_id']
            raw = client.run('export_stems', {'stem_set_id': bank})
            after = {kind: (output / raw['folder'] / (kind + '.wav')).read_bytes() for kind in kinds}
            assert before['part_9'] != after['part_9']
            assert all(before[kind] == after[kind] for kind in kinds[:-1])
            client.run('mix_stems', {'stem_set_id': bank, 'options': {'tracks': [{'kind': 'part_9', 'gain_db': -4}, {'kind': 'piano', 'mute': True}]}})
            current = client.call('get_studio_session')
            assert len(current['stem_set']['controls']['tracks']) == 9
            preview_id = current['preview']['studio_preview_id']
            # Missing paths and duplicate identities must not replace a workspace.
            client.call('import_stems', {'files': files + [{'kind': 'other', 'library': 0, 'path': 'missing.wav'}]}, error=True)
            client.call('import_stems', {'files': files[:-1] + [files[0]]}, error=True)
            assert client.call('get_studio_session')['preview']['studio_preview_id'] == preview_id
            saved = client.run('save_studio_project')
            project = output / saved['project_path']
            document = json.loads(project.read_text())
            assert len(document['stems']) == 9
            assert document['mix']['tracks'][-1]['kind'] == 'part_9'
            assert document['mix']['tracks'][-1]['gain_db'] == -4
        finally:
            client.close()
        result = {'discovery_and_schema': True, 'nine_import_edit_mix_export': True,
                  'other_eight_parts_exact': True, 'missing_path_and_duplicate_preserve_preview': True, 'project_nine_parts': True}
        if desktop:
            # Reopen without any of the external source stems.
            for kind in kinds:
                (root / (kind + '.wav')).unlink()
            evidence.mkdir(parents=True, exist_ok=True)
            for name, width, height in [('nine-desktop', 1440, 1000), ('nine-compact', 1100, 720)]:
                image = evidence / (name + '.png')
                launched = subprocess.run([str(desktop), '--studio-session', str(project), '--capture-ui', str(image), '--capture-size', str(width), str(height)], capture_output=True, text=True, timeout=90)
                assert launched.returncode == 0, launched.stderr
                state = json.loads(image.with_suffix('.state.json').read_text())
                assert state['error'] is None and state['preview_ready'] and state['playback_ready'], state
            result['desktop_self_contained_reopen'] = True
        return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    parser.add_argument('--desktop', type=Path)
    parser.add_argument('--evidence', type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve(), args.desktop.resolve() if args.desktop else None, args.evidence), indent=2))
