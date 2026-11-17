"""Read-only fail-closed retained-work inventory. Never prints private paths/IDs.

A zero count is NOT a restart lock: admission must be drained/paused and the
inventory repeated immediately before interruption. Never delete work here.
"""
import argparse
import json
from pathlib import Path
import re
import stat

JOB_DIRECTORY = re.compile(r'job-[a-f0-9]{32}\Z')
ROOT_FILES = frozenset(('.slimeybeats-hybrid-root', '.server.lock'))


def inventory(root):
    root = Path(root)
    result = {'retained_directories': 0, 'unexpected_entries': 0,
              'inspection_complete': False, 'restart_candidate': False}
    try:
        if not stat.S_ISDIR(root.lstat().st_mode):
            return result
        for entry in root.iterdir():
            mode = entry.lstat().st_mode
            if stat.S_ISDIR(mode):
                result['retained_directories'] += 1
                if not JOB_DIRECTORY.fullmatch(entry.name):
                    result['unexpected_entries'] += 1
            elif not stat.S_ISREG(mode) or entry.name not in ROOT_FILES:
                result['unexpected_entries'] += 1
        result['inspection_complete'] = True
        result['restart_candidate'] = not (result['retained_directories'] or result['unexpected_entries'])
    except OSError:
        pass  # Missing/unreadable/racing storage is not proof that it is empty.
    return result


def require_empty(root):
    if not inventory(root)['restart_candidate']:
        raise RuntimeError('Retained or unverified server work prevents interruption')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('storage', type=Path)
    args = parser.parse_args()
    value = inventory(args.storage)
    print(json.dumps(value))
    raise SystemExit(0 if value['restart_candidate'] else 1)
