"""Real multi-picker and Suno-style paired WAV/MP3 ZIP import workflows."""
import array
import io
import json
from pathlib import Path
import sys
import zipfile
from playwright.sync_api import expect
from studio_export_checks import audio_names, verify_manifest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'rust/tools'))
from stem_bundle_fixtures import chunk, suno_fixture, zipped


def review_ready(page, count):
    # A previous review can remain visible while its replacement ZIP is read.
    # Wait for the job controls to settle before trusting summary/card content.
    expect(page.locator('#s-zip-choose')).to_be_enabled(timeout=30000)
    expect(page.locator('#s-import-batch')).to_be_enabled(timeout=30000)
    expect(page.locator('#s-cancel')).to_be_disabled()
    expect(page.locator('#s-status')).to_have_text(f'Review {count} parts, then import when ready')
    expect(page.locator('#s-status.error')).to_have_count(0)
    expect(page.locator('#s-bulk-review')).to_be_visible()
    expect(page.locator('#s-bulk-summary')).to_contain_text(f'{count} distinct parts')
    expect(page.locator('.edit-bulk-part')).to_have_count(count)


def verify_import_hit(page, out, label):
    button = page.locator('#s-import-batch')
    button.scroll_into_view_if_needed()
    geometry = button.evaluate('''node => {
        const rect = element => element.getBoundingClientRect().toJSON();
        const button = rect(node), dock = document.querySelector('.edit-player');
        const hit = document.elementFromPoint(button.x + button.width / 2, button.y + button.height / 2);
        return {
            button, dock: rect(dock), summary: rect(document.querySelector('#s-bulk-action-summary')),
            hit: hit ? {tag: hit.tagName, id: hit.id, class: hit.className} : null,
            buttonContainsHit: node.contains(hit),
            viewport: {width: innerWidth, height: innerHeight, scrollY},
            documentScrollPadding: getComputedStyle(document.documentElement).scrollPaddingBottom,
        };
    }''')
    (out / f'{label}-hit.json').write_text(json.dumps(geometry, indent=2), encoding='utf-8')
    assert geometry['buttonContainsHit'], geometry
    assert 0 <= geometry['button']['top'] < geometry['button']['bottom'] <= geometry['dock']['top'], geometry
    assert 0 <= geometry['summary']['top'] < geometry['summary']['bottom'] <= geometry['dock']['top'], geometry
    expect(button).to_be_enabled()
    return geometry


def verify_fader_label(page, title, out):
    geometry = title.evaluate('''async node => {
        await document.fonts.ready;
        // Center within the scroll area: start alignment can round to -0.4px in Firefox.
        node.scrollIntoView({block: 'center', behavior: 'instant'});
        await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const rect = element => element.getBoundingClientRect().toJSON();
        const style = getComputedStyle(node), range = document.createRange();
        range.selectNodeContents(node);
        const lines = [...range.getClientRects()].filter(line => line.width > 0 && line.height > 0);
        return {
            title: rect(node), dock: rect(document.querySelector('.edit-player')),
            lineHeight: style.lineHeight, fontSize: style.fontSize, fonts: document.fonts.status,
            lineCount: new Set(lines.map(line => line.top)).size,
            lineHits: lines.map(line => {
                const hit = document.elementFromPoint(line.left + 2, line.top + line.height / 2);
                return {rect: line.toJSON(), covered: !node.contains(hit), tag: hit?.tagName, id: hit?.id};
            }),
            viewport: {width: innerWidth, height: innerHeight, scrollY},
        };
    }''')
    (out / 'friendly-fader-geometry.json').write_text(json.dumps(geometry, indent=2), encoding='utf-8')
    assert geometry['lineCount'] > 1, geometry
    assert 0 <= geometry['title']['left'] < geometry['title']['right'] <= geometry['viewport']['width'], geometry
    assert 0 <= geometry['title']['top'] < geometry['title']['bottom'] <= min(geometry['dock']['top'], geometry['viewport']['height']), geometry
    assert not any(line['covered'] for line in geometry['lineHits']), geometry
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')


def verify_bundle(page, ready, save, fixture, out, capture):
    files = suno_fixture(fixture)
    page.locator('#s-bulk-files').set_input_files(files)
    review_ready(page, 9)
    page.locator('#s-import-batch').click()
    ready()
    loose = save('#s-export-stems', 'bulk-loose-nine.zip')
    bank = zipfile.ZipFile(io.BytesIO(loose))
    verify_manifest(bank)
    bank_audio = audio_names(bank)
    assert len(bank_audio) == 9
    for file in files[::2]:
        pcm = array.array('h', chunk(file['buffer'], b'data'))
        expected = array.array('f', (sample / 32768 for sample in pcm)).tobytes()
        # Browser converters differ in positive full-scale and float multiply/divide rounding.
        asymmetric = array.array('f', (sample / (32767 if sample >= 0 else 32768) for sample in pcm)).tobytes()
        scale = array.array('f', [1 / 32767])[0]
        multiplied = array.array('f', (sample * scale if sample >= 0 else sample / 32768 for sample in pcm)).tobytes()
        assert chunk(bank.read(file['name']), b'data') in (expected, asymmetric, multiplied)

    paired = zipped(files, 'Song stems/')
    page.locator('#s-zip-file').set_input_files(paired)
    review_ready(page, 9)
    expect(page.locator('#s-bulk-format')).to_have_value('wav')
    # Distinguish the actual new ZIP plan from the identical-count loose plan.
    assert all('Song stems/' in text for text in page.locator('.edit-bulk-part small').all_text_contents())
    assert all('WAV' in text for text in page.locator('.edit-bulk-part small').all_text_contents())
    page.locator('#s-stem-drop h3').evaluate("node => node.scrollIntoView({block: 'start'})")
    expect(page.locator('#s-stem-drop h3')).to_be_in_viewport()
    expect(page.locator('#s-bulk-summary')).to_be_in_viewport()
    expect(page.locator('#s-bulk-format')).to_be_in_viewport()
    expect(page.locator('.edit-bulk-part').first).to_be_in_viewport()
    capture(out / 'suno-zip-review-desktop.png')
    page.locator('#s-import-batch').click()
    ready()
    assert save('#s-export-stems', 'suno-wav-nine.zip') == loose
    page.locator('#s-zip-file').set_input_files(paired)
    review_ready(page, 9)
    page.locator('#s-bulk-format').select_option('mp3')
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('Prefer MP3')
    assert all('MP3' in text for text in page.locator('.edit-bulk-part small').all_text_contents())
    page.locator('#s-import-batch').click()
    ready()
    compressed_bank = zipfile.ZipFile(io.BytesIO(save('#s-export-stems', 'suno-mp3-nine.zip')))
    verify_manifest(compressed_bank)
    assert audio_names(compressed_bank) == bank_audio
    assert any(chunk(compressed_bank.read(name), b'data') != chunk(bank.read(name), b'data') for name in bank_audio)
    assert all(len(chunk(compressed_bank.read(name), b'data')) == len(chunk(bank.read(name), b'data')) for name in bank_audio)

    many = [fixture(f'guitar take {index:02}.wav', 200 + index * 100) for index in range(12)]
    page.locator('#s-zip-file').set_input_files(zipped(many[:-1]))
    review_ready(page, 11)
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('11 found · 9 / 9 selected · 2 not selected')
    expect(page.locator('#s-bulk-omitted-parts li')).to_have_text(['guitar take 09', 'guitar take 10'])
    page.locator('#s-bulk-omitted-summary').focus()
    page.locator('#s-bulk-omitted-summary').press('Enter')
    expect(page.locator('#s-bulk-omitted-parts')).to_be_visible()
    page.locator('#s-bulk-omitted-summary').press('Enter')
    page.set_viewport_size({'width': 390, 'height': 844})
    verify_import_hit(page, out, 'phone-eleven')
    expect(page.locator('#s-bulk-action-summary')).to_be_in_viewport()
    expect(page.locator('#s-bulk-omitted-summary')).to_be_in_viewport()
    capture(out / 'suno-zip-review-phone-eleven.png')
    page.set_viewport_size({'width': 1480, 'height': 1000})
    page.locator('#s-zip-file').set_input_files(zipped(many))
    review_ready(page, 12)
    expect(page.get_by_role('checkbox', name='Include guitar take 11', exact=True)).to_be_disabled()
    page.get_by_role('checkbox', name='Include guitar take 00', exact=True).uncheck()
    page.get_by_role('checkbox', name='Include guitar take 11', exact=True).check()
    expect(page.locator('#s-bulk-summary')).to_contain_text('9 / 9 selected')
    page.set_viewport_size({'width': 390, 'height': 844})
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('12 found · 9 / 9 selected · 3 not selected')
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('Prefer WAV')
    expect(page.locator('#s-bulk-omitted-parts li')).to_have_text(['guitar take 00', 'guitar take 09', 'guitar take 10'])
    page.get_by_role('checkbox', name='Include guitar take 11', exact=True).uncheck()
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('8 / 9 selected')
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('4 not selected')
    page.get_by_role('checkbox', name='Include guitar take 11', exact=True).check()
    page.locator('#s-bulk-format').select_option('mp3')
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('Prefer MP3')
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('9 selected parts use the available-format fallback')
    page.locator('#s-bulk-format').select_option('wav')
    verify_import_hit(page, out, 'phone-twelve')
    expect(page.locator('#s-bulk-action-summary')).to_be_in_viewport()
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
    capture(out / 'suno-zip-review-phone.png')
    page.locator('#s-import-batch').click()
    ready()
    expect(page.locator('#s-bulk-review')).to_be_hidden()
    page.set_viewport_size({'width': 1480, 'height': 1000})
    expect(page.locator('.edit-fader')).to_have_count(9)
    names = page.locator('.edit-fader strong').all_text_contents()
    assert 'guitar take 11' in names and 'guitar take 00' not in names
    retained = save('#s-export-stems', 'suno-selected-nine.zip')
    page.locator('#s-zip-file').set_input_files({'name': 'bad.zip', 'mimeType': 'application/zip', 'buffer': b'not a ZIP'})
    expect(page.locator('#s-status.error')).to_contain_text('ZIP', timeout=30000)
    ready()
    assert save('#s-export-stems', 'suno-after-bad-zip.zip') == retained

    large = [fixture(f'Layer {index:02} ' + 'long expressive instrument ' * 4 + '.wav', seconds=.1) for index in range(64)]
    page.locator('#s-bulk-files').set_input_files(large)
    review_ready(page, 64)
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('55 not selected')
    assert len(page.locator('#s-bulk-omitted-summary').inner_text()) < 160
    expect(page.locator('#s-bulk-omitted-parts li')).to_have_count(55)
    page.locator('#s-bulk-omitted-summary').click()
    page.locator('#s-bulk-omitted-parts li').last.scroll_into_view_if_needed()
    expect(page.locator('#s-bulk-omitted-parts li').last).to_be_in_viewport()

    # Mixed availability still means one encoding per logical part.
    partial = [file for file in files[:6] if file['name'] != 'vocals.wav']
    page.locator('#s-zip-file').set_input_files(zipped(partial))
    review_ready(page, 3)
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('1 selected part uses the available-format fallback')
    checks = page.locator('.edit-bulk-part input[type="checkbox"]')
    checks.nth(0).uncheck()
    expect(page.locator('#s-import-batch')).to_be_enabled()
    checks.nth(1).uncheck()
    expect(page.locator('#s-bulk-action-summary')).to_contain_text('1 / 9 selected')
    expect(page.locator('#s-import-batch')).to_be_disabled()
    checks.nth(0).check()
    checks.nth(1).check()
    expect(page.locator('#s-import-batch')).to_be_enabled()
    page.locator('#s-import-batch').click()
    ready()
    expect(page.locator('.edit-fader')).to_have_count(3)
    fallback_bank = zipfile.ZipFile(io.BytesIO(save('#s-export-stems', 'suno-format-fallback.zip')))
    verify_manifest(fallback_bank)
    assert len(audio_names(fallback_bank)) == 3

    long_name = 'LeadVocalSessionTakeWithLongFriendlyName' * 5
    page.locator('#s-bulk-files').set_input_files([fixture(long_name + '.wav'), fixture('backing vocals.wav', 220)])
    review_ready(page, 2)
    page.locator('#s-import-batch').click()
    ready()
    page.set_viewport_size({'width': 390, 'height': 844})
    title = page.locator('.edit-fader strong').filter(has_text=long_name)
    expect(title).to_have_count(1)
    verify_fader_label(page, title, out)
    capture(out / 'suno-friendly-name-phone.png')
    page.set_viewport_size({'width': 1480, 'height': 1000})
    return ['multi-file picker pairs 18 WAV/MP3 files into nine parts; loose/ZIP raw PCM matches exactly',
            'Suno-style ZIP: WAV preference, actual MP3 selection, format fallback and no duplicate encoding',
            '12-part ZIP reviewed/selected down to nine, 390px reachable import, corrupt ZIP preserves bank',
            'live action summary follows selection/preference/fallback; one-part import disabled; settled desktop review captured',
            '11-part subset explicitly lists two omissions; live 12-part omissions and keyboard disclosure; bounded 55-name omission preview with reachable full list',
            'settled Review status and enabled Import required; long friendly fader name wraps at 390px without overflow']
