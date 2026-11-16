"""Actual application controls; no replacement timeline or DSP implementation."""
import array
import io
import json
import wave
import zipfile

from playwright.sync_api import expect
from browser_capture import BrowserDiagnostics
from region_edit_cancel_checks import verify_cancellation

TOOLS = [('silence', 'silence'), ('fade-in', 'fade_in'), ('fade-out', 'fade_out'),
         ('reverse', 'reverse'), ('apply-gain', 'gain')]
NAV = ['fit', 'fit-selection', 'zoom-in', 'zoom-out', 'left', 'right']


def fixture(name, seed=0):
    samples = array.array('h', (((frame * (43 if channel else 17) + seed * 997) % 4093) -
                               (3071 if channel else 1021)
                               for frame in range(57600) for channel in range(2)))
    output = io.BytesIO()
    with wave.open(output, 'wb') as writer:
        writer.setparams((2, 2, 48000, 0, 'NONE', 'not compressed'))
        writer.writeframes(samples.tobytes())
    return {'name': name, 'mimeType': 'audio/wav', 'buffer': output.getvalue()}


def ready(page):
    expect(page.locator('#s-export')).to_be_enabled(timeout=60000)


def install(page):
    return page.evaluate("async () => (await import('/__region_tests/region_edit_ui_probe.js')).installUiProbe()")


def import_bank(page):
    page.locator('#tab-studio-tools').click()
    details = page.locator('.edit-import')
    if not details.evaluate('el => el.open'):
        details.locator('summary').click()
    for seed, kind in enumerate(['vocals', 'instrumental']):
        page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(f'{kind}-generated.wav', seed))
    page.locator('#s-import').click()
    ready(page)
    expect(page.locator('#s-editor-view')).to_be_hidden()
    page.locator('#s-view-timeline').click()
    expect(page.locator('#s-editor-view')).to_be_visible()
    expect(page.locator('#s-editor-stem')).to_be_visible()
    page.locator('#s-editor-stem').select_option('vocals')


def selection(page, start='0.2', end='0.6'):
    page.locator('#s-edit-start').fill(start)
    page.locator('#s-edit-start').press('Enter')
    page.locator('#s-edit-end').fill(end)
    page.locator('#s-edit-end').press('Enter')
    page.locator('#s-edit-end').press('Tab')


def save(page, selector, path):
    with page.expect_download(timeout=60000) as event:
        page.locator(selector).click()
    event.value.save_as(str(path))
    ready(page)
    return path.read_bytes()


def touch(page, selector):
    locator = page.locator(selector)
    locator.scroll_into_view_if_needed()
    expect(locator).to_be_enabled()
    box = locator.bounding_box()
    assert box and box['x'] >= -1 and box['x'] + box['width'] <= page.viewport_size['width'] + 1, selector
    page.touchscreen.tap(box['x'] + box['width'] / 2, box['y'] + box['height'] / 2)


def apply_and_check(page, id, kind, click):
    page.evaluate('() => regionUi.remember()')
    before_count = page.evaluate('() => regionUi.calls.filter(c => c.action === "region").length')
    click(f'#s-edit-{id}')
    page.wait_for_function('(n) => regionUi.replies.filter(c => c.action === "region").length > n', arg=before_count)
    ready(page)
    result = page.evaluate('() => regionUi.checkEdit()')
    assert result['operation'] == kind
    click('#s-undo'); ready(page)
    page.evaluate('() => regionUi.unchanged()')
    click('#s-redo'); ready(page)
    page.evaluate('() => regionUi.sameEdited()')
    click('#s-undo'); ready(page)
    return result


def navigation(page, click):
    selection(page)
    before = page.evaluate('() => regionUi.mutations()')
    click('#s-time-fit')
    full = page.locator('#s-time-view').inner_text()
    click('#s-time-zoom-in')
    assert page.locator('#s-time-view').inner_text() != full, 'Zoom in did not change visible range'
    click('#s-time-zoom-out')
    assert page.locator('#s-time-view').inner_text() == full, 'Zoom out did not restore fitted range'
    click('#s-time-fit-selection')
    selected = page.locator('#s-time-view').inner_text()
    assert selected != full, 'Fit selection had no effect'
    click('#s-time-right')
    assert page.locator('#s-time-view').inner_text() != selected, 'Later did not pan'
    click('#s-time-left')
    assert page.locator('#s-time-view').inner_text() == selected, 'Earlier did not restore range'
    click('#s-time-fit')
    assert page.evaluate('() => regionUi.mutations()') == before, 'Navigation rendered a DSP revision'
    expect(page.locator('#s-edit-start')).to_have_value('0.2')
    expect(page.locator('#s-edit-end')).to_have_value('0.6')


def verify_ui(browser, context, page, base, out, diagnostics, browser_name, result, route_handler):
    checks = result.setdefault('ui_checks', [])
    install(page)
    import_bank(page)
    navigation(page, lambda selector: page.locator(selector).click())
    canvas = page.locator('#s-time-lanes .s-time-rows canvas').first
    canvas.scroll_into_view_if_needed()
    box = canvas.bounding_box()
    assert box and box['width'] > 100
    count = page.evaluate('() => regionUi.mutations()')
    page.mouse.move(box['x'] + box['width'] * .2, box['y'] + box['height'] / 2)
    page.mouse.down()
    page.mouse.move(box['x'] + box['width'] * .65, box['y'] + box['height'] / 2, steps=8)
    page.mouse.up()
    start, end = (float(page.locator(f'#s-edit-{key}').input_value()) for key in ['start', 'end'])
    assert 0 <= start < end <= 1.21, (start, end)
    assert abs(start - 1.2 * .2) < .03 and abs(end - 1.2 * .65) < .03, 'Pointer selection time mapping'
    assert page.evaluate('() => regionUi.mutations()') == count
    page.mouse.click(box['x'] + box['width'] * .4, box['y'] + box['height'] / 2)
    assert abs(page.evaluate('() => regionUi.playback.position()') - .48) < .04, 'Waveform click did not seek'
    checks.append('desktop real pointer drag selects the displayed frame interval; click seeks; all six navigation controls preserve selection without DSP edits')

    selection(page)
    page.locator('#s-edit-gain').fill('-6')
    measurements = [apply_and_check(page, id, kind, lambda selector: page.locator(selector).click()) for id, kind in TOOLS]
    (out / 'ui-pcm-checks.json').write_text(json.dumps(measurements, indent=2), encoding='utf-8')
    checks.append('all five numeric UI tools reach the actual worker: raw PCM checked throughout selection, exact outside/other stem, real Undo/Redo')

    page.evaluate('() => regionUi.remember()')
    before = page.evaluate('() => regionUi.mutations()')
    for start, end in [('-1', '.6'), ('.8', '.2'), ('.2', '2'), ('.2', '.2')]:
        selection(page, start, end)
        for id, _ in TOOLS:
            expect(page.locator(f'#s-edit-{id}')).to_be_disabled()
    selection(page)
    for gain in ['13', '-61', '']:
        page.locator('#s-edit-gain').fill(gain)
        expect(page.locator('#s-edit-apply-gain')).to_be_disabled()
    page.locator('#s-edit-gain').fill('-6')
    page.locator('#s-view-mixer').click()
    page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('-3')
    page.locator('#s-view-timeline').click()
    for id, _ in TOOLS:
        expect(page.locator(f'#s-edit-{id}')).to_be_disabled()
    assert page.evaluate('() => regionUi.mutations()') == before
    page.evaluate('() => regionUi.unchanged()')
    page.locator('#s-view-mixer').click()
    page.get_by_role('slider', name='vocals Gain / dB', exact=True).fill('0')
    page.locator('#s-view-timeline').click()
    page.locator('#s-mix').click(); ready(page)
    checks.append('invalid numeric ranges/gain and pending mixer controls disable destructive region tools and preserve committed bank/preview')

    checks.extend(verify_cancellation(page, ready, result))
    page.locator('#s-edit-reverse').click(); ready(page)
    page.evaluate('() => regionUi.checkEdit()')
    page.locator('#s-format').select_option('wav32f')
    preview = save(page, '#s-export', out / 'ui-edited-preview.wav')
    bank = save(page, '#s-export-stems', out / 'ui-edited-bank.zip')
    session = save(page, '#s-export-session', out / 'ui-edited-session.zip')
    raw, bundle = zipfile.ZipFile(io.BytesIO(bank)), zipfile.ZipFile(io.BytesIO(session))
    assert raw.testzip() is None and bundle.testzip() is None
    for name in ['vocals.wav', 'instrumental.wav']:
        assert bundle.read(name) == raw.read(name), 'UI session omitted edited bank'
    assert bundle.read('mix.wav') == preview
    details = page.locator('#s-playback-details')
    if not details.evaluate('el => el.open'):
        details.locator('summary').click()
    for volume in ['0', '1']:
        page.locator('#s-volume').fill(volume)
        assert save(page, '#s-export', out / f'ui-volume-{volume}.wav') == preview, 'Listening volume changed saved preview'
        assert save(page, '#s-export-stems', out / f'ui-volume-{volume}-stems.zip') == bank, 'Listening volume changed raw bank'
    diagnostics.capture(out / 'desktop-region-editor.png', full_page=True)
    checks.append('UI cancellation discards region proposal; raw/session downloads contain edited bank; zero/full listening volume leave exported PCM and stems exact')

    page.evaluate("document.dispatchEvent(new Event('open-recovery'))")
    expect(page.locator('#recovery-dialog')).to_be_visible()
    page.locator('#recovery-enable').click()
    page.locator('#recovery-save').click()
    expect(page.locator('#recovery-status')).to_contain_text('Recovery saved locally', timeout=30000)
    page.reload(wait_until='networkidle')
    expect(page.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
    install(page)
    page.locator('#recovery-offer-open').click()
    page.locator('#recovery-points button').first.click()
    expect(page.locator('#recovery-action-status')).to_contain_text('Restored with playback stopped', timeout=120000)
    page.locator('#recovery-close').click()
    page.locator('#tab-studio-tools').click(); ready(page)
    expect(page.locator('#s-play')).to_have_text('Play')
    assert save(page, '#s-export', out / 'ui-restored-preview.wav') == preview
    assert save(page, '#s-export-stems', out / 'ui-restored-bank.zip') == bank
    checks.append('actual recovery save/reload/restore replays edited Studio bank and preview exactly with playback stopped')

    for width in [320, 390]:
        touch_context = browser.new_context(viewport={'width': width, 'height': 844}, has_touch=True, accept_downloads=True)
        touch_context.route('**/*', route_handler)
        mobile = touch_context.new_page()
        capture = BrowserDiagnostics(mobile, browser_name, result)
        try:
            mobile.goto(base, wait_until='networkidle')
            expect(mobile.locator('#engine-status')).to_have_text('RUST · LOCAL', timeout=30000)
            install(mobile)
            layouts = result.setdefault('touch_layout', {}).setdefault(str(width), {})
            layouts['initial'] = mobile.evaluate('() => regionUi.layout()')
            import_bank(mobile)
            layouts['imported_time_view'] = mobile.evaluate('() => regionUi.layout()')
            navigation(mobile, lambda selector: touch(mobile, selector))
            stem_select = mobile.locator('#s-editor-stem')
            stem_select.focus()
            expect(stem_select).to_be_focused()
            stem_select.press('ArrowDown')
            expect(stem_select).to_have_value('instrumental')
            layouts['native_selection_focused'] = mobile.evaluate('() => regionUi.layout()')
            stem_select.press('ArrowUp')
            expect(stem_select).to_have_value('vocals')
            stem_select.press('ArrowDown')
            expect(stem_select).to_have_value('instrumental')
            # Open the native chooser with a real click, then commit both
            # choices through its keyboard handling instead of select_option.
            stem_select.click()
            mobile.keyboard.press('ArrowUp')
            mobile.keyboard.press('Enter')
            expect(stem_select).to_have_value('vocals')
            stem_select.click()
            mobile.keyboard.press('ArrowDown')
            mobile.keyboard.press('Enter')
            expect(stem_select).to_have_value('instrumental')
            layouts['native_chooser_selected'] = mobile.evaluate('() => regionUi.layout()')
            stem_select.press('Tab')
            layouts['second_stem_selected'] = mobile.evaluate('() => regionUi.layout()')
            selection(mobile)
            mobile.locator('#s-edit-gain').fill('-6')
            touch_measurements = result.setdefault('touch_pcm_checks', {}).setdefault(str(width), [])
            for id, kind in TOOLS:
                measured = apply_and_check(mobile, id, kind, lambda selector: touch(mobile, selector))
                assert measured['kind'] == 'instrumental', 'Touch edited a different stem'
                touch_measurements.append(measured)
            geometry = mobile.evaluate("""() => ({ ...regionUi.layout(),
                overflow: [...document.querySelectorAll('body *')].map(el => {
                    const r = el.getBoundingClientRect(), style = getComputedStyle(el);
                    return { tag: el.tagName, id: el.id, class: el.className, left: r.left, right: r.right, width: r.width,
                        scroll_width: el.scrollWidth, client_width: el.clientWidth,
                        styles: Object.fromEntries(['display', 'width', 'min-width', 'max-width', 'overflow-x',
                            'white-space', 'text-overflow', 'appearance', 'grid-template-columns'].map(key => [key, style.getPropertyValue(key)])) };
                }).filter(r => r.width > 0 && (r.left < -1 || r.right > innerWidth + 1 ||
                    (r.scroll_width > r.client_width + 1 && r.styles['overflow-x'] === 'visible'))).slice(0, 40) })""")
            layouts['after_all_edits'] = {key: value for key, value in geometry.items() if key != 'overflow'}
            (out / f'touch-{width}-geometry.json').write_text(json.dumps(geometry, indent=2), encoding='utf-8')
            assert geometry['document'] <= geometry['viewport'], f'{width}px horizontal overflow: {geometry}'
            for id, _ in TOOLS:
                locator = mobile.locator(f'#s-edit-{id}')
                locator.scroll_into_view_if_needed()
                box = locator.bounding_box()
                assert box and box['width'] >= 40 and box['height'] >= 40, f'{width}px unreachable touch target {id}: {box}'
            capture.capture(out / f'touch-{width}-region-editor.png', full_page=True)
            mobile.evaluate("document.documentElement.style.fontSize='200%'")
            mobile.wait_for_function("() => document.body.classList.contains('large-text') && parseFloat(getComputedStyle(document.documentElement).fontSize) >= 32")
            large = mobile.evaluate("""() => ({ ...regionUi.layout(), root_font_px: parseFloat(getComputedStyle(document.documentElement).fontSize),
                columns: ['.s-time-navigation', '.s-time-tools', '.s-time-gain'].map(selector =>
                    ({ selector, columns: getComputedStyle(document.querySelector(selector)).gridTemplateColumns })) })""")
            assert large['document'] <= large['viewport'], f'{width}px 200% root text overflow: {large}'
            assert all(len(item['columns'].split()) == 1 for item in large['columns']), '200% text must reflow tool/nav/gain rows into one column'
            touch(mobile, '#s-time-fit')
            large['pcm_check'] = apply_and_check(mobile, 'reverse', 'reverse', lambda selector: touch(mobile, selector))
            large['after_edits'] = mobile.evaluate('() => regionUi.layout()')
            assert large['after_edits']['document'] <= width, f'{width}px 200% text edit introduced overflow'
            result.setdefault('large_text', {})[str(width)] = large
            (out / f'touch-{width}-200-percent.json').write_text(json.dumps(large, indent=2), encoding='utf-8')
            capture.capture(out / f'touch-{width}-200-percent.png', full_page=True)
            checks.append(f'{width}px emulated touch: five tools, exact raw PCM/Undo/Redo, native chooser, six navigation controls; 100/200% actual root text reflow without overflow')
        except Exception:
            try:
                capture.capture(out / f'failure-touch-{width}.png', full_page=True)
            except Exception as error:
                result['touch_capture_failure'] = repr(error)
            raise
        finally:
            touch_context.close()
    return checks
