"""Capture the actual native renderer using a private canonical project and prefs.

Does not start playback, export audio, install software or use real recordings.
On Linux run beneath xvfb-run. Physical listening remains separate acceptance.
"""
import argparse
import hashlib
import json
from pathlib import Path
from arrangement_capture_fixture import create
from verify_recovery import Verifier, require, checkpoints


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--gui', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--layout-only', action='store_true', help='Capture layouts; skip the abrupt restart test')
    args = parser.parse_args()
    require(not args.out.exists(), 'Use a new private evidence directory')
    args.out.mkdir(parents=True)
    project = create(args.out / 'Synthetic project')
    hashes = {str(file.relative_to(project.parent)): hashlib.sha256(file.read_bytes()).hexdigest()
              for file in project.parent.rglob('*') if file.is_file()}
    verifier = Verifier(args.gui.resolve(), args.out)
    records = []
    for size in [(1100, 720), (1480, 940)]:
        record = verifier.run(f'tracks-{size[0]}', ['--studio-session', project, '--view', 'tracks'], size)
        state = record['state']
        require((state['arrangement_tracks'], state['arrangement_clips'], state['arrangement_effects']) == (4, 4, 1), 'Capture did not load the complete canonical project')
        require(state['playback_ready'], 'Track player was not prepared')
        records.append(record)
    if not args.layout_only:
        crashed = verifier.run('tracks-before-crash', ['--studio-session', project, '--view', 'tracks',
                               '--capture-wait-recovery', '--capture-stay-open'], (1100, 720), crash=True)
        saved_before = checkpoints(verifier.recovery)
        runs_before = {point['run'] for point in saved_before}
        before = max((point for point in saved_before
                      if not (verifier.recovery / 'runs' / point['run'] / 'clean.json').exists()),
                     key=lambda point: point['generation'])
        # Restore must use retained private attachments, not reopen the project path.
        moved = project.with_name('original-project-moved.json'); project.rename(moved)
        restored = verifier.run('tracks-after-crash', ['--capture-restore-recovery', '--capture-wait-recovery'], (1100, 720))
        after = max((point for point in checkpoints(verifier.recovery) if point['run'] not in runs_before),
                    key=lambda point: point['generation'])
        require(restored['state']['recovery_restored'], 'Arrangement recovery was not restored')
        require((restored['state']['arrangement_tracks'], restored['state']['arrangement_clips'],
                 restored['state']['arrangement_effects']) == (4, 4, 1), 'Restoration lost timeline metadata')
        require(before['payload']['studio']['arrangement']['history'] == after['payload']['studio']['arrangement']['history'],
                'Restoration changed committed current/undo/redo metadata')
        require(before['audio'] == after['audio'], 'Restoration changed source/comparison attachment identities')
        records.extend([crashed, restored])
        moved.rename(project)
    for name, sha in hashes.items():
        require(hashlib.sha256((project.parent / name).read_bytes()).hexdigest() == sha, 'Capture changed source/project bytes')
    result = dict(passed=True, gui_sha256=hashlib.sha256(verifier.gui.read_bytes()).hexdigest(),
                  abrupt_recovery_tested=not args.layout_only,
                  scope='actual native renderer with synthetic v2 project; no physical listening or manual acceptance', cases=records)
    (args.out / 'results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(dict(passed=True, cases=len(records), unchanged_project=True)))


if __name__ == '__main__':
    main()
