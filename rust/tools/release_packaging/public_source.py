"""Export an explicit public-source allowlist instead of archiving a development checkout."""
from pathlib import Path
import shutil

ROOT_FILES = ('README.md', 'LICENSE', 'OUTPUT_OWNERSHIP.md', 'THIRD_PARTY_NOTICES.md',
              'CONTRIBUTING.md', 'SECURITY.md', 'CHANGELOG.md', '.gitignore', '.gitattributes', '.editorconfig',
              'run-slimeybeats.bat', 'run-rust.bat', 'cli-rust.bat',
              'run.bat', 'cli.bat', 'setup.bat', 'requirements.txt')
GUIDES = ('INSTALLATION.md', 'USER_GUIDE.md', 'BUILDING.md', 'RELEASING.md', 'GETTING_STARTED.html', 'LINUX.md', 'WEB.md', 'WEB_VALIDATION.md', 'MCP.md', 'MCP_ARCHITECTURE.md', 'MCP_VALIDATION.md', 'STUDIO.md', 'STUDIO_DEVELOPMENT.md')
DEMO_FILES = ('demo-input.wav', 'demo-master.wav', 'listen-input.mp3', 'listen-master.mp3',
              'CREDITS.md', 'source.json', 'demo-master.report.json', 'Listen.html')

def copy_public_source(repository, destination):
    repository, destination = Path(repository).resolve(), Path(destination).resolve()
    if destination.exists():
        raise FileExistsError(destination)
    destination.mkdir(parents=True)
    def copy(relative):
        source = repository / relative
        if not source.is_file():
            raise FileNotFoundError(source)
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    for name in ROOT_FILES:
        copy(Path(name))
    for name in GUIDES:
        copy(Path('docs') / name)
    for name in ('welcome.png', 'manual-eq.png', 'manual-eq-detail.png',
                 'linux-manual-eq.png', 'linux-compact-eq.png', 'linux-compact-controls.png', 'web-preview.png', 'web-phone-eq.png',
                 'studio-stems.png', 'studio-repair.png', 'studio-plugins.png', 'studio-web-phone.png'):
        copy(Path('docs/images') / name)
    for name in ('python-guide.md', 'python-third-party-notices.md', 'python-changelog.md'):
        copy(Path('docs/legacy') / name)
    for path in sorted((repository / 'mastercraft').rglob('*.py')):
        if '__pycache__' not in path.parts:
            copy(path.relative_to(repository))
    copy(Path('mastercraft/data/builtin_profiles.json'))
    for name in ('make_test_signal.py', 'test_regressions.py'):
        copy(Path('test') / name)
    for directory in ('src', 'tests', 'examples', 'tools', 'assets', 'docs'):
        for path in sorted((repository / 'rust' / directory).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                copy(path.relative_to(repository))
    for name in ('Cargo.toml', 'Cargo.lock', 'README.md', 'LICENSE', '.gitignore'):
        copy(Path('rust') / name)
    for name in DEMO_FILES:
        copy(Path('examples/audio') / name)
    for directory in ('site', 'tools'):
        for path in sorted((repository / 'web' / directory).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                copy(path.relative_to(repository))
    for name in ('Cargo.toml', 'Cargo.lock'):
        copy(Path('web/dsp') / name)
    for path in sorted((repository / 'web/dsp/src').rglob('*')):
        if path.is_file():
            copy(path.relative_to(repository))
    for name in ('Cargo.toml', 'Cargo.lock'):
        copy(Path('mcp/server') / name)
    for directory in ('mcp/server/src', 'mcp/server/tests', 'mcp/tools'):
        for path in sorted((repository / directory).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                copy(path.relative_to(repository))
    for path in sorted((repository / '.github').rglob('*')):
        if path.is_file():
            copy(path.relative_to(repository))
    for name in ('Cargo.toml', 'Cargo.lock'):
        copy(Path('plugins/host') / name)
    for directory in ('plugins/host/src', 'plugins/host/tools'):
        for path in sorted((repository / directory).rglob('*')):
            if path.is_file() and '__pycache__' not in path.parts and path.suffix != '.pyc':
                copy(path.relative_to(repository))
    return destination
