"""Actual UI reference recovery and retained-PCM adoption regressions."""
import hashlib
import json

from playwright.sync_api import expect

from verify_studio_browser import fixture


def reference_recovery(browser, base, out, ready, open_recovery, save_point):
    context = browser.new_context(viewport={'width': 1480, 'height': 1000}, accept_downloads=True)
    context.add_init_script("""localStorage.setItem('slimeybeats.interface-preferences',
        JSON.stringify({version:1,advanced:true,tutorial:'declined',step:0}));
        window.referenceRecoveryJobs=[];
        const post=Worker.prototype.postMessage;
        Worker.prototype.postMessage=function(message,...args){
            referenceRecoveryJobs.push(message.action);return post.call(this,message,...args);};""")
    page = context.new_page()
    page.on('dialog', lambda dialog: dialog.accept())
    errors, requests, downloads = [], [], []
    page.on('pageerror', lambda error: errors.append(str(error)))
    page.on('request', lambda request: requests.append((request.method, request.url)))
    page.on('download', lambda download: downloads.append(download.suggested_filename))

    def export(name):
        with page.expect_download(timeout=60000) as event:
            page.locator('#export').click()
        path = out / name
        event.value.save_as(str(path))
        expect(page.locator('#export')).to_be_enabled(timeout=60000)
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def restore_latest():
        page.reload(wait_until='networkidle')
        ready(page)
        page.locator('#recovery-offer-open').click()
        page.locator('#recovery-points button').first.click()
        expect(page.locator('#recovery-action-status')).to_contain_text('Restored with playback stopped', timeout=120000)
        page.locator('#recovery-close').click()

    try:
        page.goto(base, wait_until='networkidle')
        ready(page)
        page.locator('#file').set_input_files(fixture('reference-recovery-mix.wav', 440, seconds=.6))
        expect(page.locator('#master')).to_be_enabled(timeout=30000)
        page.locator('#reference-file').set_input_files(fixture('reference.wav', 220, seconds=.6))
        expect(page.locator('#master')).to_be_enabled(timeout=30000)
        page.locator('#master').click()
        expect(page.locator('#export')).to_be_enabled(timeout=60000)
        page.locator('#format').select_option('wav32f')
        original = export('reference-A-before-recovery.wav')
        open_recovery(page)
        page.locator('#recovery-enable').click()
        save_point(page)
        page.locator('#recovery-close').click()
        restore_latest()
        expect(page.locator('#export')).to_be_enabled()
        assert export('reference-A-restored.wav') == original, 'Unchanged reference recovery changed export PCM'

        # Same filename, different audio: preserve the A master for recovery and
        # audition, but both the UI and worker must refuse exporting it as B.
        page.locator('#reference-file').set_input_files(fixture('reference.wav', 880, seconds=.6))
        expect(page.locator('#master')).to_be_enabled(timeout=30000)
        expect(page.locator('#export')).to_be_disabled()
        expect(page.locator('#session-state')).to_contain_text('Direction changed')
        open_recovery(page)
        save_point(page)
        page.locator('#recovery-close').click()
        restore_latest()
        expect(page.locator('#export')).to_be_disabled()
        expect(page.locator('#master-source')).to_be_enabled()
        expect(page.locator('#session-state')).to_contain_text('Direction changed')
        before = len(downloads)
        jobs = page.evaluate('referenceRecoveryJobs.length')
        page.evaluate('document.querySelector("#export").onclick()')
        expect(page.locator('#status')).to_contain_text('Master your current direction before exporting.')
        assert len(downloads) == before
        assert 'export' not in page.evaluate('start=>referenceRecoveryJobs.slice(start)', jobs)

        # Seed only numeric server provenance into an actual local snapshot to
        # exercise the legacy retained-PCM adoption branch, without a fake DSP
        # result or an unnecessary server remaster.
        adoption = page.evaluate('''async () => {
            const resource=name=>performance.getEntriesByType('resource').find(e=>new URL(e.name).pathname===name).name;
            const {workspaceRecovery}=await import(resource('/recovery-bridge.js'));
            const {DspClient}=await import(resource('/dsp-client.js'));
            const {RECOVERY_VERSION}=await import(resource('/recovery-format.js'));
            const snapshot={version:RECOVERY_VERSION,...await workspaceRecovery.capture()},source=snapshot.master.preview;
            const hash=async blob=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await blob.arrayBuffer())),
                value=>value.toString(16).padStart(2,'0')).join('');
            const expected=await hash(source.data),call=DspClient.prototype.call;
            let adoptedClient;
            DspClient.prototype.call=function(action,...args){if(action==='adopt-master')adoptedClient=this;return call.call(this,action,...args);};
            const start=referenceRecoveryJobs.length;
            snapshot.master.serverMetadata={};
            try {
                const staged=await workspaceRecovery.prepare(snapshot,()=>true);
                staged.commit();staged.dispose();
            } finally {DspClient.prototype.call=call;}
            const recovered=await workspaceRecovery.capture();
            let refused=false;
            try {await adoptedClient.call('export',{settings:JSON.stringify(snapshot.master.settings),format:'wav32f'});}
            catch(error){refused=String(error).includes('Reference changed');}
            return {before_sha256:expected,after_sha256:await hash(recovered.master.preview.data),
                frames:source.frames,sample_rate:source.rate,channels:source.channels,
                jobs:referenceRecoveryJobs.slice(start),stale_worker_export_refused:refused};
        }''')
        assert adoption['before_sha256'] == adoption['after_sha256'], adoption
        assert adoption['jobs'].count('adopt-master') == 1 and 'master' not in adoption['jobs'], adoption
        assert adoption['stale_worker_export_refused'], adoption
        expect(page.locator('#export')).to_be_disabled()
        page.locator('#master').click()
        expect(page.locator('#export')).to_be_enabled(timeout=60000)
        export('reference-B-remastered.wav')
        assert not errors, errors
        assert all(method == 'GET' and url.startswith(base) for method, url in requests)
        (out / 'reference-recovery-proof.json').write_text(json.dumps({
            'passed': True, 'reference_A_export_sha256': original, 'adoption': adoption,
            'page_errors': errors, 'network': 'same-origin static GET only',
            'adoption_fixture': 'actual local snapshot plus empty numeric server provenance',
        }, indent=2), encoding='utf-8')
        return [
            'unchanged reference reload keeps exact current export',
            'reference A to same-name B save/reload retains stale master but refuses UI export',
            'retained-PCM adoption preserves exact cached samples and rejects stale worker export; explicit B remaster restores export',
        ]
    finally:
        context.close()
