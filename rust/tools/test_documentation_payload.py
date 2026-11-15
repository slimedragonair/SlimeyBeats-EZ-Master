"""Public source and runtime guides retain workflows, images and local links."""
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import zipfile

from release_packaging.documentation_links import verify_local_links
from release_packaging.documentation_payload import (
    copy_documentation, copy_project_notices, offline_guide,
)
from release_packaging.public_source import copy_public_source, DEMO_FILES, GUIDES
from verify_release_contents import BINARY_NAMES, verify


class DocumentationPayloadTests(unittest.TestCase):
    def test_native_notices_keep_browser_attribution_without_an_absent_local_link(self):
        repository = Path(__file__).resolve().parents[2]
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory)
            copy_project_notices(repository, destination)
            notices = (destination / 'THIRD_PARTY_NOTICES.md').read_text(encoding='utf-8')
        self.assertIn('LAME 3.100', notices)
        self.assertIn('wasm-media-encoders 0.7.0', notices)
        self.assertIn('`web/site/audio/encoders` in the separate source/browser package', notices)
        self.assertNotIn('](web/site/audio/encoders/README.md)', notices)

    def test_source_references_become_paths_and_user_workflows_keep_links(self):
        source = ('[Guide](USER_GUIDE.md#export) [Audio](../examples/audio/demo-input.wav) '
                  '[State](../rust/src/gui/user_experience.rs) [Web](../web/site/learning.js) '
                  '[MCP](../mcp/server/src/main.rs) [Workflow](../.github/workflows/web.yml) '
                  '[Host](../plugins/host/src/main.rs)')
        converted = offline_guide(source)
        self.assertIn('[Guide](USER_GUIDE.md#export)', converted)
        self.assertIn('[Audio](../Examples/demo-input.wav)', converted)
        self.assertIn('`rust/src/gui/user_experience.rs`', converted)
        self.assertIn('`web/site/learning.js`', converted)
        self.assertIn('`mcp/server/src/main.rs`', converted)
        self.assertIn('`.github/workflows/web.yml`', converted)
        self.assertIn('`plugins/host/src/main.rs`', converted)
        self.assertNotIn('](../rust/', converted)
        self.assertIn('separate source package', converted)


class PublicDocumentationPackagingTests(unittest.TestCase):
    NEW_GUIDES = (
        'STEM_EDITOR.md', 'WEB_STEM_EDITOR.md', 'STEM_DOCTOR.md', 'WEB_EXPORTS.md',
        'WEB_EXPORT_VALIDATION.md', 'BROWSER_PARITY_ROADMAP.md', 'MOBILE_PREVIEW.md',
        'HOTFIX4.md',
    )
    NEW_IMAGES = (
        'stem-editor-timeline.png', 'stem-editor-compact.png', 'stem-editor-selection.jpg',
        'web-stem-editor-phone.png', 'web-stem-editor-tools.png', 'doctor-desktop.png',
        'doctor-web-findings.png', 'doctor-web-repair.png', 'web-session-downloads.png',
    )

    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        cls.root = Path(cls.temporary.name)
        cls.repository = Path(__file__).resolve().parents[2]
        cls.source = copy_public_source(cls.repository, cls.root / 'source')
        cls.runtime = cls.root / 'runtime'
        cls.runtime.mkdir()
        copy_documentation(cls.repository, cls.runtime)
        copy_project_notices(cls.repository, cls.runtime)
        examples = cls.runtime / 'Examples'
        examples.mkdir()
        for name in DEMO_FILES:
            shutil.copy2(cls.repository / 'examples/audio' / name, examples / name)
        # Fixture binaries/notices exercise content auditing without building or
        # executing app code; actual installer qualification is a separate check.
        for name in [*BINARY_NAMES, 'Connect AI Assistant.bat', 'Connect-AI-Assistant.ps1']:
            (cls.runtime / name).write_bytes(b'packaging fixture')
        for relative in ['', 'MCP-dependencies', 'Plugin-dependencies']:
            notices = cls.runtime / relative
            notices.mkdir(exist_ok=True)
            (notices / 'THIRD-PARTY.json').write_text(
                json.dumps([{'license_files': ['LICENSE']}]), encoding='utf-8',
            )
            if relative:
                (notices / 'LICENSE').write_text('fixture notice', encoding='utf-8')
        cls.archive = cls.archive_source()

    @classmethod
    def archive_source(cls, omitted=None):
        archive = cls.root / ('source.zip' if omitted is None else 'incomplete-source.zip')
        with zipfile.ZipFile(archive, 'w') as bundle:
            for path in sorted(cls.source.rglob('*')):
                if path.is_file() and path.relative_to(cls.source).as_posix() != omitted:
                    bundle.write(path, path.relative_to(cls.root))
        return archive

    def test_current_workflow_guides_ship_in_source_and_runtime(self):
        for name in self.NEW_GUIDES:
            with self.subTest(guide=name):
                self.assertEqual((self.source / 'docs' / name).read_bytes(),
                                 (self.repository / 'docs' / name).read_bytes())
                self.assertTrue((self.runtime / 'docs' / name).is_file(), name)

    def test_reviewed_workflow_images_ship_without_modification(self):
        for name in self.NEW_IMAGES:
            for bundle in [self.source, self.runtime]:
                with self.subTest(image=name, bundle=bundle.name):
                    self.assertEqual((bundle / 'docs/images' / name).read_bytes(),
                                     (self.repository / 'docs/images' / name).read_bytes())

    def test_source_readme_and_public_guide_links_are_self_contained(self):
        guides = [self.source / 'README.md', self.source / 'THIRD_PARTY_NOTICES.md',
                  *(self.source / 'docs' / name for name in GUIDES)]
        self.assertGreater(verify_local_links(guides, self.source), 0)

    def test_runtime_public_guide_links_are_self_contained(self):
        guides = [self.runtime / 'THIRD_PARTY_NOTICES.md',
                  *(self.runtime / 'docs' / name for name in GUIDES)]
        self.assertGreater(verify_local_links(guides, self.runtime), 0)

    def test_unreviewed_docs_images_and_private_payloads_stay_excluded(self):
        checkout = self.root / 'checkout'
        shutil.copytree(self.source, checkout)
        private_paths = [
            'docs/private-report.md', 'docs/reports/session.json',
            'docs/images/private-session.png', 'personal.wav', 'user_data/settings.json',
            'output/private.wav', 'models/private.onnx', 'rust/target/release/private.exe',
            'rust/tools/__pycache__/private.pyc', 'mastercraft/__pycache__/private.py',
        ]
        for relative in private_paths:
            path = checkout / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b'private fixture; must not ship')
        exported = copy_public_source(checkout, self.root / 'filtered-source')
        for relative in private_paths:
            with self.subTest(path=relative):
                self.assertFalse((exported / relative).exists(), relative)
        self.assertEqual(
            {path.relative_to(self.source) for path in self.source.rglob('*') if path.is_file()},
            {path.relative_to(exported) for path in exported.rglob('*') if path.is_file()},
        )

    def test_release_audit_accepts_complete_public_documentation(self):
        self.assertTrue(verify(self.runtime, self.archive)['passed'])

    def test_release_audit_rejects_a_missing_source_jpeg(self):
        missing = 'docs/images/stem-editor-selection.jpg'
        archive = self.archive_source(omitted=missing)
        with self.assertRaisesRegex(AssertionError, missing):
            verify(self.runtime, archive)

    def test_release_audit_rejects_a_missing_runtime_image(self):
        image = self.runtime / 'docs/images/doctor-desktop.png'
        original = image.read_bytes()
        self.addCleanup(image.write_bytes, original)
        image.unlink()
        with self.assertRaisesRegex(AssertionError, 'docs/images/doctor-desktop.png'):
            verify(self.runtime, self.archive)


if __name__ == '__main__':
    unittest.main()
