"""Seal/verify same-run CI build inputs; never reuse a different commit's outputs."""
import argparse
import hashlib
import json
from pathlib import Path

BINARIES = ('slimeybeats-ez-master-cli', 'slimeybeats_hybrid_worker')


def payload_paths(root, kind):
    site = root / 'web/dist'
    if site.is_symlink() or not site.is_dir() or not (site / 'index.html').is_file():
        raise ValueError('Missing built site')
    paths = sorted(p for p in site.rglob('*') if p.is_file() or p.is_symlink())
    profile = 'release' if kind == 'web' else 'debug'
    paths += [root / 'rust/target' / profile / name for name in BINARIES]
    for path in paths:
        if path.is_symlink() or not path.is_file():
            raise ValueError(f'Missing or symlink payload: {path}')
    return paths


def digest(path):
    h = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(chunk)
    return h.hexdigest()


def snapshot(root, kind):
    return {str(p.relative_to(root)): {'sha256': digest(p), 'size': p.stat().st_size}
            for p in payload_paths(root, kind)}


def seal(root, kind, sha, run_id, attempt, manifest):
    result = {'schema': 1, 'kind': kind, 'sha': sha, 'run_id': run_id,
              'attempt': attempt, 'files': snapshot(root, kind)}
    manifest.write_text(json.dumps(result, sort_keys=True, indent=2) + '\n', encoding='utf-8')


def verify(root, kind, sha, run_id, attempt, manifest):
    data = json.loads(manifest.read_text(encoding='utf-8'))
    expected = {'schema': 1, 'kind': kind, 'sha': sha, 'run_id': run_id, 'attempt': attempt}
    if any(data.get(k) != v for k, v in expected.items()):
        raise ValueError('Build provenance mismatch')
    if data.get('files') != snapshot(root, kind):
        raise ValueError('Payload content/size/file-set mismatch')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('action', choices=['create', 'verify'])
    p.add_argument('--root', type=Path, default=Path('.'))
    p.add_argument('--kind', choices=['web', 'arrangement'], required=True)
    p.add_argument('--sha', required=True)
    p.add_argument('--run-id', required=True)
    p.add_argument('--attempt', required=True)
    p.add_argument('--manifest', type=Path, required=True)
    a = p.parse_args()
    (seal if a.action == 'create' else verify)(a.root, a.kind, a.sha, a.run_id, a.attempt, a.manifest)
