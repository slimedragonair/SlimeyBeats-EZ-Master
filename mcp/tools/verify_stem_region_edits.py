"""Real stdio discovery, selected-stem PCM, controls, history and project checks."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import struct
import tempfile
import wave

from protocol_client import Client
from stem_import_fixtures import wav_data


KINDS = ('vocals', 'part_64', 'drums')
OPERATIONS = ('silence', 'fade_in', 'fade_out', 'gain', 'reverse')
RATE, CHANNELS, FRAMES = 8000, 2, 1600
START, END = 400, 1200


def f32(value):
    return struct.unpack('<f', struct.pack('<f', value))[0]


def fixture(path, index):
    integers = [((frame * (index + 1) + channel * 23) % 97 - 48) * (index + 1)
                for frame in range(FRAMES) for channel in range(CHANNELS)]
    with wave.open(str(path), 'wb') as wav:
        wav.setparams((CHANNELS, 2, RATE, 0, 'NONE', ''))
        wav.writeframes(struct.pack(f'<{len(integers)}h', *integers))
    return struct.pack(f'<{len(integers)}f', *(value / 32768 for value in integers))


def floats(data):
    return struct.unpack(f'<{len(data) // 4}f', data)


def edited_pcm(original, operation):
    samples = list(floats(original))
    selected = samples[START * CHANNELS:END * CHANNELS]
    for frame in range(END - START):
        for channel in range(CHANNELS):
            value = selected[frame * CHANNELS + channel]
            if operation == 'silence':
                value = 0.
            elif operation == 'fade_in':
                value = f32(value * frame / (END - START - 1))
            elif operation == 'fade_out':
                value = f32(value * (END - START - 1 - frame) / (END - START - 1))
            elif operation == 'gain':
                value = f32(value * 10 ** (-3 / 20))
            else:
                value = selected[(END - START - 1 - frame) * CHANNELS + channel]
            samples[(START + frame) * CHANNELS + channel] = value
    return struct.pack(f'<{len(samples)}f', *samples)


def assert_close_pcm(actual, expected):
    assert len(actual) == len(expected)
    assert max(abs(a - b) for a, b in zip(floats(actual), floats(expected))) < 1e-8


def mixed_pcm(stems, controls):
    output = [0.] * (FRAMES * CHANNELS)
    solo = any(track['solo'] for track in controls['tracks'])
    for track in controls['tracks']:
        if track['mute'] or (solo and not track['solo']):
            continue
        gain = f32(10 ** ((track['gain_db'] + controls['output_gain_db']) / 20))
        balance = track['balance']
        levels = (f32(math.cos(balance * math.pi / 2)) if balance > 0 else 1.,
                  f32(math.cos(-balance * math.pi / 2)) if balance < 0 else 1.)
        for index, value in enumerate(floats(stems[track['kind']])):
            output[index] = f32(output[index] + f32(f32(value * gain) * levels[index % 2]))
    return struct.pack(f'<{len(output)}f', *output)


def project(client, output):
    result = client.run('save_studio_project')
    path = output / result['project_path']
    document = json.loads(path.read_text(encoding='utf-8'))
    read = lambda asset: wav_data((path.parent / asset['name']).read_bytes())
    return document, {stem['kind']: read(stem['file']) for stem in document['stems']}, {
        name: read(document[name]) for name in ('source', 'before', 'preview')}


def discovery(client):
    tools = client.request('tools/list', {})['result']['tools']
    assert len(tools) == 45 and len({tool['name'] for tool in tools}) == 45
    capabilities = client.call('get_capabilities')
    assert capabilities['tool_count'] == len(tools)
    assert capabilities['stem_region_edit']['operations'] == list(OPERATIONS)
    tool = next(tool for tool in tools if tool['name'] == 'edit_stem_region')
    schema = tool['inputSchema']
    assert schema['additionalProperties'] is False
    assert set(schema['required']) == {'stem_set_id', 'kind', 'edit'}
    assert schema['$defs']['RegionEdit']['additionalProperties'] is False
    for name in ('start_seconds', 'end_seconds'):
        assert schema['$defs']['RegionEdit']['properties'][name]['type'] == 'number'
    alternatives = schema['$defs']['RegionOperation']['oneOf']
    assert len(alternatives) == 5
    for alternative in alternatives:
        assert alternative['additionalProperties'] is False
    assert 'pattern' in schema['$defs']['StemKind']
    assert tool['annotations']['destructiveHint'] is False
    guide = client.call('get_help', {'topic': 'stems'})['time_region_edit']
    assert guide['example']['tool'] == 'edit_stem_region'
    assert [item['kind'] for item in guide['operations']] == list(OPERATIONS)
    assert 'whole mix' in guide['rendered_audio'].lower() or 'new mix' in guide['rendered_audio']
    assert 'edit_stem_region' in client.initialized['result']['instructions']
    return len(tools)


def request(bank, operation='silence'):
    action = {'kind': operation}
    if operation == 'gain':
        action['db'] = -3.
    return {'stem_set_id': bank, 'kind': 'part_64', 'edit': {
        'start_seconds': START / RATE, 'end_seconds': END / RATE, 'operation': action}}


def invalid_requests(client, bank):
    before = client.call('get_studio_session')
    values = []
    for start, end in [(-0.1, 0.1), (0.1, 0.1), (0.15, 0.05), (0., 0.3), (0., 0.000001),
                       ('0.05', 0.15), (True, 0.15), (0.05, None)]:
        value = request(bank)
        value['edit'].update(start_seconds=start, end_seconds=end)
        values.append(value)
    for action in [{'kind': 'gain'}, {'kind': 'gain', 'db': '-3'}, {'kind': 'gain', 'db': 12.1},
                   {'kind': 'gain', 'db': -60.1}, {'kind': 'split'}, {'kind': 'silence', 'db': 0}]:
        value = request(bank)
        value['edit']['operation'] = action
        values.append(value)
    for field, value in [('kind', 'part_65'), ('kind', 'Backing Vocals'), ('kind', 'part_064'),
                          ('stem_set_id', 'stems-stale'), ('export', True)]:
        invalid = request(bank)
        invalid[field] = value
        values.append(invalid)
    invalid = request(bank)
    invalid['edit']['loop'] = True
    values.append(invalid)
    for invalid in values:
        client.call('edit_stem_region', invalid, error=True)
        assert client.call('get_studio_session') == before
        assert client.call('get_session')['active_job_id'] is None
    return len(values)


def verify(server):
    with tempfile.TemporaryDirectory(prefix='SlimeyBeats region MCP ') as temporary:
        root = Path(temporary)
        output = root / 'Working projects'
        names = ('Lead Vocals.wav', 'Backing Vocals.wav', 'Drums.wav')
        originals = {kind: fixture(root / name, index)
                     for index, (kind, name) in enumerate(zip(KINDS, names))}
        source_hashes = {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in names}
        client = Client([str(server), '--input-root', str(root), '--output-root', str(output),
                         '--studio-data', str(root / 'Unused model runtime')])
        try:
            count = discovery(client)
            source = client.run('load_audio', {'library': 0, 'path': names[0]})['source_id']
            for operation in OPERATIONS:
                imported = client.run('import_stems', {'files': [
                    {'kind': kind, 'library': 0, 'path': name} for kind, name in zip(KINDS, names)]})
                bank = imported['stem_set']['stem_set_id']
                client.run('mix_stems', {'stem_set_id': bank, 'options': {
                    'protect_peaks': False, 'output_gain_db': -2.,
                    'tracks': [{'kind': 'part_64', 'gain_db': -6., 'balance': 0.25},
                               {'kind': 'drums', 'mute': True}]}})
                before = client.call('get_studio_session')
                controls = before['stem_set']['controls']
                project_count = len(list(output.glob('Studio-session-*')))
                rendered = client.run('edit_stem_region', request(bank, operation))
                assert rendered['stem_set_id'] != bank
                assert rendered['stem_set']['stem_set_id'] == rendered['stem_set_id']
                assert rendered['preview']['studio_preview_id'] == rendered['studio_preview_id']
                current = client.call('get_studio_session')
                for key in ('assets', 'stem_set', 'preview', 'can_undo', 'can_redo'):
                    assert rendered[key] == current[key]
                for kind in KINDS:
                    assert any(asset['audio_id'] == f"{rendered['stem_set_id']}:{kind}"
                               for asset in rendered['assets'])
                assert current['stem_set']['controls'] == controls
                assert current['stem_set']['stem_names']['part_64'] == 'Backing Vocals'
                assert all(asset['sample_rate'] == RATE and asset['channels'] == CHANNELS
                           and asset['duration_seconds'] == FRAMES / RATE for asset in current['assets'])
                assert 'Audio edit rendered' in rendered['notes'][0]
                assert len(list(output.glob('Studio-session-*'))) == project_count, 'Edit unexpectedly saved a project'
                assert not list(output.glob('*.wav')), 'Edit unexpectedly exported final audio'
                document, stems, audio = project(client, output)
                assert document['view'] == 'stems' and document['mix'] == controls
                assert document['source_name'] == 'Imported stem mix.wav'
                assert document['notes'] == rendered['notes']
                assert stems['vocals'] == originals['vocals'] and stems['drums'] == originals['drums']
                changed = edited_pcm(originals['part_64'], operation)
                assert stems['part_64'][:START * 8] == originals['part_64'][:START * 8]
                assert stems['part_64'][END * 8:] == originals['part_64'][END * 8:]
                assert_close_pcm(stems['part_64'], changed)
                expected = dict(originals, part_64=changed)
                assert_close_pcm(audio['preview'], mixed_pcm(expected, controls))
                assert_close_pcm(audio['before'], mixed_pcm(originals, controls))
                neutral = dict(controls, output_gain_db=0., tracks=[dict(track, gain_db=0., balance=0.,
                    mute=False, solo=False) for track in controls['tracks']])
                assert_close_pcm(audio['source'], mixed_pcm(originals, neutral))
                assert audio['preview'] != stems['part_64']
                assert client.call('get_session')['source']['source_id'] == source
                client.call('edit_stem_region', request(bank), error=True)
                if operation == 'silence':
                    baseline_mix = audio['before']
                    undo = client.run('undo_studio_edit')
                    assert undo['stem_set']['stem_set_id'] == bank
                    assert undo['preview']['studio_preview_id'] == before['preview']['studio_preview_id']
                    _, restored, undo_audio = project(client, output)
                    assert restored == originals and undo_audio['preview'] == baseline_mix
                    redo = client.run('redo_studio_edit')
                    assert redo['stem_set']['stem_set_id'] == rendered['stem_set_id']
                    _, restored, redo_audio = project(client, output)
                    assert restored == stems and redo_audio['preview'] == audio['preview']
            rejected = invalid_requests(client, current['stem_set']['stem_set_id'])
            client.run('undo_studio_edit')
            restored = client.call('get_studio_session')
            client.run('edit_stem_region', request(restored['stem_set']['stem_set_id'], 'gain'))
            assert not client.call('get_studio_session')['can_redo']
            assert all(hashlib.sha256((root / name).read_bytes()).hexdigest() == digest
                       for name, digest in source_hashes.items())
            assert not (root / 'Unused model runtime').exists()
            return {'tools': count, 'operations': list(OPERATIONS), 'rejected_unchanged': rejected,
                    'pcm_controls_history_projects': 'passed', 'desktop_launch': 'not_run'}
        finally:
            client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
