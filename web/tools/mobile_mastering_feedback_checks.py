"""Verify mobile completion cues, explicit result navigation, and stale-master messaging."""
from playwright.sync_api import expect
from verify_studio_browser import fixture


def verify_mobile_mastering_feedback(browser, url, width=390, height=844, screenshot=None):
    context = browser.new_context(viewport={'width': width, 'height': height}, has_touch=True)
    try:
        page = context.new_page()
        page.goto(url, wait_until='domcontentloaded')
        page.locator('#tutorial-decline').tap()
        analysis = page.locator('#mobile-analysis-feedback')
        master = page.locator('#mobile-master-feedback')
        export = page.locator('#mobile-export')
        expect(analysis).not_to_be_visible()
        expect(master).not_to_be_visible()
        expect(page.locator('#add')).to_be_enabled(timeout=30000)
        page.locator('#file').set_input_files(fixture('mobile-feedback.wav', seconds=3))
        expect(page.locator('#track-card')).to_be_visible(timeout=30000)
        page.evaluate('scrollTo(0, 0)')
        page.locator('#analyze').tap()
        expect(analysis.get_by_text('Analysis ready', exact=True)).to_be_visible(timeout=20000)
        expect(analysis.get_by_role('button', name='View analysis')).to_be_in_viewport()
        assert page.evaluate('scrollY') < 2, 'Analyze must not scroll to its results automatically'
        analysis.get_by_role('button', name='View analysis').tap()
        expect(page.locator('#input-results-heading')).to_be_focused()
        expect(page.locator('#input-results-heading')).to_be_in_viewport()

        if not page.locator('#page-navigation').evaluate('element => element.open'):
            page.locator('#page-navigation-summary').tap()
        page.locator('#tab-eq').tap()
        # Start with the action group visible, as when the user just chose an export format.
        page.locator('#mobile-master').evaluate("element => element.scrollIntoView({block:'start', behavior:'instant'})")
        format_hit = '''() => {
            const format = document.querySelector('#format');
            const r = format.getBoundingClientRect();
            return r.top >= 0 && r.bottom <= document.querySelector('.playback.primary-player').getBoundingClientRect().top &&
                document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === format;
        }'''
        page.wait_for_function(format_hit, timeout=3000)
        page.locator('#mobile-master').tap()
        expect(export).to_be_enabled(timeout=35000)
        expect(master.get_by_text('Master ready', exact=True)).to_be_visible()
        expect(page.locator('#tab-eq')).to_have_attribute('aria-current', 'page')
        expect(page.locator('#play')).to_have_text('Play')
        assert page.evaluate(format_hit), 'Mastering must not push Export format behind the player'
        master.get_by_role('button', name='View master results').focus()
        page.set_viewport_size({'width': 800, 'height': 844})
        expect(master).not_to_be_visible()
        expect(page.locator('#master')).to_be_focused()
        page.set_viewport_size({'width': width, 'height': height})
        master.get_by_role('button', name='View master results').tap()
        expect(page.locator('#output-results-heading')).to_be_focused()
        expect(page.locator('#output-results-heading')).to_be_in_viewport()

        preview_metrics = page.locator('#output-metrics').text_content()
        if not page.locator('#page-navigation').evaluate('element => element.open'):
            page.locator('#page-navigation-summary').tap()
        page.locator('#tab-eq').tap()
        page.locator('#eq-output').fill('-12')
        with page.expect_download(timeout=35000) as saved:
            export.tap()
        assert saved.value.suggested_filename.endswith('.wav')
        expect(page.locator('#status')).to_contain_text('WAV downloaded')
        page.locator('#eq-output').fill('0')
        assert page.locator('#output-metrics').text_content() == preview_metrics, 'Saved-file measurements replaced the preview meters'
        page.locator('#profile').select_option('edm')
        expect(export).to_be_disabled()
        expect(master).to_contain_text('Re-master to export these settings')
        expect(master).to_contain_text('B still plays the previous master')
        expect(page.locator('#output-results-state')).to_have_text('Previous master')
        expect(export).to_have_attribute('aria-describedby', 'mobile-master-message')
        expect(page.locator('#master-source')).to_be_enabled()
        expect(page.locator('#play')).to_have_text('Play')
        if screenshot:
            page.evaluate("document.querySelector('#mobile-export').scrollIntoView({block:'center'})")
            page.screenshot(path=str(screenshot))
        page.evaluate('''async () => {
            const resource = performance.getEntriesByType('resource').find(e => e.name.includes('/dsp-client.js'));
            const {DspClient} = await import(resource.name);
            const call = DspClient.prototype.call;
            DspClient.prototype.call = function(action, ...args) {
                const result = call.call(this, action, ...args);
                if (action !== 'master') return result;
                DspClient.prototype.call = call;
                return result.then(value => new Promise(resolve => { window.releaseMobileMaster = () => resolve(value); }));
            };
        }''')
        page.locator('#mobile-master').tap()
        expect(master).to_contain_text('Re-mastering')
        expect(master).not_to_contain_text('B still plays')
        expect(page.locator('#play')).to_be_disabled()
        page.wait_for_function("() => typeof window.releaseMobileMaster === 'function'")
        page.evaluate('window.releaseMobileMaster()')
        expect(export).to_be_enabled(timeout=35000)
        expect(master.get_by_text('Master ready', exact=True)).to_be_visible()
        expect(page.locator('#output-results-state')).to_have_text('Ready to audition')
        assert export.get_attribute('aria-describedby') is None
        page.locator('#clear').tap()
        expect(analysis).not_to_be_visible()
        expect(master).not_to_be_visible()
    finally:
        context.close()
