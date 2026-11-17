"""Real local canonical continuation with unapplied UI fields retained."""
from playwright.sync_api import expect


def prepare_local(page, a, sources):
    a('files').set_input_files([str(source) for source in sources]); a('import-commit').click()
    expect(page.locator('#s-play')).to_be_enabled(timeout=30000)
    snapshot = lambda: page.evaluate('()=>arrangementAudit.latest')
    def edit(name):
        previous = snapshot()['revision']; a(name).click()
        page.wait_for_function('rev=>arrangementAudit.latest.revision>rev&&!arrangementAudit.latest.preview.pending', arg=previous, timeout=30000)
        expect(a('add-files')).to_be_enabled(timeout=30000)
    lane = snapshot()['arrangement']['lanes'][0]
    a('lane-select').select_option(lane['id']); a('clip-select').select_option(lane['clips'][0]['id'])
    a('clip-gain').fill('-1'); edit('clip-level')
    a('eq-scope').select_option('range'); a('eq-range-target').select_option('clip')
    a('range-start').fill('10'); a('range-end').fill('11'); a('range-end').press('Tab')
    a('eq-band').select_option('2'); a('eq-frequency').fill('440'); a('eq-shape').select_option('bell'); a('eq-q').fill('1'); a('eq-gain').fill('-3'); edit('eq-update')
    saved = snapshot()
    a('clip-name').fill('Unapplied lead name'); a('clip-start').fill('0.125'); a('eq-gain').fill('-2')
    a('lane-inspector').locator('summary').click(); a('lane-name').fill('Unapplied track name')
    page.locator('#s-playback-details > summary').click(); page.locator('#s-seek').fill('2.12')
    page.wait_for_function('()=>arrangementAudit.preparations.at(-1)?.position_frame===101760', timeout=30000)
    return saved


def assert_pending(a):
    for name, value in [('clip-name', 'Unapplied lead name'), ('clip-start', '0.125'), ('eq-gain', '-2'), ('lane-name', 'Unapplied track name')]:
        expect(a(name)).to_have_value(value)


def assert_migrated(page, a, saved, current, row):
    assert current['arrangement'] == saved['arrangement']
    identity = lambda value: {(asset['id'], asset['frames'], asset['sample_rate'], asset['channels']) for asset in value['assets']}
    assert identity(current) == identity(saved)
    assert current['history']['can_undo'] and current['history']['undo_label'] == saved['history']['undo_label']
    assert_pending(a); expect(page.locator('#s-play')).to_have_text('Play')
    expect(page.locator('#s-seek')).to_have_value('2.12')
    assert current['arrangement']['eq'][0]['range'] == {'start_frame': 480000, 'end_frame': 528000}
    assert current['arrangement']['eq'][0]['bands'][2]['gain'] == -3
    assert current['arrangement']['lanes'][0]['clips'][0]['start_frame'] == 0
    assert len(page.evaluate('()=>arrangementRemoteAudit.uploads.filter(value=>value.role==="arrangement_project")')) == 1
    row['checks'].append('explicit Arrange tracks Server continues the canonical applied document, IDs, source-frame EQ, history, paused position and raw unfinished fields')
