"""Editing-origin and worker isolation checks for web recovery."""
from playwright.sync_api import expect
from verify_studio_browser import fixture


def studio_recovery(browser, base, out, ready, open_recovery, save_point):
    context = browser.new_context(viewport={'width': 1480, 'height': 1000}, accept_downloads=True)
    context.add_init_script("localStorage.setItem('slimeybeats.interface-preferences.tutorial-dismissed','declined')")
    page = context.new_page(); page.on('dialog', lambda d: d.accept())
    page.goto(base, wait_until='networkidle'); ready(page)
    page.locator('#controls-advanced').click()
    open_recovery(page); page.locator('#recovery-enable').click(); page.locator('#recovery-close').click()
    page.locator('#tab-studio-tools').click()
    page.locator('.edit-import > summary').click()
    for kind, hz in [('vocals', 220), ('instrumental', 660)]:
        page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(kind + '.wav', hz, seconds=.6))
    page.locator('#s-import').click()
    expect(page.locator('#s-export')).to_be_enabled(timeout=60000)
    page.locator('#s-target').select_option('vocals')
    page.locator('#s-tab-voice').click(); page.locator('#s-highpass').fill('150'); page.locator('#s-voice').click()
    expect(page.locator('#s-apply-stem')).to_be_enabled(timeout=60000)
    page.locator('#s-apply-stem').click(); expect(page.locator('#s-export')).to_be_enabled(timeout=60000)

    def save(selector, name):
        with page.expect_download(timeout=60000) as event: page.locator(selector).click()
        event.value.save_as(str(out / name))
        expect(page.locator('#s-export')).to_be_enabled(timeout=60000)
        return (out / name).read_bytes()

    before = save('#s-export', 'studio-before.wav')
    bank = save('#s-export-stems', 'studio-bank-before.zip')
    # Recover both workspaces together, with different original recordings.
    page.locator('#file').set_input_files(fixture('Independent mastering mix.wav', 880, seconds=.3))
    expect(page.locator('#master')).to_be_enabled(); page.locator('#master').click()
    expect(page.locator('#export')).to_be_enabled(timeout=60000)
    page.locator('#tab-studio-tools').click()
    page.locator('#s-tab-mix').click()
    page.get_by_role('checkbox', name='vocals mute', exact=True).check()
    open_recovery(page); save_point(page)
    page.reload(wait_until='networkidle'); ready(page)
    page.locator('#recovery-offer-open').click(); page.locator('#recovery-points button').first.click()
    expect(page.locator('#recovery-action-status')).to_contain_text('Restored with playback stopped', timeout=120000)
    page.locator('#recovery-close').click(); page.locator('#tab-studio-tools').click()
    expect(page.locator('#export')).to_be_enabled()
    expect(page.locator('#s-play')).to_have_text('Play')
    expect(page.get_by_role('checkbox', name='vocals mute', exact=True)).to_be_checked()
    expect(page.locator('#s-pending')).to_contain_text('Stem changes are live')
    expect(page.locator('#s-export')).to_be_disabled()
    expect(page.locator('#s-highpass')).to_have_value('150')
    with page.expect_download(timeout=60000) as event: page.locator('#s-export-stems').click()
    event.value.save_as(str(out / 'studio-bank-restored.zip'))
    assert (out / 'studio-bank-restored.zip').read_bytes() == bank
    page.get_by_role('checkbox', name='vocals mute', exact=True).uncheck()
    page.locator('#s-mix').click(); expect(page.locator('#s-export')).to_be_enabled(timeout=60000)
    assert save('#s-export', 'studio-restored.wav') == before
    page.locator('#s-tab-voice').click(); page.locator('#s-target').select_option('instrumental')
    page.locator('#s-voice').click(); expect(page.locator('#s-apply-stem')).to_be_enabled(timeout=60000)
    page.locator('#s-apply-stem').click(); expect(page.locator('#s-export')).to_be_enabled(timeout=60000)
    page.screenshot(path=str(out / 'studio-recovered.png'))
    context.close()
    return ['both workspaces restore together: independent mastering mix plus exact edited Studio bank/preview, pending mute and tool controls retained, recovered stems remain editable']


def import_race(browser, base, out, ready, open_recovery):
    context = browser.new_context(accept_downloads=True)
    context.add_init_script("localStorage.setItem('slimeybeats.interface-preferences.tutorial-dismissed','declined')")
    page = context.new_page(); page.on('dialog', lambda d: d.accept())
    page.goto(base, wait_until='networkidle'); ready(page)
    open_recovery(page)
    # Stage a saved master in a separate worker and hold the actual load reply.
    page.evaluate('''async () => {
        const resource = performance.getEntriesByType('resource').find(e => e.name.includes('/dsp-client.js')).name;
        const Client = (await import(resource)).DspClient, call = Client.prototype.call;
        window.restoreLoadStarted = false;
        Client.prototype.call = function(action, ...args) {
            const result = call.call(this, action, ...args);
            if (action !== 'load') return result;
            Client.prototype.call = call;
            return result.then(value => new Promise(resolve => {
                window.restoreLoadStarted = true;
                window.releaseRestoreLoad = () => resolve(value);
            }));
        };
    }''')
    page.locator('#recovery-file').set_input_files(str(out / 'manual-off.sbrecovery'))
    page.wait_for_function('() => window.restoreLoadStarted === true')
    # Real decoding/loading starts after recovery. Its worker must remain owner.
    page.locator('#file').set_input_files(fixture('NEWER_IMPORT.wav', 880, seconds=.4))
    expect(page.locator('#filename')).to_have_text('NEWER_IMPORT.wav', timeout=30000)
    page.evaluate('window.releaseRestoreLoad()')
    expect(page.locator('#recovery-action-status')).to_contain_text('New work arrived', timeout=120000)
    expect(page.locator('#filename')).to_have_text('NEWER_IMPORT.wav')
    page.locator('#recovery-close').click(); page.locator('#master').click()
    expect(page.locator('#export')).to_be_enabled(timeout=120000)
    context.close()
    return ['real in-flight restore vs newer audio import: newer filename/audio/worker/job retained and still masters']


def chromium_crash(browser, base, ready, open_recovery, save_point, *, proof=None):
    from recovery_process_check import process_crash
    assert proof is not None, 'A fresh crash proof destination is required'
    return process_crash(browser.browser_type, base, ready, open_recovery, save_point, proof)
