"""Nine-part imports, independent editing, export and bounded WASM staging."""
import io
import zipfile
from playwright.sync_api import expect
from studio_export_checks import audio_names, verify_manifest


def verify_multitrack(page, ready, save, fixture, out, capture):
    page.locator('.edit-import').evaluate('(node) => node.open = true')
    page.locator('#s-mode').select_option('multi')
    kinds = ['vocals', 'drums', 'bass', 'guitar', 'piano', 'synth', 'strings', 'percussion', 'part_9']
    page.get_by_label('Part 9 type', exact=True).select_option('part_9')
    for i, kind in enumerate(kinds):
        page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files(fixture(kind + '.wav', 200 + i * 150))
    page.locator('#s-import').click()
    ready()
    page.locator('#s-tab-mix').click()
    expect(page.locator('.edit-fader')).to_have_count(9)
    before = save('#s-export', 'nine-mix.wav')
    raw_before = save('#s-export-stems', 'nine-stems.zip')
    bank_before = zipfile.ZipFile(io.BytesIO(raw_before))
    assert bank_before.testzip() is None
    assert audio_names(bank_before) == {kind + '.wav' for kind in kinds}
    verify_manifest(bank_before)
    page.get_by_role('checkbox', name='part_9 mute', exact=True).check()
    page.locator('#s-mix').click()
    ready()
    muted = save('#s-export', 'nine-muted.wav')
    assert muted != before
    page.locator('#s-mix').click()
    ready()
    assert save('#s-export', 'nine-muted-repeat.wav') == muted
    page.get_by_role('checkbox', name='part_9 mute', exact=True).uncheck()
    page.locator('#s-mix').click()
    ready()
    page.locator('#s-target').select_option('part_9')
    page.locator('#s-tab-voice').click()
    page.locator('#s-highpass').fill('150')
    page.locator('#s-voice').click()
    ready()
    page.locator('#s-apply-stem').click()
    ready()
    edited = save('#s-export-stems', 'nine-edited.zip')
    bank_after = zipfile.ZipFile(io.BytesIO(edited))
    assert bank_before.read('part_9.wav') != bank_after.read('part_9.wav')
    for kind in kinds[:-1]:
        assert bank_before.read(kind + '.wav') == bank_after.read(kind + '.wav')
    page.locator('#s-undo').click()
    ready()
    assert save('#s-export-stems', 'nine-undo.zip') == raw_before
    page.locator('#s-redo').click()
    ready()
    assert save('#s-export-stems', 'nine-redo.zip') == edited
    page.locator('#s-tab-mix').click()
    page.locator('#s-faders').scroll_into_view_if_needed()
    capture(out / 'nine-faders.png', full_page=True)
    page.locator('.edit-import').evaluate('(node) => node.open = true')
    page.locator('#s-mode').scroll_into_view_if_needed()
    capture(out / 'nine-import-desktop.png', full_page=True)
    page.set_viewport_size({'width': 390, 'height': 844})
    capture(out / 'nine-import-phone.png', full_page=True)
    assert page.evaluate('document.documentElement.scrollWidth <= innerWidth + 1')
    page.locator('#s-stem-files input[data-kind="part_9"]').scroll_into_view_if_needed()
    capture(out / 'nine-phone-last-slot.png')
    page.locator('#s-import').scroll_into_view_if_needed()
    assert page.locator('#s-import').evaluate('(node) => { const r = node.getBoundingClientRect(); return document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2) === node; }')
    capture(out / 'nine-phone-import-action.png')
    page.set_viewport_size({'width': 1480, 'height': 1000})
    page.get_by_label('Part 9 type', exact=True).select_option('vocals')
    page.locator('#s-import').click()
    expect(page.locator('#s-status')).to_contain_text('unique')
    ready()
    assert save('#s-export-stems', 'nine-after-duplicate.zip') == edited
    page.get_by_label('Part 9 type', exact=True).select_option('part_9')
    for kind in kinds[5:]:
        page.locator(f'#s-stem-files input[data-kind="{kind}"]').set_input_files([])
    page.locator('#s-import').click()
    ready()
    expect(page.locator('.edit-fader')).to_have_count(5)
    assert len(audio_names(zipfile.ZipFile(io.BytesIO(save('#s-export-stems', 'five-stems.zip'))))) == 5

    # Exercise a tenth/duplicate stage against the real WASM boundary, bypassing UI limits.
    boundary = page.evaluate('''async () => {
        const {default: init, BrowserStudio} = await import('./pkg/dsp.js');
        const {STEM_KINDS} = await import('./studio-stems.js');
        await init();
        const defaults = JSON.parse(BrowserStudio.parameter_defaults());
        if (defaults.max_stems !== 9 || JSON.stringify([...defaults.stem_kinds].sort()) !== JSON.stringify([...STEM_KINDS].sort())) throw Error('Stem schema drift');
        const bank = new BrowserStudio();
        let duplicate = '', tenth = '';
        try {
            for (let i = 1; i <= 9; ++i) bank.stage_stem('part_' + i, new Float32Array(800), 8000, 1);
            try { bank.stage_stem('guitar', new Float32Array(800), 8000, 1); } catch (error) { tenth = String(error); }
            bank.commit_stems();
            if (JSON.parse(bank.info()).stems.length !== 9) throw Error('Tenth stage corrupted bank');
        } finally { bank.free(); }
        const other = new BrowserStudio();
        try {
            other.stage_stem('guitar', new Float32Array(800), 8000, 1);
            try { other.stage_stem('guitar', new Float32Array(800), 8000, 1); } catch (error) { duplicate = String(error); }
        } finally { other.free(); }
        let memory = '';
        const bounded = new BrowserStudio();
        try {
            const pcm = new Float32Array(4 * 1024 * 1024);
            for (let i = 1; i <= 8; ++i) bounded.stage_stem('part_' + i, pcm, 48000, 1);
            try { bounded.stage_stem('part_9', pcm, 48000, 1); } catch (error) { memory = String(error); }
        } finally { bounded.free(); }
        return {tenth, duplicate, memory};
    }''')
    assert 'nine' in boundary['tenth'] and 'unique' in boundary['duplicate'], boundary
    assert '128 MiB' in boundary['memory'], boundary
    return ['nine stems: independent ninth fader, deterministic mix, exact raw export, direct edit and undo/redo',
            'partial five-stem bank, repeated-instrument identities, duplicate recovery and 390px import layout',
            'real WASM rejects tenth/duplicate stage and combined PCM over 128 MiB; JS kinds match shared Rust schema']
