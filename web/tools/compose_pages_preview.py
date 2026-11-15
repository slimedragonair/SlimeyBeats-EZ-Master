"""Preserve retained main website bytes and overlay a fresh current-work preview.

The CLI reads GH_TOKEN and prints a JSON hash report to stdout, outside the site.
The caller must serialize deployment against main; composition cannot lock Pages.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import tempfile

from pages_artifacts import (GitHubClient, MAX_BYTES, MAX_FILES, REPOSITORY, SHA,
                             assert_main_source, copy_bounded, extract_main, safe_parts)
from pages_baseline import retrieve_main, write_baseline


def checked_path(path):
    path = Path(os.path.abspath(path))
    for ancestor in (path, *path.parents):
        if ancestor.is_symlink() or (hasattr(ancestor, 'is_junction') and ancestor.is_junction()):
            raise ValueError('Symlinks and junctions are not allowed in site paths')
    return path.resolve()


def preview_files(root):
    files, seen, total = [], set(), 0
    def fail(error):
        raise error
    for directory, directories, names in os.walk(root, followlinks=False, onerror=fail):
        for name in sorted(directories + names):
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            parts = safe_parts(relative)
            key = '/'.join(parts).casefold()
            kind = path.lstat()
            if key == 'preview-build.json':
                raise ValueError('Preview must not contain stale preview-build.json')
            if (key in seen or len(seen) >= MAX_FILES or path.is_symlink()
                    or (hasattr(path, 'is_junction') and path.is_junction())
                    or not (stat.S_ISDIR(kind.st_mode) or stat.S_ISREG(kind.st_mode))):
                raise ValueError('Unsafe or duplicate preview entry')
            seen.add(key)
            if stat.S_ISREG(kind.st_mode):
                total += kind.st_size
                if total > MAX_BYTES:
                    raise ValueError('Preview exceeds byte limit')
                files.append((relative, path))
    if not any(relative == 'index.html' for relative, _ in files):
        raise ValueError('Preview site has no root index.html')
    return files


def hash_file(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def runtime_version(preview):
    assets = re.findall(r'''(?:src|href)\s*=\s*["']([^"']+\.(?:js|css)(?:[?#][^"']*)?)["']''',
                        (preview / 'index.html').read_text(encoding='utf-8'), re.IGNORECASE)
    versions = [re.fullmatch(r'[^?#]+\.(?:js|css)\?v=([0-9a-f]{16})', asset) for asset in assets]
    if not versions or any(version is None for version in versions):
        raise ValueError('Built preview requires 16-hex ?v= versions on every JS/CSS asset')
    tokens = {version[1] for version in versions}
    if len(tokens) != 1:
        raise ValueError('Built preview JS/CSS runtime versions do not agree')
    return tokens.pop()


def assert_preview_head(client, commit):
    if client.api('git/ref/heads/current-work')['object']['sha'] != commit:
        raise ValueError('Preview commit no longer matches current-work head')


def verify_report(report, repository, client=None):
    if repository != REPOSITORY or report.get('repository') != repository:
        raise ValueError('Report repository is not the authorized repository')
    metadata, expected = report['preview'], report['main_files']['index.html']
    if metadata['branch'] != 'current-work' or not SHA.fullmatch(metadata['commit']):
        raise ValueError('Report has invalid preview identity')
    if not re.fullmatch(r'[0-9a-f]{16}', metadata['runtime_version']):
        raise ValueError('Report has invalid runtime version')
    if not re.fullmatch(r'[0-9a-f]{64}', expected):
        raise ValueError('Report has invalid main index hash')
    client = client or GitHubClient(repository)
    assert_preview_head(client, metadata['commit'])
    run = assert_main_source(client, metadata['main_run_id'], metadata['main_commit'],
                             report['main_artifact_id'])
    if hashlib.sha256(client.published_index()).hexdigest() != expected:
        raise ValueError('Published root differs from selected main index; refusing rollback')
    return {'verified': True, 'commit': metadata['commit'], 'main_run_id': run['id']}


def compose(preview_site, out, repository, commit, run_url, client=None, baseline_out=None):
    if repository != REPOSITORY or not SHA.fullmatch(commit):
        raise ValueError('Expected authorized repository and full lowercase commit SHA')
    if not re.fullmatch(r'https://github\.com/' + re.escape(REPOSITORY) +
                        r'/actions/runs/[1-9][0-9]*', run_url):
        raise ValueError('Expected a workflow run URL in the authorized repository')
    preview, destination = checked_path(preview_site), checked_path(out)
    if preview == destination or preview in destination.parents or destination in preview.parents:
        raise ValueError('Preview and output paths must not overlap')
    if not preview.is_dir():
        raise ValueError('Preview site must be a directory')
    if destination.exists() and (not destination.is_dir() or any(destination.iterdir())):
        raise ValueError('Output must be a fresh or empty directory')
    baseline = checked_path(baseline_out) if baseline_out is not None else None
    if baseline is not None and (baseline.exists() or any(
            baseline == root or baseline in root.parents or root in baseline.parents
            for root in (preview, destination))):
        raise ValueError('Baseline must be a fresh directory outside both site trees')
    files = preview_files(preview)
    version = runtime_version(preview)
    client = client or GitHubClient(repository)
    assert_preview_head(client, commit)
    with tempfile.TemporaryFile() as archive, tempfile.TemporaryDirectory(prefix='pages-main-') as temporary:
        run, artifact = retrieve_main(client, archive)
        staging = Path(temporary)
        main_manifest = extract_main(archive, staging)
        if client.published_index() != (staging / 'index.html').read_bytes():
            raise ValueError('Published root differs from main artifact; refusing rollback')
        metadata = {'branch': 'current-work', 'commit': commit, 'runtime_version': version,
                    'built_at': datetime.now(timezone.utc).isoformat().replace('+00:00', 'Z'),
                    'run_url': run_url, 'main_commit': run['head_sha'], 'main_run_id': run['id']}
        # Repeat selection immediately before writing output to detect main updates.
        assert_main_source(client, run['id'], run['head_sha'], artifact['id'])
        assert_preview_head(client, commit)
        if client.published_index() != (staging / 'index.html').read_bytes():
            raise ValueError('Published root changed during composition; retry')
        checked_path(destination)
        destination.mkdir(parents=True, exist_ok=True)
        if any(destination.iterdir()):
            raise ValueError('Output is no longer empty')
        for relative in main_manifest:
            target = destination / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with (staging / relative).open('rb') as source, target.open('xb') as output:
                copy_bounded(source, output)
        preview_manifest, preview_root = {}, destination / 'current-work'
        preview_root.mkdir()
        for relative, path in files:
            checked_path(path)
            target = preview_root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            with path.open('rb') as source, target.open('xb') as output:
                copy_bounded(source, output)
            preview_manifest['current-work/' + relative] = hash_file(target)
        if runtime_version(preview_root) != version:
            raise ValueError('Preview index runtime version changed during composition')
        metadata_path = preview_root / 'preview-build.json'
        metadata_path.write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8')
        preview_manifest['current-work/preview-build.json'] = hash_file(metadata_path)
        if any(hash_file(destination / name) != digest for name, digest in main_manifest.items()):
            raise ValueError('Stable root file verification failed')
        if baseline is not None:
            write_baseline(archive, baseline, run, artifact)
    return {'repository': repository, 'preview': metadata, 'main_artifact_id': artifact['id'],
            'main_files': main_manifest, 'preview_files': preview_manifest}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--preview-site', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--repository', required=True, choices=[REPOSITORY])
    parser.add_argument('--commit')
    parser.add_argument('--run-url')
    parser.add_argument('--report', type=Path, help='Write hash report outside both site trees')
    parser.add_argument('--verify-report', type=Path, help='Recheck report immediately before deployment')
    parser.add_argument('--baseline-out', type=Path, help='Retain exact main ZIP and provenance outside the site')
    args = parser.parse_args()
    try:
        if args.verify_report:
            if any((args.preview_site, args.out, args.commit, args.run_url, args.report, args.baseline_out)):
                raise ValueError('--verify-report only accepts --repository')
            report = verify_report(json.loads(args.verify_report.read_text(encoding='utf-8')),
                                   args.repository)
        else:
            if not all((args.preview_site, args.out, args.commit, args.run_url)):
                raise ValueError('Composition requires --preview-site, --out, --commit and --run-url')
            if args.report:
                report_path = checked_path(args.report)
                if not report_path.parent.is_dir() or report_path.exists() or any(
                        root == report_path or root in report_path.parents or report_path in root.parents
                        for root in [checked_path(path) for path in
                                     (args.out, args.preview_site, args.baseline_out) if path is not None]):
                    raise ValueError('Report must be a new file outside both site trees')
            report = compose(args.preview_site, args.out, args.repository, args.commit, args.run_url,
                             baseline_out=args.baseline_out)
            if args.report:
                with report_path.open('x', encoding='utf-8') as output:
                    json.dump(report, output, indent=2, sort_keys=True)
                    output.write('\n')
    except (ValueError, OSError, EOFError, KeyError, TypeError) as error:
        parser.exit(1, f'Pages composition refused: {error}\n')
    print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == '__main__':
    main()
