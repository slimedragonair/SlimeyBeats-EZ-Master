"""Real server sources above the local PCM budget, with bounded fixture writes."""
import wave

import numpy as np
from playwright.sync_api import expect


def make_budget_sources(out):
    result = []
    for name, hz in [('long-lead', 440), ('long-pad', 880)]:
        path = out / f'{name}.wav'
        mono = np.round(.02 * 32767 * np.sin(np.arange(9600) * (2 * np.pi * hz / 48000))).astype('<i2')
        block = np.column_stack([mono, mono]).tobytes()
        with wave.open(str(path), 'wb') as writer:
            writer.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
            for _ in range(900): writer.writeframesraw(block)
        result.append(path)
    return result


def add_budget_sources(page, a, row, paths, snapshot, edit):
    before = snapshot()
    a('files').set_input_files([str(path) for path in paths])
    expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(2, timeout=120000)
    current = edit('import-commit')
    assert len(current['arrangement']['lanes']) == len(before['arrangement']['lanes']) + 2
    total = sum(asset['frames'] * asset['channels'] * 4 for asset in current['assets'])
    assert total > 128 * 1024**2
    row['decoded_server_source_bytes'] = total
    return current
