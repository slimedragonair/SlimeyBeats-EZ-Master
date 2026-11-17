"""Freeze native probe images so Windows builds can replace mutable targets."""
import hashlib
import json
from pathlib import Path
import secrets
import shutil


def frozen_worker(worker, output):
    worker = Path(worker).resolve(strict=True)
    cli = worker.with_name('slimeybeats-ez-master-cli' + worker.suffix)
    directory = Path(output) / ('native-binaries-' + secrets.token_hex(8))
    directory.mkdir()
    records = []
    for source in (worker, cli):
        target = directory / source.name
        shutil.copy2(source, target)
        with target.open('rb') as copied:
            digest = hashlib.file_digest(copied, 'sha256').hexdigest()
        with source.open('rb') as original:
            if hashlib.file_digest(original, 'sha256').hexdigest() != digest:
                raise RuntimeError('Native binary changed while freezing the fixture')
        records.append({'file': target.name, 'sha256': digest, 'bytes': target.stat().st_size})
    (directory / 'sha256.json').write_text(json.dumps(records, indent=2) + '\n', encoding='utf-8')
    return directory / worker.name
