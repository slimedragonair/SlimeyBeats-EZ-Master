"""Synthetic archive boundary and actual owned ZIP review controls."""
import subprocess
import zipfile

from playwright.sync_api import expect


def make_archive(out, sources, ffmpeg, filler_mib):
    mp3 = out / 'archive-alternate.mp3'
    subprocess.run([str(ffmpeg), '-nostdin', '-v', 'error', '-i', str(sources[0]), str(mp3)], check=True, capture_output=True)
    target = out / 'synthetic-parts.zip'
    with zipfile.ZipFile(target, 'w', compression=zipfile.ZIP_STORED) as archive:
        archive.write(sources[0], 'WAV/Lead.wav'); archive.write(mp3, 'MP3/Lead.mp3'); archive.write(sources[1], 'WAV/Pad.wav')
        if filler_mib:
            with archive.open('unselected-notes.bin', 'w') as filler:
                for _ in range(filler_mib): filler.write(bytes(1024**2))
    return target


def verify_archive(page, a, row, archive, snapshot, ready, edit, measure):
    saved = snapshot(); document = saved['arrangement']
    upload_mark = page.evaluate('()=>arrangementRemoteAudit.uploads.length')
    a('zip').set_input_files(str(archive))
    expect(a('import-parts').locator('input[type=checkbox]')).to_have_count(2, timeout=120000)
    expect(a('import-commit')).to_be_enabled(timeout=30000)
    assert snapshot()['revision'] == saved['revision'] and snapshot()['arrangement'] == document
    rows = a('import-parts').locator('.arr-import-part')
    lead = rows.filter(has_text='Lead'); pad = rows.filter(has_text='Pad')
    expect(lead).to_have_count(1); expect(pad).to_have_count(1)
    assert lead.locator('select option').count() == 2
    assert 'WAV' in lead.locator('select option:checked').inner_text()
    lead.locator('input').uncheck(); expect(pad.locator('input')).to_be_checked(); lead.locator('input').check()
    current = edit('import-commit')
    assert len(current['arrangement']['lanes']) == len(document['lanes']) + 2
    added = current['arrangement']['lanes'][-2:]
    assert {lane['name']: lane['clips'][0]['end_frame'] for lane in added} == {'Lead': 960000, 'Pad': 576000}
    assert len(current['arrangement']['eq']) == len(document['eq'])
    uploads = page.evaluate('mark=>arrangementRemoteAudit.uploads.slice(mark)', upload_mark)
    assert len(uploads) == 1 and uploads[0]['role'] == 'arrangement_archive' and uploads[0]['bytes'] == archive.stat().st_size, uploads
    previous = current['revision']; a('undo').click(); undone = ready(previous)
    assert undone['arrangement'] == document
    previous = undone['revision']; a('redo').click(); current = ready(previous)
    assert len(current['arrangement']['lanes']) == len(document['lanes']) + 2
    # Existing tracks are muted through the same real edit controls, isolating
    # admitted archive audio for the actual compressed-output measurement.
    for lane in document['lanes']:
        a('lane-select').select_option(lane['id'])
        previous = snapshot()['revision']
        a('rows').locator(f'[data-lane-id="{lane["id"]}"]').get_by_role('button', name=f'Mute {lane["name"]}', exact=True).click()
        current = ready(previous)
    page.locator('#s-stop').click(); page.locator('#s-play').click()
    expect(page.locator('#s-play')).to_have_text('Pause', timeout=45000)
    value = measure('server archive PCM'); page.locator('#s-stop').click()
    assert abs(value['first'] / value['second'] - 1) < .04 and value['first'] > .025, value
    summary = page.evaluate('()=>arrangementRemoteAudit.summary()')
    assert summary['largeBlobReads'] == 0 and summary['fullMediaReads'] == 0
    row['checks'].append('server ZIP review uploads once in chunks, uses WAV/MP3 owned versions, keeps unequal durations and adds selected sources in one Undo step')
    row['checks'].append('archive source playback is real compressed PCM and uses no full archive read or phone ZIP decode')
    row['archive_bytes'] = archive.stat().st_size
    return current
