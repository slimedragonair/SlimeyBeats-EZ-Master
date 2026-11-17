"""Run the complete browser inventory in isolated CI shards, without changing assertions."""
import argparse
import json
from pathlib import Path
import subprocess
import time

BROWSERS = ('chromium', 'firefox', 'webkit')
GROUPS = ('core', 'studio', 'learning')
# Every invocation from the former serial job is retained. Browser-neutral checks
# run once (Chromium shard); all cross-engine checks still run on all engines.
SPECS = {
    'core': (
        ('verify_recovery_browser.py', 'recovery-{browser}', ('--site', 'web/dist')),
        ('verify_gapless.py', 'gapless-{browser}', ()),
        ('verify_imports.py', 'imports-{browser}', ()),
        ('verify_browser.py', '{browser}', ()),
        ('verify_bootstrap_browser.py', 'bootstrap-{browser}', ()),
        ('verify_eq_preview.py', 'eq-{browser}.json', ()),
        ('verify_eq_suggestions.py', 'eq-suggestions-{browser}', ('--site', 'web/dist')),
        ('verify_mp3_export.py', 'mp3-{browser}', ('--integration',)),
        ('verify_session_export.py', None, ()),
        ('verify_export_workflows.py', 'exports-{browser}', ()),
    ),
    'studio': (
        ('verify_remote_media.py', 'remote-media-{browser}', ('--site', 'web/dist', '--cli', 'rust/target/release/slimeybeats-ez-master-cli', '--worker', 'rust/target/release/slimeybeats_hybrid_worker')),
        ('verify_remote_studio.py', 'remote-studio-{browser}', ('--site', 'web/dist', '--cli', 'rust/target/release/slimeybeats-ez-master-cli', '--worker', 'rust/target/release/slimeybeats_hybrid_worker')),
        ('verify_remote_migration.py', 'remote-migration-{browser}', ('--site', 'web/dist', '--cli', 'rust/target/release/slimeybeats-ez-master-cli', '--worker', 'rust/target/release/slimeybeats_hybrid_worker')),
        ('verify_studio_browser.py', 'studio-{browser}', ()),
        ('verify_region_edit_browser.py', 'stem-editor-{browser}', ()),
        ('verify_doctor_browser.py', 'doctor-{browser}', ()),
        ('verify_studio_live.py', 'studio-live-{browser}', ()),
    ),
    'learning': (
        ('verify_learning_browser.py', 'learning-{browser}', ()),
    ),
}
ONCE = {
    'core': (
        ('verify_session_export_lifecycle.py', None, ()),
        ('verify_memory.py', 'replacement', ('--tracks', '4', '--seconds', '10')),
    ),
    'studio': (
        ('verify_studio_memory.py', 'studio-memory', ('--tracks', '4', '--seconds', '10', '--check-limits')),
    ),
    'learning': (),
}


def commands(browser, group, out):
    if browser not in BROWSERS or group not in GROUPS:
        raise ValueError('Unknown shard')
    result = []
    for filename, output, extra in SPECS[group]:
        cmd = ['python3', 'web/tools/'+filename, '--browser', browser, *extra]
        if output: cmd += ['--out', str(out/output.format(browser=browser))]
        result.append(cmd)
    if browser == 'chromium':
        for filename, output, extra in ONCE[group]:
            cmd = ['python3', 'web/tools/'+filename, *extra]
            if output: cmd += ['--out', str(out/output)]
            result.append(cmd)
    return result


def run(browser, group, out):
    out.mkdir(parents=True, exist_ok=True)
    for cmd in commands(browser, group, out):
        print("Starting: " + " ".join(cmd), flush=True)
        started = time.monotonic()
        code = 1
        try:
            code = subprocess.run(cmd, check=False).returncode
        finally:
            with (out/'shard-timings.jsonl').open('a', encoding='utf-8') as stream:
                stream.write(json.dumps({'browser': browser, 'group': group,
                    'command': cmd, 'seconds': round(time.monotonic()-started, 3),
                    'returncode': code})+'\n')
        if code:
            raise SystemExit(code)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--browser', choices=BROWSERS, required=True)
    p.add_argument('--group', choices=GROUPS, required=True)
    p.add_argument('--out', type=Path, required=True)
    p.add_argument('--list', action='store_true')
    a = p.parse_args()
    if a.list: print(json.dumps(commands(a.browser, a.group, a.out), indent=2))
    else: run(a.browser, a.group, a.out)
