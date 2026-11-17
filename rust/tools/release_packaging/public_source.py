"""Export an explicit public-source allowlist instead of archiving a development checkout."""
from pathlib import Path
import shutil

ROOT_FILES = ('README.md', 'LICENSE', 'OUTPUT_OWNERSHIP.md', 'THIRD_PARTY_NOTICES.md',
              'CONTRIBUTING.md', 'SECURITY.md', 'CHANGELOG.md', '.gitignore', '.gitattributes', '.editorconfig',
              'run-slimeybeats.bat', 'run-rust.bat', 'cli-rust.bat',
              'run.bat', 'cli.bat', 'setup.bat', 'requirements.txt')
GUIDES = (
    'INSTALLATION.md', 'USER_GUIDE.md', 'BUILDING.md', 'RELEASING.md', 'GETTING_STARTED.html',
    'LINUX.md', 'WEB.md', 'WEB_VALIDATION.md', 'WEB_EXPORTS.md', 'WEB_EXPORT_VALIDATION.md',
    'BROWSER_PARITY_ROADMAP.md', 'MOBILE_PREVIEW.md', 'MCP.md', 'MCP_ARCHITECTURE.md',
    'MCP_VALIDATION.md', 'STUDIO.md', 'STEM_LIVE_MONITORING.md', 'STEM_EDITOR.md',
    'WEB_STEM_EDITOR.md', 'STEM_DOCTOR.md', 'UPDATES_AND_OPTIONAL_TOOLS.md',
    'RECOVERY_AND_REPORTING.md', 'STUDIO_DEVELOPMENT.md', 'STUDIO_BETA3_VALIDATION.md',
    'MERGE_VALIDATION.md', 'LEARNING_AND_HELP.md', 'UX_DEVELOPMENT.md', 'UX_VALIDATION.md',
    'HOTFIX4.md', 'HYBRID_STUDIO.md', 'HYBRID_REMOTE_PROTOCOL.md', 'HYBRID_V2_VALIDATION.md',
    'EQ_SUGGESTION_INTEGRATION.md',
    'ARRANGEMENT_CORE.md', 'ARRANGEMENT_TRANSPORT.md', 'ARRANGEMENT_NATIVE_AUDIO.md', 'WEB_ARRANGEMENT_UI.md',
    'ARRANGEMENT_NATIVE_UI.md',
    'ARRANGEMENT_RECOVERY.md',
    'NATIVE_STUDIO_NAVIGATION.md',
)
# Keep screenshots explicit: only reviewed public captures belong in the source
# package, never an arbitrary image or private report added to the checkout.
DOCUMENTATION_IMAGES = (
    'arrangement-desktop.png', 'arrangement-compact.png',
    'arrangement-eq-desktop.png', 'arrangement-eq-compact.png',
    'arrangement-eq-scrolled-desktop.png', 'arrangement-eq-scrolled-compact.png',
    'welcome.png', 'manual-eq.png', 'manual-eq-detail.png',
    'linux-manual-eq.png', 'linux-compact-eq.png', 'linux-compact-controls.png',
    'web-preview.png', 'web-phone-eq.png', 'studio-stems.png', 'studio-repair.png',
    'studio-plugins.png', 'studio-web-phone.png', 'studio-large-import.png',
    'ux-first-run.png', 'ux-manual-eq.png', 'ux-web-help.png', 'updates-and-tools.png',
    'recovery-desktop.png', 'private-report-desktop.png', 'recovery-web.png',
    'private-report-web-phone.png', 'web-basic-overview.png', 'web-phone-master.png',
    'stem-editor-timeline.png', 'stem-editor-compact.png', 'stem-editor-selection.jpg',
    'web-stem-editor-phone.png', 'web-stem-editor-tools.png', 'doctor-desktop.png',
    'doctor-web-findings.png', 'doctor-web-repair.png', 'web-session-downloads.png',
    'hybrid-server-job.png', 'hybrid-server-master.png', 'hybrid-shared-editor-phone.png',
)
DEMO_FILES = ('demo-input.wav', 'demo-master.wav', 'listen-input.mp3', 'listen-master.mp3',
              'CREDITS.md', 'source.json', 'demo-master.report.json', 'Listen.html')
HYBRID_DEPLOYMENT_FILES = (
    'hybrid/deploy/cloudflare-source-findings.json',
    'hybrid/deploy/slimeybeats-cloudflared.sysusers.conf',
    'hybrid/deploy/relay/home-relay.sysusers.conf',
    'hybrid/deploy/relay/vps-tunnel.sysusers.conf',
)

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
    for name in DOCUMENTATION_IMAGES:
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
    for directory in ('server', 'tests', 'deploy'):
        for path in sorted((repository / 'hybrid' / directory).rglob('*')):
            if (path.is_file() and '__pycache__' not in path.parts
                    and (path.suffix in ('.py', '.md', '.service', '.example')
                         or path.relative_to(repository).as_posix() in HYBRID_DEPLOYMENT_FILES)):
                copy(path.relative_to(repository))
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
    copy(Path('mcp/ARRANGEMENT_PROJECTS.md'))
    for directory in ('mcp/server/src', 'mcp/server/tests', 'mcp/server/examples', 'mcp/tools'):
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
