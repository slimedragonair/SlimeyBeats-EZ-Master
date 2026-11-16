"""Keep offline guides, screenshots and attribution links valid in each bundle."""
from pathlib import Path
import re
import shutil

from .public_source import DOCUMENTATION_IMAGES, GUIDES


def offline_guide(text):
    text = text.replace('../examples/audio/', '../Examples/').replace('examples/audio/demo-input.wav', 'Examples/demo-input.wav')
    # Runtime bundles do not contain the separate source archive. Keep contributor
    # references as explicit repository paths rather than broken local links.
    text, count = re.subn(r'(?<!!)\[[^\]]+\]\(\.\./((?:rust|web|mcp|plugins|hybrid|\.github)/[^)]+)\)', lambda match: '`' + match[1] + '`', text)
    if count:
        text += '\n\nContributor file paths above refer to the separate source package.\n'
    return text


def copy_documentation(repository, destination):
    repository, destination = Path(repository), Path(destination)
    project = repository / 'rust'
    docs = destination / 'docs'
    shutil.copytree(project / 'docs', docs)
    for name in GUIDES:
        text = (repository / 'docs' / name).read_text(encoding='utf-8')
        text = offline_guide(text)
        (docs / name).write_text(text, encoding='utf-8')
    images = docs / 'images'
    images.mkdir(exist_ok=True)
    for name in DOCUMENTATION_IMAGES:
        shutil.copy2(repository / 'docs/images' / name, images / name)
    shutil.copytree(repository / 'docs/legacy', docs / 'legacy')
    shutil.copy2(project / 'tests/fixtures/README.md', docs / 'fixture-credits.md')
    shutil.copy2(project / 'assets/ASSETS.md', docs / 'ASSETS.md')
    for name in ['slime-frame-generation.md', 'sovereign-skin-generation.md']:
        source = project / 'assets' / name
        if source.is_file(): shutil.copy2(source, docs / name)


def copy_project_notices(repository, destination):
    repository, destination = Path(repository), Path(destination)
    shutil.copy2(repository / 'LICENSE', destination / 'LICENSE')
    policy = (repository / 'OUTPUT_OWNERSHIP.md').read_text(encoding='utf-8').replace('examples/audio/', 'Examples/')
    (destination / 'OUTPUT_OWNERSHIP.md').write_text(policy, encoding='utf-8')
    text = (repository / 'THIRD_PARTY_NOTICES.md').read_text(encoding='utf-8')
    text = text.replace('examples/audio/', 'Examples/').replace('rust/assets/ASSETS.md', 'docs/ASSETS.md').replace('rust/tests/fixtures/README.md', 'docs/fixture-credits.md')
    text = text.replace('[`web/site/audio/encoders`](web/site/audio/encoders/README.md)',
                        '`web/site/audio/encoders` in the separate source/browser package')
    (destination / 'THIRD_PARTY_NOTICES.md').write_text(text, encoding='utf-8')
