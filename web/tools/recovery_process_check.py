"""Force-quit only an owned browser process and recover its private persisted profile."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import time

from playwright.sync_api import expect
from verify_studio_browser import fixture


def process_crash(browser_type, base, ready, open_recovery, save_point, proof):
    evidence = {'passed': False, 'method': 'owned_browser_process_termination'}
    processes, connections = [], []
    requests = []
    with tempfile.TemporaryDirectory(prefix='owned-crash-profile-', dir=proof.parent) as directory:
        profile = Path(directory)

        def terminate(process):
            if process.poll() is None:
                if os.name == 'nt':
                    subprocess.run(['taskkill', '/PID', str(process.pid), '/T', '/F'],
                                   check=True, capture_output=True, timeout=15)
                else:
                    os.killpg(process.pid, signal.SIGKILL)
            return process.wait(timeout=15)

        def launch():
            port_file = profile / 'DevToolsActivePort'
            port_file.unlink(missing_ok=True)
            options = {'creationflags': subprocess.CREATE_NO_WINDOW | subprocess.CREATE_NEW_PROCESS_GROUP} if os.name == 'nt' else {'start_new_session': True}
            process = subprocess.Popen([
                browser_type.executable_path, '--headless', '--no-sandbox',
                '--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0',
                '--user-data-dir=' + str(profile), '--no-first-run', '--no-default-browser-check',
                '--disable-background-networking', '--disable-component-update',
                '--disable-default-apps', '--disable-sync', '--disable-dev-shm-usage',
                '--mute-audio', 'about:blank',
            ], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, **options)
            processes.append(process)
            deadline = time.monotonic() + 30
            while not port_file.exists() and time.monotonic() < deadline:
                assert process.poll() is None, 'Owned browser exited before exposing its local endpoint'
                time.sleep(.05)
            assert port_file.exists(), 'Owned browser endpoint did not become ready'
            port = int(port_file.read_text(encoding='ascii').splitlines()[0])
            assert 0 < port < 65536
            connection = browser_type.connect_over_cdp(f'http://127.0.0.1:{port}')
            connections.append(connection)
            context = connection.contexts[0]
            context.add_init_script("localStorage.setItem('slimeybeats.interface-preferences.tutorial-dismissed','declined')")
            context.on('request', lambda request: requests.append({
                'method': request.method, 'url': request.url,
                'body': len(request.post_data_buffer or b'')}))
            page = context.new_page(); page.on('dialog', lambda dialog: dialog.accept())
            page.goto(base, wait_until='networkidle'); ready(page)
            return process, page

        try:
            process, page = launch()
            evidence['browser'] = connections[-1].version
            page.locator('#file').set_input_files(fixture('Before forced crash.wav', seconds=.2))
            expect(page.locator('#master')).to_be_enabled()
            page.locator('#master').click(); expect(page.locator('#export')).to_be_enabled(timeout=60000)
            with page.expect_download() as event: page.locator('#export').click()
            before = proof.parent / 'process-before-crash.wav'; event.value.save_as(str(before))
            open_recovery(page); page.locator('#recovery-enable').click(); save_point(page)
            owner = page.evaluate("async () => (await (new (await import('/recovery-store.js')).RecoveryStore()).list())[0].owner")
            name = 'slimeybeats-recovery:' + owner
            page.wait_for_function('(name) => navigator.locks.query().then(s => s.held.some(l => l.name === name))', arg=name)
            evidence['lock_held_before_crash'] = True
            evidence['terminated_exit_code'] = terminate(process)
            assert evidence['terminated_exit_code'] != 0
            evidence['owned_process_abruptly_terminated'] = True
            _, fresh = launch()
            assert not fresh.evaluate('(name) => navigator.locks.query().then(s => [...s.held,...s.pending].some(l => l.name === name))', name)
            evidence['old_process_lock_absent'] = True
            fresh.locator('#recovery-offer-open').click(); fresh.locator('#recovery-points button').first.click()
            expect(fresh.locator('#recovery-action-status')).to_contain_text('Restored with playback stopped', timeout=30000)
            fresh.locator('#recovery-close').click()
            expect(fresh.locator('#filename')).to_have_text('Before forced crash.wav')
            expect(fresh.locator('#play')).to_have_text('Play')
            with fresh.expect_download() as event: fresh.locator('#export').click()
            after = proof.parent / 'process-after-crash.wav'; event.value.save_as(str(after))
            assert before.read_bytes() == after.read_bytes(), 'Recovered browser-process export changed'
            evidence['exact_export_restored'] = True
            assert all(r['method'] == 'GET' and r['body'] == 0 and (r['url'].startswith(base) or r['url'] == 'about:blank') for r in requests), requests
            evidence['application_requests_static_local_get_only'] = True
            evidence['passed'] = True
        finally:
            try:
                for process in processes: terminate(process)
                for connection in reversed(connections):
                    try: connection.close()
                    except Exception:
                        if any(p.poll() is None for p in processes): raise
            finally: proof.write_text(json.dumps(evidence, indent=2) + '\n', encoding='utf-8')
    return ['forced owned Chromium process termination: persisted profile restores exact exported audio with stopped playback']
