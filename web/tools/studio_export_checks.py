"""Independent ZIP/manifest checks for exported raw stem banks."""
import json


def audio_names(archive):
    return {name for name in archive.namelist() if name.endswith('.wav')}


def verify_manifest(archive):
    assert archive.testzip() is None
    names = audio_names(archive)
    assert 2 <= len(names) <= 9
    assert set(archive.namelist()) == names | {'stems.json'}
    manifest = json.loads(archive.read('stems.json'))
    assert manifest['format'] == 'wav32f'
    assert {stem['file'] for stem in manifest['stems']} == names
    assert len(manifest['stems']) == len(names)
    assert len({stem['kind'] for stem in manifest['stems']}) == len(names)
    assert all(isinstance(stem['display_name'], str) and stem['display_name'] for stem in manifest['stems'])
    return manifest
