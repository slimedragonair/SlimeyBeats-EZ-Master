"""CC0 synthetic immutable Float32 pool and canonical v2 native project fixture."""
import copy
import hashlib
import json
import math
import struct


def create(root):
    root.mkdir(parents=True, exist_ok=False)
    pool = root / 'assets'; pool.mkdir()
    rate, sources, lanes = 48000, [], []
    for index, (name, hz, seconds, channels, start) in enumerate([
        ('Lead vocal', 440, 12, 1, 0), ('Drums and percussion', 180, 18, 2, 2),
        ('Guitar layers', 220, 15, 2, 5), ('Atmosphere', 83, 20, 2, 0),
    ]):
        pcm = bytearray()
        for frame in range(rate * seconds):
            t = frame / rate
            envelope = min(1, t * 5, (seconds - t) * 5) * (.4 + .6 * math.sin(t * math.pi * 2) ** 2)
            for channel in range(channels):
                value = envelope * (.16 * math.sin(t * 2 * math.pi * hz + channel * .2)
                                    + .04 * math.sin(t * 2 * math.pi * (hz * 3.01)))
                pcm.extend(struct.pack('<f', value))
        fmt = struct.pack('<HHIIHH', 3, channels, rate, rate * channels * 4, channels * 4, 32)
        wav = b'RIFF' + struct.pack('<I', 36 + len(pcm)) + b'WAVEfmt ' + struct.pack('<I', 16) + fmt + b'data' + struct.pack('<I', len(pcm)) + pcm
        sha = hashlib.sha256(wav).hexdigest(); filename = f'assets/{sha}.wav'
        (root / filename).write_bytes(wav)
        source_id, lane_id, clip_id = f'source_{index}', f'lane_{index}', f'clip_{index}'
        sources.append(dict(id=source_id, name=name, file=filename, identity=dict(bytes=len(wav), sha256=sha),
                            frames=rate * seconds, sample_rate=rate, channels=channels))
        lanes.append(dict(id=lane_id, name=name, gain_db=-2 if index else 0, pan=0, mute=False, solo=False,
                          clips=[dict(id=clip_id, name=f'{name} take', source_id=source_id,
                                      start_frame=start * rate, source_start_frame=0, end_frame=(start + seconds) * rate,
                                      gain_db=0, fade_in_frames=2400, fade_out_frames=4800)]))
    before = dict(version=1, sample_rate=rate, channels=2, lanes=lanes, eq=[])
    bands = [dict(type='low_shelf' if i == 0 else 'high_shelf' if i == 6 else 'bell', freq=hz, gain=0, q=.707, on=True)
             for i, hz in enumerate([50, 120, 300, 800, 2000, 5000, 10000])]
    bands[-1]['gain'] = -1.5
    current = copy.deepcopy(before)
    current['eq'] = [dict(id='section_eq', scope=dict(kind='mix'), range=dict(start_frame=8 * rate, end_frame=12 * rate),
                          bands=bands, output_gain_db=0, bypass=False)]
    manifest = dict(format='slimeybeats-studio', version=2,
                    history=dict(current=current, undo=[before], redo=[], capacity=32), sources=sources,
                    render_options=dict(output_gain_db=0, protect_peaks=True, ceiling_dbtp=-1, max_output_bytes=512 * 1024 * 1024),
                    origin=before, before=before, undo_labels=[], redo_labels=[])
    path = root / 'project.json'; path.write_text(json.dumps(manifest, indent=2), encoding='utf-8')
    return path
