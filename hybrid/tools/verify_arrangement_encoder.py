"""Measure the real preview encoder's start alignment at every project rate."""
import argparse
import array
import json
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from hybrid.server.arrangement_stream import encoder_arguments, PREVIEW_PRIMING_FRAMES


def mp4_timing(ffmpeg, encoded):
    def descriptor(offset):
        tag, length = encoded[offset], 0
        offset += 1
        for _ in range(4):
            value = encoded[offset]; offset += 1
            length = length * 128 + (value & 127)
            if value < 128:
                return tag, offset, length
        raise AssertionError('Invalid MP4 ES descriptor')
    tag, first, _ = descriptor(encoded.index(b'esds') + 8)
    assert tag == 3 and encoded[first + 2] == 0
    tag, first, _ = descriptor(first + 3)
    assert tag == 4 and encoded[first] == 0x6B
    ffprobe = ffmpeg.with_name('ffprobe' + ffmpeg.suffix)
    metadata = json.loads(subprocess.run([str(ffprobe), '-v', 'error', '-select_streams', 'a:0', '-show_streams', '-show_packets',
        '-show_entries', 'stream=codec_name,codec_tag_string,time_base:packet=pts,dts,duration', '-of', 'json', '-i', 'pipe:0'],
        input=encoded, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True).stdout)
    stream = metadata['streams'][0]
    assert stream['codec_name'] == 'mp3' and stream['codec_tag_string'] == 'mp4a' and stream['time_base'] == '1/48000'
    for index, packet in enumerate(metadata['packets']):
        assert (packet['pts'], packet['dts']) == (index * 1152, index * 1152), packet
        assert 0 < packet['duration'] <= 1152
        assert index == len(metadata['packets']) - 1 or packet['duration'] == 1152
    return {'codec_object_type': '0x6b', 'packet_count': len(metadata['packets']), 'packet_timestamp_step': 1152,
            'last_packet_duration': metadata['packets'][-1]['duration']}


def verify(ffmpeg, container='mp3'):
    results = []
    for rate in (8000, 32000, 44100, 48000, 96000):
        pcm = array.array('f', [0.0] * (rate // 2 * 2))
        pcm[0] = pcm[1] = 0.5
        if sys.byteorder != 'little':
            pcm.byteswap()
        command = encoder_arguments(SimpleNamespace(ffmpeg=ffmpeg, v2_preview_kbps=192),
                                    {'sample_rate': rate, 'channels': 2, 'preview_container': container})
        encoded = subprocess.run([str(value) for value in command], input=pcm.tobytes(),
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True).stdout
        decoded = subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', 'pipe:0',
                                  '-f', 'f32le', '-ar', '48000', '-ac', '2', 'pipe:1'], input=encoded,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10, check=True).stdout
        samples = array.array('f')
        samples.frombytes(decoded)
        if sys.byteorder != 'little':
            samples.byteswap()
        left = samples[::2]
        peak = max(range(len(left)), key=lambda index: abs(left[index]))
        assert abs(peak - PREVIEW_PRIMING_FRAMES) <= 1, (rate, peak)
        assert len(left) >= 24000 + PREVIEW_PRIMING_FRAMES
        assert abs(left[peak]) > 0.01
        row = {'project_rate': rate, 'preview_rate': 48000, 'impulse_peak_frame': peak}
        if container == 'mp4':
            row.update(mp4_timing(ffmpeg, encoded))
        results.append(row)
    return {'passed': True, 'container': container, 'rates': results, 'scope': 'encoder alignment; browser buffering is qualified separately'}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--ffmpeg', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--container', choices=('mp3', 'mp4'), default='mp3')
    args = parser.parse_args()
    result = verify(args.ffmpeg.resolve(strict=True), args.container)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    main()
