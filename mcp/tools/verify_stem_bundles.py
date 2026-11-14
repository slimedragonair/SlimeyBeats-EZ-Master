"""Real MCP Suno ZIP import, format preference/fallback and optional subset selection."""
import argparse
import array
import hashlib
import io
import json
import math
from pathlib import Path
import sys
import tempfile
import wave
from protocol_client import Client
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'rust/tools'))
from stem_bundle_fixtures import chunk, suno_fixture, zipped


def fixture(name, hz=440):
    pcm = array.array('h', (int(2200 * math.sin(i * hz * math.tau / 48000)) for i in range(144000)))
    output = io.BytesIO()
    with wave.open(output, 'wb') as wav:
        wav.setparams((1, 2, 48000, 0, 'NONE', ''))
        wav.writeframes(pcm.tobytes())
    return {'name': name, 'mimeType': 'audio/wav', 'buffer': output.getvalue()}


def verify(server, codecs=False):
    files = suno_fixture(fixture, codecs=codecs)
    with tempfile.TemporaryDirectory(prefix='MCP Suno ZIP QA ') as temporary:
        root = Path(temporary)
        output = root / 'Exports'
        source = root / 'Suno stems.zip'
        source.write_bytes(zipped(files, 'Suno song/')['buffer'])
        original_hash = hashlib.sha256(source.read_bytes()).hexdigest()
        client = Client([str(server), '--input-root', str(root), '--output-root', str(output)])
        def bank(session):
            raw = client.run('export_stems', {'stem_set_id': session['stem_set']['stem_set_id']})
            return {entry['kind']: (output / raw['folder'] / entry['file']).read_bytes() for entry in raw['manifest']['stems']}
        try:
            listing = client.call('list_audio_files', {'library': 0})
            assert any(file['path'] == source.name and file['stem_archive'] for file in listing['files'])
            tools = client.request('tools/list', {})['result']['tools']
            assert {'inspect_stem_bundle', 'import_stem_bundle'} <= {tool['name'] for tool in tools}
            inspected = client.call('inspect_stem_bundle', {'library': 0, 'path': source.name})
            assert len(inspected['parts']) == 9 and all(len(part['versions']) == 2 for part in inspected['parts'])
            imported = client.run('import_stem_bundle', {'library': 0, 'path': source.name})
            wav_bank = bank(imported)
            assert len(wav_bank) == 9
            for file in files[::2]:
                pcm = array.array('h', chunk(file['buffer'], b'data'))
                expected = array.array('f', (sample / 32768 for sample in pcm)).tobytes()
                assert chunk(wav_bank[file['name'][:-4]], b'data') == expected
            if codecs:
                imported = client.run('import_stem_bundle', {'library': 0, 'path': source.name, 'preferred_format': 'mp3'})
                mp3_bank = bank(imported)
                assert set(mp3_bank) == set(wav_bank)
                assert any(chunk(mp3_bank[kind], b'data') != chunk(wav_bank[kind], b'data') for kind in wav_bank)
                assert all(len(chunk(mp3_bank[kind], b'data')) == len(chunk(wav_bank[kind], b'data')) for kind in wav_bank)
                fallback = root / 'fallback.zip'
                fallback.write_bytes(zipped([file for file in files[:6] if file['name'] != 'vocals.wav'])['buffer'])
                imported = client.run('import_stem_bundle', {'library': 0, 'path': fallback.name})
                assert len(bank(imported)) == 3
            many = root / '12 parts.zip'
            many.write_bytes(zipped([fixture(f'guitar take {index:02}.wav', 200 + index * 100) for index in range(12)])['buffer'])
            inspected = client.call('inspect_stem_bundle', {'library': 0, 'path': many.name})
            assert inspected['max_selected_parts'] is None and len(inspected['parts']) == 12
            all_parts = client.run('import_stem_bundle', {'library': 0, 'path': many.name})
            assert len(bank(all_parts)) == 12
            selected = client.run('import_stem_bundle', {'library': 0, 'path': many.name, 'parts': [{'part_index': index} for index in range(3, 12)]})
            assert len(bank(selected)) == 9
            preview = selected['preview']['studio_preview_id']
            (root / 'bad.zip').write_bytes(b'bad ZIP')
            client.call('inspect_stem_bundle', {'library': 0, 'path': 'bad.zip'}, error=True)
            failed = client.finish(client.call('import_stem_bundle', {'library': 0, 'path': 'bad.zip'}))
            assert failed['status'] == 'failed'
            assert client.call('get_studio_session')['preview']['studio_preview_id'] == preview
            assert hashlib.sha256(source.read_bytes()).hexdigest() == original_hash
            return {'tools': len(tools), 'ZIP_discovery_and_nine_pair_inspection': True, 'nine_WAV_parts_sample_exact': True,
                    'actual_MP3_preference_and_fallback': codecs, 'all_twelve_default_and_optional_nine': True,
                    'invalid_bundles_preserve_preview': True, 'original_archive_unchanged': True}
        finally:
            client.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--server', type=Path, required=True)
    parser.add_argument('--codecs', action='store_true')
    args = parser.parse_args()
    print(json.dumps(verify(args.server.resolve(), args.codecs), indent=2))
