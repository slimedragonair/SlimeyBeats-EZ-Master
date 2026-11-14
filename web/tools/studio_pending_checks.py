"""Pending mix controls must survive every rejected browser Studio action."""
import io
import zipfile

from playwright.sync_api import expect


def verify_pending_mix(page, ready, save, raw_before, out, capture):
    # Observe the real client and playback; neither processing nor downloads are mocked.
    page.evaluate('''async () => {
        const resource = name => performance.getEntriesByType('resource').find(e => e.name.includes('/' + name)).name;
        const {StudioClient} = await import(resource('studio-client.js'));
        const {StudioPlayback} = await import(resource('studio-player.js'));
        const call = StudioClient.prototype.call, setAudio = StudioPlayback.prototype.setAudio;
        const probe = window.pendingMixProbe = {calls: []};
        StudioClient.prototype.call = function(action, ...args) {
            probe.calls.push(action);
            return call.call(this, action, ...args);
        };
        StudioPlayback.prototype.setAudio = function(...args) {
            probe.playback = this;
            return setAudio.apply(this, args);
        };
        probe.restore = () => {
            StudioClient.prototype.call = call;
            StudioPlayback.prototype.setAudio = setAudio;
            delete window.pendingMixProbe;
        };
    }''')
    try:
        return check_pending_mix(page, ready, save, raw_before, out, capture)
    finally:
        page.evaluate('() => window.pendingMixProbe.restore()')


def check_pending_mix(page, ready, save, raw_before, out, capture):
    page.locator('#s-target').select_option('vocals')
    page.locator('#s-tab-voice').click()
    page.locator('#s-highpass').fill('150')
    page.locator('#s-voice').click()
    ready()
    expect(page.locator('#s-apply-stem')).to_be_enabled()
    old_preview = save('#s-export', 'pending-old-preview.wav')
    page.evaluate('''() => {
        const p = pendingMixProbe;
        p.before = p.playback.before;
        p.after = p.playback.after;
        p.controls = () => [...document.querySelectorAll('#s-faders input')].map(n => [n.value, n.checked]);
    }''')
    page.locator('#s-tab-mix').click()
    page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('-9')
    page.get_by_role('checkbox', name='instrumental mute', exact=True).check()
    page.get_by_role('checkbox', name='vocals solo', exact=True).check()
    pending_controls = page.evaluate('() => pendingMixProbe.controls()')

    def pending():
        expect(page.locator('#s-pending')).to_be_visible()
        expect(page.locator('#s-pending')).to_contain_text('Stem mix → Update mix preview')
        assert page.evaluate('() => pendingMixProbe.controls()') == pending_controls
        assert page.evaluate('() => pendingMixProbe.before === pendingMixProbe.playback.before && pendingMixProbe.after === pendingMixProbe.playback.after')
        for action in ['voice', 'repair', 'apply-stem', 'export', 'promote', 'undo', 'redo']:
            expect(page.locator('#s-' + action)).to_be_disabled()
        expect(page.locator('#s-export-stems')).to_be_enabled()

    pending()
    for tab, action in [('voice', 'voice'), ('repair', 'repair'),
                        ('repair', 'apply-stem'), ('repair', 'export'),
                        ('repair', 'promote'), ('repair', 'undo'), ('repair', 'redo')]:
        page.locator('#s-tab-' + tab).click()
        calls = page.evaluate('() => pendingMixProbe.calls.length')
        # Invoke onclick directly: disabled controls alone cannot pass this check.
        page.evaluate('async id => { await document.getElementById("s-" + id).onclick(); }', action)
        assert page.evaluate('() => pendingMixProbe.calls.length') == calls, action
        expect(page.locator('#s-status')).to_contain_text('before further edits or saving')
        expect(page.locator('#studio-tools')).to_be_visible()
        pending()
    capture(out / 'pending-repair-guard.png', full_page=True)

    # Exercise mutate itself through the import callback, which uses the shared job.
    calls = page.evaluate('() => pendingMixProbe.calls.length')
    page.evaluate('async () => { await document.getElementById("s-import").onclick(); }')
    assert page.evaluate('() => pendingMixProbe.calls.length') == calls
    expect(page.locator('#s-status')).to_contain_text('before further edits or saving')
    pending()

    page.locator('#s-target').select_option('source')
    page.locator('#analyze').click()
    expect(page.locator('#s-status')).to_contain_text('LUFS integrated', timeout=30000)
    expect(page.locator('#s-cancel')).to_be_disabled()
    pending()
    page.locator('#s-spectrum').click()
    expect(page.locator('#s-status')).to_contain_text('graph shows', timeout=30000)
    expect(page.locator('#s-cancel')).to_be_disabled()
    pending()
    with page.expect_download(timeout=120000) as download:
        page.locator('#s-export-stems').click()
    raw_path = out / 'pending-raw-stems.zip'
    download.value.save_as(str(raw_path))
    expect(page.locator('#s-cancel')).to_be_disabled()
    assert raw_path.read_bytes() == raw_before
    pending()
    page.locator('#s-target').select_option('vocals')

    # Cancellation must also retain the pending faders.
    page.locator('#s-tab-mix').click()
    page.evaluate('() => { document.getElementById("s-mix").click(); document.getElementById("s-cancel").click(); }')
    expect(page.locator('#s-status')).to_contain_text('Cancelled', timeout=30000)
    expect(page.locator('#s-cancel')).to_be_disabled()
    pending()

    page.locator('#s-mix').click()
    ready()
    expect(page.locator('#s-pending')).to_be_hidden()
    assert page.evaluate('() => pendingMixProbe.controls()') == pending_controls
    rendered = save('#s-export', 'pending-rendered-mix.wav')
    assert rendered != old_preview
    page.locator('#s-undo').click()
    ready()
    assert save('#s-export', 'pending-undo.wav') == old_preview
    page.locator('#s-redo').click()
    ready()
    assert save('#s-export', 'pending-redo.wav') == rendered

    page.locator('#s-tab-voice').click()
    page.locator('#s-highpass').fill('175')
    expect(page.locator('#s-voice')).to_be_enabled()
    page.locator('#s-voice').click()
    ready()
    edited = save('#s-export', 'pending-subsequent-voice.wav')
    assert edited != rendered
    page.locator('#s-apply-stem').click()
    ready()
    raw_after = save('#s-export-stems', 'pending-applied-stems.zip')
    before_bank = zipfile.ZipFile(io.BytesIO(raw_before))
    after_bank = zipfile.ZipFile(io.BytesIO(raw_after))
    assert before_bank.read('vocals.wav') != after_bank.read('vocals.wav')
    assert before_bank.read('instrumental.wav') == after_bank.read('instrumental.wav')
    page.locator('#s-promote').click()
    expect(page.locator('#studio-tools')).to_be_hidden(timeout=60000)
    page.locator('#tab-studio-tools').click()
    ready()

    # Re-import the starting fixtures: Studio intentionally bounds its history.
    page.evaluate('async () => { await document.getElementById("s-import").onclick(); }')
    ready()
    assert save('#s-export-stems', 'pending-restored-stems.zip') == raw_before
    page.locator('#s-highpass').fill('75')
    page.locator('#s-target').select_option('source')
    page.locator('#s-tab-mix').click()
    return ['pending gain/mute/solo survive Voice/Repair switches and direct handler calls; edit/apply/history/export/promote issue no worker requests',
            'shared mutate entry rejects a replacement import; old playback buffers and pending controls remain unchanged',
            'pending mix permits source analysis, spectrogram and byte-identical immutable raw stem ZIP export',
            'cancelled mix retains pending controls; Update mix clears pending, renders new audio and undo restores the exact old preview',
            'after Update mix, voice edits, direct stem apply, preview/raw exports and mastering promotion succeed']
