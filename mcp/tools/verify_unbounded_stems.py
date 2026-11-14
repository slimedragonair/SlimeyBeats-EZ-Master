"""Live MCP discovery/schema, large loose/ZIP banks, identity and resource regressions."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import tempfile
import wave
from protocol_client import Client
from stem_import_fixtures import LEGACY_KINDS, kinds, wav_data, wav_fixture, write_zip


def discover(client):
    instructions = client.initialized['result']['instructions']
    assert 'no fixed stem-count cap' in instructions and 'Omit parts' in instructions
    caps = client.call('get_capabilities')['stem_import']
    assert caps['maximum'] is None and caps['minimum'] == 2
    assert caps['kinds_are_examples'] and set(LEGACY_KINDS) <= set(caps['kinds'])
    assert caps['selection_default'] == 'all distinct parts'
    limits = caps['resource_limits']
    assert not limits['fixed_stem_count_cap']
    assert limits['max_decoded_input_mib'] == 512
    assert limits['max_aggregate_decoded_bank_mib'] == 4096
    assert limits['max_import_metadata_bytes'] == 8 * 1024 * 1024
    assert limits['metadata_overhead_bytes_per_entry'] == 256
    assert caps['automatic_separation_modes'] == ['vocal_instrumental', 'four_stems']
    tools = {t['name']: t for t in client.request('tools/list', {})['result']['tools']}
    valid = LEGACY_KINDS + ['part_10', 'part_24', 'part_64', 'part_1000', 'part_4294967295']
    invalid = ['', 'unknown', 'part_', 'part_0', 'part_00', 'part_01', 'part_+1', 'part_-1',
               'part_1 ', ' part_1', 'Part_10', 'part_1.0', 'part_1e2', 'part_١',
               'part_4294967296', 'part_9999999999', 'part_10000000000']
    for name in ('import_stems', 'import_stem_bundle', 'mix_stems', 'apply_preview_to_stem'):
        schema = tools[name]['inputSchema']['$defs']['StemKind']
        assert schema['type'] == 'string' and 'enum' not in schema
        assert all(re.fullmatch(schema['pattern'], value) for value in valid), (name, schema)
        assert all(not re.fullmatch(schema['pattern'], value) for value in invalid), (name, schema)
    for name in ('import_stems', 'import_stem_bundle', 'inspect_stem_bundle'):
        assert 'No fixed stem-count cap' in tools[name]['description'] or 'no fixed stem-count cap' in tools[name]['description']
    guide = client.call('get_help', {'topic': 'stems'})
    assert 'ALL distinct parts' in guide['zip_import']['steps'][-1]['then']
    assert 'part_64' in json.dumps(guide) and '4294967295' in json.dumps(guide)
    text = json.dumps([instructions, guide, tools], ensure_ascii=False)
    assert '2–9' not in text and 'more than nine' not in text and '64 distinct parts' not in text
    assert client.call('get_help', {'topic': 'quick_start'})['job_example']
    return tools, invalid


def bank_controls(session, count, expected_ids=None):
    controls = session['stem_set']['controls']['tracks']
    ids = [track['kind'] for track in controls]
    assert len(ids) == len(set(ids)) == count
    if expected_ids is not None:
        assert set(ids) == set(expected_ids)
    assert all(track['gain_db'] == 0 and track['balance'] == 0
               and not track['mute'] and not track['solo'] for track in controls)
    return ids


def raw_bank(client, session, output, expected=None):
    exported = client.run('export_stems', {'stem_set_id': session['stem_set']['stem_set_id']})
    entries = exported['manifest']['stems']
    folder = output / exported['folder']
    assert (folder / 'stems.json').is_file()
    assert len(entries) == len(session['stem_set']['controls']['tracks'])
    assert exported['manifest']['format'] == 'wav32f'
    data = {}
    for entry in entries:
        assert entry['file'] == entry['kind'] + '.wav'
        assert entry['frames'] == 1600 and entry['sample_rate'] == 8000 and entry['channels'] == 1
        pcm = wav_data((folder / entry['file']).read_bytes())
        if expected is not None:
            assert pcm == expected[entry['kind']], entry
        data[entry['kind']] = pcm
    return data


def preserves_preview(client, preview):
    assert client.call('get_studio_session')['preview']['studio_preview_id'] == preview


def reject_job(client, tool, arguments, preview):
    job = client.finish(client.call(tool, arguments))
    assert job['status'] == 'failed' and job['error'], (tool, job)
    preserves_preview(client, preview)
    return job['error']


def loose_banks(client, root, output, invalid):
    for count in (24, 64, 100):
        identities = kinds(count)
        files, expected = [], {}
        for index, kind in enumerate(identities):
            payload, expected[kind] = wav_fixture(index)
            path = f'{kind}.wav'
            (root / path).write_bytes(payload)
            files.append({'kind': kind, 'library': 0, 'path': path})
        imported = client.run('import_stems', {'files': files})
        bank_controls(imported, count, identities)
        before = raw_bank(client, imported, output, expected)
        bank = imported['stem_set']['stem_set_id']
        mixed = client.run('mix_stems', {'stem_set_id': bank, 'options': {
            'tracks': [{'kind': 'part_64', 'gain_db': -2}]}})
        current = client.call('get_studio_session')
        assert current['stem_set']['stem_set_id'] == bank
        for track in current['stem_set']['controls']['tracks']:
            assert track['gain_db'] == (-2 if track['kind'] == 'part_64' else 0)
        assert raw_bank(client, current, output, expected) == before
        preview = mixed['studio_preview_id']
        for bad in invalid + [0, None, {}, {'extra_part': 10}]:
            client.call('import_stems', {'files': [files[0], dict(files[1], kind=bad)]}, error=True)
        for bad_files in ([], files[:1], files + [files[0]]):
            client.call('import_stems', {'files': bad_files}, error=True)
        client.call('mix_stems', {'stem_set_id': bank, 'options': {
            'tracks': [{'kind': 'part_64'}, {'kind': 'part_64'}]}}, error=True)
        client.call('mix_stems', {'stem_set_id': bank, 'options': {
            'tracks': [{'kind': 'part_4294967294'}]}}, error=True)
        preserves_preview(client, preview)
        if count == 64:
            (root / 'unaligned.wav').write_bytes(wav_fixture(1, frames=1601)[0])
            reject_job(client, 'import_stems', {'files': [files[0], {
                'kind': 'part_64', 'library': 0, 'path': 'unaligned.wav'}]}, preview)
            audio_id = next(a['audio_id'] for a in current['assets'] if a['label'] == 'part_64')
            cleaned = client.run('clean_voice', {'audio_id': audio_id, 'settings': {
                'highpass_hz': 150, 'noise_reduction': 0, 'deess': 0}})
            client.run('apply_preview_to_stem', {'stem_set_id': bank,
                'studio_preview_id': cleaned['studio_preview_id'], 'kind': 'part_64'})
            current = client.call('get_studio_session')
            after = raw_bank(client, current, output)
            assert after['part_64'] != before['part_64']
            assert all(after[k] == before[k] for k in identities if k != 'part_64')
            saved = client.run('save_studio_project')
            project = json.loads((output / saved['project_path']).read_text(encoding='utf-8'))
            assert len(project['stems']) == 64 and len(project['mix']['tracks']) == 64
            assert {track['kind'] for track in project['mix']['tracks']} == set(identities)
            assert next(t for t in project['mix']['tracks'] if t['kind'] == 'part_64')['gain_db'] == -2
    return [24, 64, 100]


def zip_banks(client, root, output):
    for count in (24, 64, 100):
        path = root / f'{count} paired parts.zip'
        entries, by_name = [], {}
        for index in range(count):
            name = f'custom take {index:03}'
            payload, by_name[name] = wav_fixture(index)
            entries.extend([(f'Song/WAV/{name}.wav', payload),
                            (f'Song/MP3/{name}.mp3', b'unselected encoding must not be decoded')])
        write_zip(path, entries)
        original = hashlib.sha256(path.read_bytes()).hexdigest()
        inspected = client.call('inspect_stem_bundle', {'library': 0, 'path': path.name})
        assert inspected['max_selected_parts'] is None and len(inspected['parts']) == count
        assert all(len(p['versions']) == 2 for p in inspected['parts'])
        imported = client.run('import_stem_bundle', {'library': 0, 'path': path.name})
        ids = bank_controls(imported, count)
        assert set(ids) == {p['kind'] for p in inspected['parts']}
        expected = {part['kind']: by_name[part['name']] for part in inspected['parts']}
        raw_bank(client, imported, output, expected)
        step = imported['preview']['steps'][-1]
        assert len(step['parts']) == count and step['preferred_format'] == 'wav'
        assert step['alternate_encodings_not_doubled']
        preview = imported['preview']['studio_preview_id']
        for selections in ([], [{'part_index': 0}], [{'part_index': 0}, {'part_index': 0}],
                           [{'part_index': count}, {'part_index': 1}],
                           [{'part_index': 0, 'kind': 'part_64'}, {'part_index': 1, 'kind': 'part_64'}]):
            reject_job(client, 'import_stem_bundle', {
                'library': 0, 'path': path.name, 'parts': selections}, preview)
        selected = client.run('import_stem_bundle', {'library': 0, 'path': path.name,
            'parts': [{'part_index': index} for index in range(16)]})
        bank_controls(selected, 16)
        assert hashlib.sha256(path.read_bytes()).hexdigest() == original
    # Same instrument in separate song folders remains distinct, with paired encodings.
    path = root / 'two songs.zip'
    first, second = wav_fixture(1), wav_fixture(2)
    write_zip(path, [(f'{song}/{encoding}/guitar.{extension}', data)
        for song, data in [('Song A', first[0]), ('Song B', second[0])]
        for encoding, extension in [('WAV', 'wav'), ('MP3', 'mp3')]])
    inspected = client.call('inspect_stem_bundle', {'library': 0, 'path': path.name})
    assert len(inspected['parts']) == 2 and all(len(p['versions']) == 2 for p in inspected['parts'])
    imported = client.run('import_stem_bundle', {'library': 0, 'path': path.name})
    bank_controls(imported, 2)
    assert set(raw_bank(client, imported, output).values()) == {first[1], second[1]}
    return [24, 64, 100]


def resource_budget(server, root, output):
    client = Client([str(server), '--input-root', str(root), '--output-root', str(output),
                     '--max-decoded-mib', '8'])
    try:
        files = [{'kind': kind, 'library': 0, 'path': kind + '.wav'} for kind in ('vocals', 'drums')]
        imported = client.run('import_stems', {'files': files})
        preview = imported['preview']['studio_preview_id']
        for index in range(2):
            with wave.open(str(root / f'large-{index}.wav'), 'wb') as wav:
                wav.setparams((1, 2, 8000, 0, 'NONE', ''))
                wav.writeframes(b'\x01\x00' * 1_200_000)
        files = [dict(file, path=f'large-{index}.wav') for index, file in enumerate(files)]
        # Each 4.8 MB decoded input fits 8 MiB; their 9.6 MB bank must still pass.
        imported = client.run('import_stems', {'files': files})
        bank_controls(imported, 2)
        preview = imported['preview']['studio_preview_id']
        with wave.open(str(root / 'too-large.wav'), 'wb') as wav:
            wav.setparams((1, 2, 8000, 0, 'NONE', ''))
            wav.writeframes(b'\x01\x00' * 2_200_000)
        error = reject_job(client, 'import_stems', {
            'files': [dict(files[0], path='too-large.wav'), files[1]]}, preview)
        assert any(word in str(error).lower() for word in ('budget', 'decoded', 'memory'))
    finally:
        client.close()


def verify(server):
    with tempfile.TemporaryDirectory(prefix='Unbounded stem MCP QA ') as temporary:
        root = Path(temporary)
        output = root / 'Exports'
        client = Client([str(server), '--input-root', str(root), '--output-root', str(output)])
        try:
            tools, invalid = discover(client)
            loose = loose_banks(client, root, output, invalid)
            bundles = zip_banks(client, root, output)
            assert client.call('get_session')['master'] is None
        finally:
            client.close()
        resource_budget(server, root, output)
        return {'tools': len(tools), 'loose_bank_counts': loose, 'paired_zip_counts': bundles,
                'no_fixed_cap_and_all_parts_default': True, 'legacy_and_extensible_schema_ids': True,
                'invalid_ids_collisions_alignment_preserve_preview': True, 'raw_stems_sample_exact': True,
                'one_custom_stem_edit_preserves_other_63': True, 'project_64_ids_and_faders': True,
                'cross_song_same_name_parts_preserved': True, 'per_input_budget_is_not_bank_budget': True,
                'automatic_extraction_contract_unchanged': True, 'mastering_not_run': True}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve()), indent=2))
