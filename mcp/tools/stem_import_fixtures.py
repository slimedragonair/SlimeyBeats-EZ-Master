"""Small deterministic WAV/ZIP fixtures for resource-bounded stem-import checks."""
import io
import math
import struct
import wave
import zipfile


LEGACY_KINDS = [
    'vocals', 'instrumental', 'drums', 'bass', 'other', 'guitar', 'piano',
    'synth', 'strings', 'percussion', *[f'part_{i}' for i in range(1, 10)],
]


def kinds(count):
    result = LEGACY_KINDS + ['part_10', 'part_24', 'part_64', 'part_1000', 'part_4294967295']
    index = 11
    while len(result) < count:
        kind = f'part_{index}'
        if kind not in result:
            result.append(kind)
        index += 1
    return result[:count]


def wav_fixture(index, frames=1600, sample_rate=8000):
    samples = [int(100 * math.sin(frame * (100 + index * 13) * math.tau / sample_rate))
               for frame in range(frames)]
    output = io.BytesIO()
    with wave.open(output, 'wb') as wav:
        wav.setparams((1, 2, sample_rate, 0, 'NONE', ''))
        wav.writeframes(struct.pack(f'<{len(samples)}h', *samples))
    expected_float = struct.pack(f'<{len(samples)}f', *(sample / 32768 for sample in samples))
    return output.getvalue(), expected_float


def write_zip(path, entries):
    with zipfile.ZipFile(path, 'w', compression=zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries:
            archive.writestr(name, data)


def wav_data(data):
    assert data[:4] == b'RIFF' and data[8:12] == b'WAVE'
    offset = 12
    while offset + 8 <= len(data):
        size = struct.unpack_from('<I', data, offset + 4)[0]
        if data[offset:offset + 4] == b'data':
            return data[offset + 8:offset + 8 + size]
        offset += 8 + size + (size & 1)
    raise AssertionError('Export has no WAV data chunk')
