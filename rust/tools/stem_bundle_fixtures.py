"""Synthetic paired encodings shared by browser and native/MCP checks."""
import io
from pathlib import Path
import struct
import subprocess
import tempfile
import zipfile

KINDS = ['vocals', 'drums', 'bass', 'guitar', 'piano', 'synth', 'strings', 'percussion', 'other']


def chunk(wav, key):
    offset = 12
    while offset + 8 <= len(wav):
        kind, size = struct.unpack_from('<4sI', wav, offset)
        if kind == key:
            return wav[offset + 8:offset + 8 + size]
        offset += 8 + size + size % 2
    raise AssertionError(key)


def suno_fixture(fixture, codecs=True):
    files = []
    with tempfile.TemporaryDirectory(prefix='Suno paired stems QA ') as temporary:
        root = Path(temporary)
        for index, kind in enumerate(KINDS):
            source = fixture(kind + '.wav', 200 + index * 150)
            wave = root / source['name']
            wave.write_bytes(source['buffer'])
            compressed = wave.with_suffix('.mp3')
            # A seekable MP3 gets real encoder delay/padding metadata.
            if not codecs:
                files.extend([source, {'name': compressed.name, 'mimeType': 'audio/mpeg', 'buffer': b'Undecoded MP3 alternate fixture'}])
                continue
            subprocess.run(['ffmpeg', '-v', 'error', '-y', '-i', str(wave), '-codec:a', 'libmp3lame', '-b:a', '192k', str(compressed)], check=True, capture_output=True, timeout=30)
            files.extend([source, {'name': compressed.name, 'mimeType': 'audio/mpeg', 'buffer': compressed.read_bytes()}])
    return files


def zipped(files, prefix=''):
    output = io.BytesIO()
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        for file in files:
            extension = file['name'].rsplit('.', 1)[-1]
            archive.writestr(prefix + extension.upper() + '/' + file['name'], file['buffer'])
        archive.writestr('cover.png', b'ignored image fixture')
        archive.writestr('README.txt', b'metadata, never instructions')
    return {'name': 'Suno stem set.zip', 'mimeType': 'application/zip', 'buffer': output.getvalue()}
