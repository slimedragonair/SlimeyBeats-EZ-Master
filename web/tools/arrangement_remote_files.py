"""Explicit remote downloads and project validation through real Studio controls."""
import json
import subprocess
import zipfile

import numpy as np
from playwright.sync_api import expect
from verify_arrangement_project_ui import float_wav


def measure_delivery(path, format, ffmpeg):
    if format == 'wav32f':
        pcm, rate = float_wav(path.read_bytes())
    else:
        raw = subprocess.run([str(ffmpeg), '-v', 'error', '-i', str(path), '-f', 'f32le', '-acodec', 'pcm_f32le', '-'], capture_output=True, check=True).stdout
        pcm, rate = np.frombuffer(raw, dtype='<f4').reshape(-1, 2), 48000
    assert rate == 48000 and pcm.shape[1] == 2 and np.isfinite(pcm).all()
    tone = lambda hz: 2 * abs(np.sum(pcm[96000:105600, 0] * np.exp(-2j*np.pi*hz*np.arange(9600)/rate))) / 9600
    return {'frames': len(pcm), 'first': tone(440), 'second': tone(880), 'rate': rate}


def verify_files(page, a, row, out, width, ffmpeg, snapshot, ready, edit):
    saved = snapshot(); document = saved['arrangement']
    a('eq-suggest').click()
    expect(a('eq-apply-proposal')).to_be_visible(timeout=120000)
    expect(a('eq-suggest')).to_be_enabled(timeout=30000)
    assert snapshot()['revision'] == saved['revision'] and snapshot()['arrangement'] == document
    assert not a('eq-details').evaluate('element=>element.open')
    a('eq-details').locator('summary').click(); expect(a('eq-notes')).to_be_visible()
    assert len(a('eq-notes').inner_text()) > 40
    a('eq-details').locator('summary').click()
    row['checks'].append('owned scoped suggestion returns full expandable notes and a ghost proposal without applying or changing history')

    a('file-actions').locator('summary').click()
    with page.expect_download(timeout=120000) as pending: a('project-save').click()
    project = out / f'{width}-server-project.zip'; pending.value.save_as(str(project))
    expect(a('project-save')).to_be_enabled(timeout=30000)
    with zipfile.ZipFile(project) as archive:
        manifest = json.loads(archive.read('arrangement.json'))
        assert manifest['version'] == 2 and manifest['history']['current'] == document
        assert {source['id'] for source in manifest['sources']} == {asset['id'] for asset in saved['assets']}
        assert manifest['history']['undo'] and manifest['undo_labels']
    assert snapshot()['arrangement'] == document

    for format, suffix in [('wav32f', 'wav'), ('mp3', 'mp3')]:
        a('audio-format').select_option(format)
        with page.expect_download(timeout=120000) as pending: a('audio-export').click()
        path = out / f'{width}-server.{suffix}'; pending.value.save_as(str(path))
        expect(a('audio-export')).to_be_enabled(timeout=30000)
        if format == 'wav32f':
            pcm, rate = float_wav(path.read_bytes()); assert len(pcm) == 960000
        else:
            decoded = subprocess.run([str(ffmpeg), '-v', 'error', '-i', str(path), '-f', 'f32le', '-acodec', 'pcm_f32le', '-'], capture_output=True, check=True).stdout
            pcm, rate = np.frombuffer(decoded, dtype='<f4').reshape(-1, 2), 48000
            assert 960000 <= len(pcm) <= 972000
        assert rate == 48000 and pcm.shape[1] == 2 and np.isfinite(pcm).all()
        tone = lambda hz: 2 * abs(np.sum(pcm[48000:57600, 0] * np.exp(-2j*np.pi*hz*np.arange(9600)/rate))) / 9600
        first, second = tone(440), tone(880)
        assert abs(first - .06 * 10 ** (-18/20)) < .0007 and abs(second - .06) < .0007, (first, second)
        row['measurements'][f'server {format} download'] = {'frames': len(pcm), 'first': first, 'second': second}
        assert snapshot()['arrangement'] == document
    row['checks'].append('explicit WAV and MP3 downloads stream to disk and independently decode with applied scoped EQ/gain and no document changes')

    revision = snapshot()['revision']; deleted = edit('delete-clip')
    assert not deleted['arrangement']['lanes'][0]['clips']
    previous = deleted['revision']; a('project-file').set_input_files(str(project)); restored = ready(previous)
    assert restored['arrangement'] == document and restored['epoch'] != saved['epoch']
    assert restored['history']['can_undo'] and {asset['id'] for asset in restored['assets']} == {asset['id'] for asset in saved['assets']}
    expect(page.locator('#s-play')).to_have_text('Play')
    previous = restored['revision']; a('undo').click(); undone = ready(previous)
    previous = undone['revision']; a('redo').click(); restored = ready(previous)
    assert restored['arrangement'] == document and restored['revision'] > revision
    row['checks'].append('canonical server project restores source/effect IDs and usable Undo/Redo with a new epoch and paused transport')

    damaged = out / f'{width}-server-corrupt.zip'; contents = bytearray(project.read_bytes()); contents[70] ^= 1; damaged.write_bytes(contents)
    a('project-file').set_input_files(str(damaged))
    expect(a('status')).to_have_class('arr-status error', timeout=120000)
    expect(a('project-open')).to_be_enabled(timeout=30000)
    assert snapshot()['arrangement'] == document and snapshot()['revision'] == restored['revision']
    row['checks'].append('corrupt server Open reports failure and preserves the current document and source attachments')
    return restored


def verify_guest_limit(page, a, row, snapshot):
    saved = snapshot()
    remaining = page.evaluate('()=>arrangementRemoteAudit.capabilities.limits.guest_jobs_per_session-arrangementRemoteAudit.jobs.filter(value=>!["review_archive","commit_import"].includes(value.operation)).length')
    assert 0 <= remaining <= 3
    for _ in range(remaining):
        a('eq-suggest').click()
        expect(a('eq-suggest')).to_be_enabled(timeout=120000)
        expect(a('eq-apply-proposal')).to_be_visible(timeout=30000)
    a('eq-suggest').click()
    expect(a('status')).to_contain_text('guest session has used its processing allowance', timeout=30000)
    expect(a('eq-suggest')).to_be_enabled(timeout=30000)
    expect(page.locator('#hybrid-availability')).to_have_text('[Server online]')
    assert snapshot()['revision'] == saved['revision'] and snapshot()['arrangement'] == saved['arrangement']
    row['checks'].append('an advertised three-job fixture quota reports an exhausted guest session without claiming temporary server busy or losing applied edits')
