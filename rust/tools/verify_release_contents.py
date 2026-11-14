"""Audit shipped components, local guide links, notices and the clean source ZIP."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import zipfile

from release_packaging.documentation_links import verify_local_links
from release_packaging.public_source import DEMO_FILES, GUIDES

BINARY_NAMES = ['SlimeyBeats-EZ-Master.exe', 'SlimeyBeats-EZ-Master-CLI.exe',
                'slimeybeats-mcp.exe', 'slimeybeats-vst3-host.exe']


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(folder, source_zip):
    required = [*BINARY_NAMES, 'Connect AI Assistant.bat', 'Connect-AI-Assistant.ps1',
                'LICENSE', 'OUTPUT_OWNERSHIP.md', 'THIRD_PARTY_NOTICES.md', 'Examples/CREDITS.md']
    required.extend('docs/' + name for name in GUIDES)
    assert all((folder / name).is_file() for name in required)
    for relative in ['', 'MCP-dependencies', 'Plugin-dependencies']:
        root = folder / relative
        packages = json.loads((root / 'THIRD-PARTY.json').read_text(encoding='utf-8'))
        assert packages
        for package in packages:
            for name in package['license_files']:
                path = (root / name).resolve()
                assert path.is_relative_to(root.resolve()) and path.is_file(), path

    checked_links = verify_local_links(
        [*(folder / 'docs' / name for name in GUIDES), folder / 'THIRD_PARTY_NOTICES.md'], folder,
    )

    with zipfile.ZipFile(source_zip) as archive:
        assert archive.testzip() is None
        names = archive.namelist()
        roots = {PurePosixPath(name).parts[0] for name in names}
        assert len(roots) == 1
        prefix = next(iter(roots)) + '/'
        relative = {name.removeprefix(prefix): name for name in names}
        banned = {'.git', 'target', '__pycache__', '.venv', 'env', 'models', 'user_data', 'output'}
        for name in relative:
            path = PurePosixPath(name)
            assert not path.is_absolute() and '..' not in path.parts
            assert not banned.intersection(path.parts), name
            assert path.suffix.lower() not in {'.exe', '.dll', '.so', '.pt', '.pth', '.onnx'}, name
            if path.suffix.lower() in {'.wav', '.mp3', '.flac', '.m4a', '.aiff', '.ogg'}:
                assert name in {'examples/audio/' + value for value in DEMO_FILES}, name
        for name in ['rust/Cargo.toml', 'web/dsp/src/studio.rs', 'mcp/server/src/studio_protocol.rs',
                     'plugins/host/src/output_channel.rs', 'rust/assets/learning-guide.json',
                     *('docs/' + guide for guide in GUIDES)]:
            assert name in relative, name
        for name in DEMO_FILES:
            assert archive.read(relative['examples/audio/' + name]) == (folder / 'Examples' / name).read_bytes()
        source_files = len(names)
    return {'passed': True, 'components': {name: digest(folder/name) for name in BINARY_NAMES},
            'guides_checked': len(GUIDES), 'local_guide_links_checked': checked_links, 'source_files': source_files,
            'source_archive_sha256': digest(source_zip), 'example_provenance_files_match': True,
            'private_data_models_build_products_excluded': True, 'dependency_license_files_present': True}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--folder', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    result = verify(args.folder.resolve(), args.source.resolve())
    text = json.dumps(result, indent=2)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(text, encoding='utf-8')
    print(text)
