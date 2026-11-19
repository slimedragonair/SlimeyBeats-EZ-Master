"""Exercise the real tutorial UI and the rendered spotlight geometry."""
import argparse
from pathlib import Path

from playwright.sync_api import expect, sync_playwright


def check_spotlight(page, selector):
    expect(page.locator('#tutorial-spotlight')).to_be_visible(timeout=3500)
    page.wait_for_function('''selector => {
        const light = document.querySelector('#tutorial-spotlight').getBoundingClientRect();
        const card = document.querySelector('#learning-dialog').getBoundingClientRect();
        return Array.from(document.querySelectorAll(selector)).some(el => {
            const r = el.getBoundingClientRect();
            return r.width > 0 && r.height > 0 && r.top >= -1 && r.bottom <= innerHeight + 1 &&
                light.top <= r.top && light.bottom >= r.bottom && light.left <= r.left &&
                light.right >= r.right && light.width - r.width <= 20 && light.height - r.height <= 20 &&
                (card.bottom <= r.top || card.top >= r.bottom);
        });
    }''', arg=selector, timeout=3500)


def verify_tutorial_spotlights(browser, url, out=None):
    cases = [
        (1100, 760, False, 'no-preference'),
        (390, 844, True, 'no-preference'),
        (320, 568, True, 'reduce'),
    ]
    for width, height, touch, motion in cases:
        context = browser.new_context(viewport={'width': width, 'height': height},
                                      has_touch=touch, reduced_motion=motion)
        try:
            page = context.new_page()
            page.goto(url, wait_until='domcontentloaded')
            activate = 'tap' if touch else 'click'
            getattr(page.locator('#tutorial-start'), activate)()
            ring = page.locator('#tutorial-spotlight')
            dialog = page.locator('#learning-dialog')
            targets = [
                '#add', '#analyze', '#master,#mobile-master',
                '.primary-player .ab,#master,#mobile-master', '#tab-eq',
                '#export,#mobile-export', '#tab-studio-tools', '#open-learning',
            ]

            for index, target in enumerate(targets):
                check_spotlight(page, target)
                if index == 0:
                    animation = ring.evaluate("e => getComputedStyle(e, '::after').animationName")
                    assert (animation != 'none') == (motion == 'no-preference'), animation
                    assert dialog.evaluate("e => getComputedStyle(e, '::backdrop').backdropFilter") == 'none'
                if out and index in {0, 4}:
                    page.screenshot(path=str(out / f'tour-{width}-{index + 1}.png'), animations='disabled')
                if index == 2:
                    getattr(page.locator('#tutorial-back'), activate)()
                    check_spotlight(page, targets[1])
                    getattr(page.locator('#tutorial-next'), activate)()
                    check_spotlight(page, target)
                if index < len(targets) - 1:
                    getattr(page.locator('#tutorial-next'), activate)()
            getattr(page.locator('#tutorial-finish'), activate)()
            expect(ring).not_to_be_visible()
            expect(dialog).not_to_be_visible()
            expect(page.locator('#open-learning')).to_be_focused()
            expect(page.locator('#export')).to_be_disabled()
            assert page.evaluate("!document.querySelector('#file').files.length")
        finally:
            context.close()


def verify_loaded_tutorial_spotlight(browser, url):
    context = browser.new_context(viewport={'width': 390, 'height': 844}, has_touch=True)
    try:
        page = context.new_page()
        page.goto(url, wait_until='domcontentloaded')
        page.locator('#tutorial-decline').tap()
        expect(page.locator('#demo')).to_be_enabled(timeout=30000)
        page.locator('#demo').tap()
        expect(page.locator('#track-card')).to_be_visible(timeout=30000)
        snapshot = '''() => ({file: document.querySelector('#filename').textContent,
            source: document.querySelector('#original').className,
            play: document.querySelector('#play').textContent,
            export: document.querySelector('#mobile-export').disabled})'''
        before = page.evaluate(snapshot)
        page.locator('#open-learning').tap()
        page.locator('#replay-tutorial').tap()
        check_spotlight(page, '#track-card')
        expect(page.locator('.tutorial-cue')).to_contain_text('Remove track')
        for _ in range(3):
            page.locator('#tutorial-next').tap()
        check_spotlight(page, '.primary-player .ab')
        page.set_viewport_size({'width': 320, 'height': 568})
        check_spotlight(page, '.primary-player .ab')
        page.keyboard.press('Escape')
        expect(page.locator('#tutorial-spotlight')).not_to_be_visible()
        expect(page.locator('#open-learning')).to_be_focused()
        assert page.evaluate(snapshot) == before
    finally:
        context.close()


def verify_tutorial_navigation_restoration(browser, url):
    for initial, resized in [(320, 1100), (320, None), (1100, None)]:
        page = browser.new_page(viewport={'width': initial, 'height': 568 if initial < 680 else 760})
        try:
            page.goto(url, wait_until='domcontentloaded')
            expect(page.locator('#tutorial-start')).to_be_visible()
            pages = page.locator('#page-navigation')
            if initial < 680:
                expect(page.locator('#page-navigation-summary')).to_be_visible()
                assert not pages.evaluate('e => e.open')
            page.locator('#tutorial-start').click()
            for _ in range(4):
                page.locator('#tutorial-next').click()
            check_spotlight(page, '#tab-eq')
            if resized:
                page.set_viewport_size({'width': resized, 'height': 760})
                expect(page.locator('#page-navigation-summary')).not_to_be_visible()
                check_spotlight(page, '#tab-eq')
            page.keyboard.press('Escape')
            expect(page.locator('#tutorial-spotlight')).not_to_be_visible()
            assert pages.evaluate('e => e.open') == ((resized or initial) > 680)
            if (resized or initial) > 680:
                page.locator('#tab-eq').click()
                expect(page.locator('#tab-eq')).to_have_attribute('aria-current', 'page')
        finally:
            page.context.close()


def verify_tutorial_pane_restoration(browser, url):
    page = browser.new_page(viewport={'width': 1280, 'height': 800})
    try:
        page.goto(url, wait_until='domcontentloaded')
        page.locator('#tutorial-decline').click()
        page.locator('#controls-advanced').click()
        direction = page.locator('.direction')
        start = direction.evaluate('e => { e.scrollTop = 140; return e.scrollTop; }')
        assert start > 0, 'The direction pane must actually scroll in this fixture'
        page.locator('#open-learning').click()
        page.locator('#replay-tutorial').click()
        page.locator('#tutorial-next').click()
        page.locator('#tutorial-next').click()
        check_spotlight(page, '#master')
        assert direction.evaluate('e => e.scrollTop') < start
        page.keyboard.press('Escape')
        expect(page.locator('#tutorial-spotlight')).not_to_be_visible()
        assert direction.evaluate('e => e.scrollTop') == start
    finally:
        page.context.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', required=True)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--browser', choices=['chromium', 'firefox', 'webkit'], default='chromium')
    parser.add_argument('--with-audio', action='store_true')
    args = parser.parse_args()
    if args.out:
        args.out.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as playwright:
        browser = getattr(playwright, args.browser).launch()
        try:
            verify_tutorial_spotlights(browser, args.url, args.out)
            verify_tutorial_navigation_restoration(browser, args.url)
            verify_tutorial_pane_restoration(browser, args.url)
            if args.with_audio:
                verify_loaded_tutorial_spotlight(browser, args.url)
        finally:
            browser.close()
    print(f'{args.browser}: tutorial spotlight targets, reduced motion and cleanup passed')
