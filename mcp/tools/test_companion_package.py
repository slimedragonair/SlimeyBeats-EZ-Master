"""Fixture-only companion packaging checks; no Cargo fetch, release build or upload."""
import contextlib
import io
import json
from pathlib import Path
import tempfile
import tomllib
import unittest
from unittest.mock import patch
import zipfile

import package_companion


class CompanionPackageTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="MCP package contract ")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.repository = self.root / "source"
        self.output = self.root / "packages"
        self.version = "0.2.0-beta.3"
        self.manifest = self.repository / "mcp/server/Cargo.toml"
        self.manifest.parent.mkdir(parents=True)
        self.manifest.write_text(f'[package]\nversion = "{self.version}"\n', encoding="utf-8")
        self.server = self.root / "server.exe"
        self.desktop = self.root / "desktop.exe"
        self.server.write_bytes(b"server payload fixture")
        self.desktop.write_bytes(b"desktop payload fixture")
        self.host = self.repository / "plugins/host/target/release/slimeybeats-vst3-host.exe"
        self.host.parent.mkdir(parents=True)
        self.host.write_bytes(b"host payload fixture")
        script = self.repository / "mcp/tools/Connect-AI-Assistant.ps1"
        script.parent.mkdir(parents=True)
        script.write_text("# connection helper fixture", encoding="utf-8")
        self.examples = self.repository / "examples/audio"
        self.examples.mkdir(parents=True)
        (self.examples / "Demo.wav").write_bytes(b"example fixture")
        self.addCleanup(patch.stopall)
        patch.object(package_companion, "REPOSITORY", self.repository).start()
        patch.object(package_companion, "DEMO_FILES", ["Demo.wav"]).start()
        patch.object(package_companion, "dependency_notices", lambda project, target: (target / "original-notice.txt").write_bytes(b"notice fixture")).start()
        patch.object(package_companion, "copy_project_notices", lambda project, target: (target / "LICENSE").write_bytes(b"license fixture")).start()
        patch.object(package_companion, "copy_documentation", self.copy_docs).start()

    @staticmethod
    def copy_docs(project, target):
        docs = target / "docs"
        docs.mkdir()
        (docs / "MCP.md").write_text("MCP guide fixture", encoding="utf-8")

    def package(self, label=None):
        printed = io.StringIO()
        with contextlib.redirect_stdout(printed):
            package_companion.package(self.output, self.server, self.desktop, label=label)
        return json.loads(printed.getvalue())

    def test_hotfix_label_preserves_existing_asset_and_manifest_version_and_payloads(self):
        old_name = f"SlimeyBeats-MCP-{self.version}-preview-Windows-x64"
        old_folder = self.output / old_name
        old_folder.mkdir(parents=True)
        (old_folder / "README.txt").write_bytes(b"published original readme")
        old_archive = self.output / (old_name + ".zip")
        old_archive.write_bytes(b"published original archive")
        result = self.package("Hotfix2")
        folder = Path(result["folder"])
        archive = Path(result["zip"])
        self.assertEqual(folder.name, f"SlimeyBeats-MCP-{self.version}-Hotfix2-preview-Windows-x64")
        self.assertEqual(archive.name, folder.name + ".zip")
        self.assertEqual(old_archive.read_bytes(), b"published original archive")
        self.assertEqual((old_folder / "README.txt").read_bytes(), b"published original readme")
        self.assertEqual(tomllib.loads(self.manifest.read_text())["package"]["version"], self.version)
        with zipfile.ZipFile(archive) as bundle:
            for filename, source in [("slimeybeats-mcp.exe", self.server), ("SlimeyBeats-EZ-Master.exe", self.desktop), (self.host.name, self.host)]:
                self.assertEqual(bundle.read(folder.name + "/" + filename), source.read_bytes())
            self.assertEqual(bundle.read(folder.name + "/MCP-dependencies/original-notice.txt"), b"notice fixture")

    def test_unlabeled_calls_keep_original_filename_and_existing_output_is_not_replaced(self):
        result = self.package()
        archive = Path(result["zip"])
        self.assertEqual(archive.name, f"SlimeyBeats-MCP-{self.version}-preview-Windows-x64.zip")
        original = archive.read_bytes()
        with self.assertRaises(FileExistsError):
            self.package()
        self.assertEqual(archive.read_bytes(), original)

    def test_label_validation_uses_shared_safe_suffix_before_creating_output(self):
        for label in ["../old", "bad/name", "x..y", "", "x" * 49]:
            with self.subTest(label=label), self.assertRaises(ValueError):
                self.package(label)
        self.assertFalse(self.output.exists())

    def test_readme_describes_current_tool_count_consent_and_private_cache_setup(self):
        result = self.package("Hotfix2")
        readme = (Path(result["folder"]) / "README.txt").read_text(encoding="utf-8")
        for text in ["33 organized tools", "58 tools", "--tool-profile legacy", "optional_tool_status", "setup_optional_tool", "allow_download=true", "automatically prepare", "private user cache", "private recovery cache", "No AI model is bundled", "Package label: Hotfix2"]:
            self.assertIn(text, readme)
        for text in ["support_report operation.action=preview", "operation.action=save", "owner_review_confirmed=true", "model provider", "before review or saving", "cannot retract client input", "This companion does not upload files or post issues"]:
            self.assertIn(text, readme)
        self.assertNotIn("require separately installed ffmpeg", readme)

    def test_shared_eq_integration_guide_ships_through_real_documentation_projection(self):
        from release_packaging.documentation_payload import copy_documentation, offline_guide
        repository = Path(__file__).resolve().parents[2]
        guide = "docs/EQ_SUGGESTION_INTEGRATION.md"
        expected = offline_guide((repository / guide).read_text(encoding="utf-8"))
        with patch.object(package_companion, "copy_documentation",
                          lambda unused, target: copy_documentation(repository, target)):
            result = self.package("EQGuide")
        folder = Path(result["folder"])
        self.assertEqual((folder / guide).read_text(encoding="utf-8"), expected)
        with zipfile.ZipFile(result["zip"]) as bundle:
            self.assertEqual(bundle.read(folder.name + "/" + guide), (folder / guide).read_bytes())

    def test_arrangement_integration_guide_ships_in_the_actual_companion_archive(self):
        from release_packaging.documentation_payload import copy_documentation, offline_guide
        repository = Path(__file__).resolve().parents[2]
        guide = "docs/ARRANGEMENT_TRANSPORT.md"
        expected = offline_guide((repository / guide).read_text(encoding="utf-8"))
        with patch.object(package_companion, "copy_documentation", lambda unused, target: copy_documentation(repository, target)):
            result = self.package("ArrangementGuide")
        folder = Path(result["folder"])
        self.assertEqual((folder / guide).read_text(encoding="utf-8"), expected)
        with zipfile.ZipFile(result["zip"]) as bundle:
            self.assertEqual(bundle.read(folder.name + "/" + guide), (folder / guide).read_bytes())


if __name__ == "__main__":
    unittest.main()
