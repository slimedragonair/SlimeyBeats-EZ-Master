"""Check the real mobile mastering controls and their desktop handoff."""
import re

from playwright.sync_api import expect


def verify_mobile_mastering_controls(browser, url, width=390, height=844, *, download=False, screenshot=None):
    context = browser.new_context(viewport={'width': width, 'height': height}, has_touch=True)
    try:
        page = context.new_page()
        page.goto(url, wait_until='domcontentloaded')
        page.locator('#tutorial-decline').tap()
        expect(page.locator('#steps')).to_be_visible()
        profile = page.locator('#profile')
        format_choice = page.locator('#format')
        expect(profile).to_be_enabled(timeout=30000)
        page.evaluate("window.firstMainSelects = [document.querySelector('#profile'), document.querySelector('#format')]")
        page.locator('#demo').tap()
        expect(page.locator('#track-card')).to_be_visible(timeout=30000)
        expect(page.locator('#steps')).not_to_be_visible()
        mobile = page.locator('#mobile-master-settings')
        expect(mobile).to_be_visible()
        expect(mobile.locator('#profile')).to_be_visible()
        expect(mobile.locator('#format')).to_be_visible()
        expect(page.locator('#mobile-master')).to_be_enabled()
        page.evaluate("document.querySelector('#mobile-master-settings').scrollIntoView({block:'center'})")
        layout = page.evaluate('''() => {
            const rect = id => document.getElementById(id).getBoundingClientRect();
            const master = rect('mobile-master'), profile = rect('profile'), format = rect('format');
            const root = document.documentElement;
            return {gap:profile.top - master.bottom, heights:[profile.height,format.height],
                scroll:root.scrollWidth, width:root.clientWidth, left:Math.min(profile.left,format.left),
                right:Math.max(profile.right,format.right)};
        }''')
        assert 0 <= layout['gap'] <= 92 and min(layout['heights']) >= 44, layout
        assert layout['left'] >= 0 and layout['right'] <= layout['width'] and layout['scroll'] <= layout['width'] + 1, layout
        profile.select_option(label='Game / Instrumental Score')
        format_choice.select_option('wav32f')
        for control in [profile, format_choice]:
            text_fit = control.evaluate('''select => {
                const style = getComputedStyle(select);
                const canvas = document.createElement('canvas').getContext('2d');
                canvas.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
                const text = select.selectedOptions[0].text.trim();
                const padding = parseFloat(style.paddingInlineStart) + parseFloat(style.paddingInlineEnd);
                // Reserve room for the native dropdown arrow as well as the padding.
                return {text, width:canvas.measureText(text).width, room:select.clientWidth - padding - 24};
            }''')
            assert text_fit['width'] <= text_fit['room'], text_fit
        if screenshot:
            page.evaluate("document.querySelector('#mobile-master').scrollIntoView({block:'center'})")
            page.screenshot(path=str(screenshot))
        profile.select_option('edm')
        format_choice.select_option('mp3')
        expect(page.locator('#mobile-export')).to_contain_text('MP3')
        expect(page.locator('#mobile-export')).to_be_disabled()
        profile.focus()
        page.set_viewport_size({'width': 1280, 'height': 900})
        expect(page.locator('.direction #profile')).to_be_visible()
        expect(page.locator('.direction #format')).to_be_visible()
        expect(profile).to_be_focused()
        expect(page.locator('#export')).to_contain_text('MP3')
        page.set_viewport_size({'width': width, 'height': height})
        expect(mobile.locator('#profile')).to_be_focused()
        expect(profile).to_have_value('edm')
        expect(format_choice).to_have_value('mp3')
        assert page.evaluate("window.firstMainSelects.every((node, i) => node === document.querySelector(['#profile','#format'][i]))")
        if download:
            page.locator('#mobile-master').tap()
            expect(page.locator('#mobile-export')).to_be_enabled(timeout=120000)
            with page.expect_download(timeout=120000) as export:
                page.locator('#mobile-export').tap()
            assert export.value.suggested_filename.endswith('.mp3')
            assert export.value.path().stat().st_size > 1000
        if width >= 370:
            page.evaluate("document.documentElement.style.fontSize = '200%'")
            expect(page.locator('body')).to_have_class(re.compile(r'\blarge-text\b'))
            enlarged = page.evaluate('''() => {
                const root = document.documentElement;
                const profile = document.getElementById('profile').getBoundingClientRect();
                const format = document.getElementById('format').getBoundingClientRect();
                return {scroll:root.scrollWidth, width:root.clientWidth, bottom:profile.bottom, top:format.top};
            }''')
            assert enlarged['scroll'] <= enlarged['width'] + 1 and enlarged['top'] >= enlarged['bottom'], enlarged
            page.evaluate("document.documentElement.style.removeProperty('font-size')")
        page.locator('#clear').tap()
        expect(mobile).not_to_be_visible()
        expect(page.locator('#steps')).to_be_visible()
        expect(page.locator('.direction #profile')).to_have_count(1)
    finally:
        context.close()
