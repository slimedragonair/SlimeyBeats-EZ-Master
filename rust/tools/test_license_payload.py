"""Regression coverage for original notice paths and offline upstream pins."""
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from release_packaging.license_payload import copy_dependency_licenses, sha256, upstream_url


class LicensePayloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.crate = self.root / "crate"
        self.crate.mkdir()
        self.output = self.root / "output"
        self.overrides = self.root / "overrides"
        self.package = {"name": "example", "version": "1.2.3", "source": "registry+example",
                        "manifest_path": str(self.crate / "Cargo.toml"),
                        "license": "MIT OR Apache-2.0", "license_file": None,
                        "repository": "https://github.com/example/project", "checksum": "a" * 64}

    def write(self, relative, data=b"original notice"):
        path = self.crate / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return path

    def collect(self):
        return copy_dependency_licenses([self.package], self.output, self.overrides)[0]

    def override(self, data=b"original upstream notice"):
        directory = self.overrides / "example-1.2.3"
        directory.mkdir(parents=True)
        (directory / "LICENSE-MIT").write_bytes(data)
        entry = {"path": "LICENSE-MIT", "upstream_path": "LICENSE-MIT",
                 "url": "https://raw.githubusercontent.com/example/project/" + "b" * 40 + "/LICENSE-MIT",
                 "sha256": sha256(data)}
        manifest = {**{key: self.package[key] for key in ("name", "version", "source", "license", "repository")},
                    "format_version": 1, "crate_checksum": self.package["checksum"], "crate_vcs": None,
                    "upstream_repository": self.package["repository"], "upstream_commit": "b" * 40,
                    "files": [entry]}
        (directory / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        return directory, manifest

    def test_nested_notices_keep_paths_and_equal_basenames(self):
        self.write("LICENSE", b"root")
        self.write("vendor/one/LICENSE", b"one")
        self.write("vendor/two/LICENSE", b"two")
        self.write("src/copying.rs", b"not a license")
        self.write("vendor/OFL.txt", b"font license")
        record = self.collect()
        self.assertEqual(len(record["license_files"]), 4)
        for name in ("LICENSE", "vendor/one/LICENSE", "vendor/two/LICENSE", "vendor/OFL.txt"):
            self.assertEqual((self.output / "licenses/example-1.2.3" / name).read_bytes(),
                             (self.crate / name).read_bytes())
        self.assertEqual(record["license"], self.package["license"])

    def test_declared_license_file_keeps_its_nonstandard_path(self):
        self.write("legal/terms.txt")
        self.package["license_file"] = "legal/terms.txt"
        self.assertEqual(self.collect()["license_files"], ["licenses/example-1.2.3/legal/terms.txt"])

    def test_default_fonts_copy_original_descriptive_notices(self):
        self.package["name"] = "epaint_default_fonts"
        for name in ("Hack-Regular.txt", "UFL.txt", "OFL.txt", "emoji-icon-font-mit-license.txt", "egui-icons.txt"):
            self.write("fonts/" + name)
        self.write("fonts/Hack-Regular.ttf", b"font bytes")
        self.assertEqual(len(self.collect()["license_files"]), 5)

    def test_override_copies_original_bytes_offline_with_provenance(self):
        self.override()
        with patch("urllib.request.urlopen", side_effect=AssertionError("network forbidden")):
            record = self.collect()
        provenance = json.loads((self.output / record["notice_provenance"]).read_text())
        self.assertEqual(provenance["files"][0]["sha256"], sha256(b"original upstream notice"))
        self.assertEqual(provenance["files"][0]["sources"][0]["kind"], "pinned_upstream")
        first = (self.output / record["notice_provenance"]).read_bytes()
        self.collect()
        self.assertEqual(first, (self.output / record["notice_provenance"]).read_bytes())

    def test_tampered_override_fails(self):
        directory, _ = self.override()
        (directory / "LICENSE-MIT").write_bytes(b"changed")
        with self.assertRaisesRegex(ValueError, "hash mismatch"):
            self.collect()

    def test_mismatched_crate_checksum_fails(self):
        self.override()
        self.package["checksum"] = "c" * 64
        with self.assertRaisesRegex(ValueError, "provenance mismatch"):
            self.collect()

    def test_changed_declaration_is_never_replaced_by_override(self):
        self.override()
        self.package["license"] = "Apache-2.0"
        with self.assertRaisesRegex(ValueError, "license mismatch"):
            self.collect()

    def test_override_cannot_escape_its_directory(self):
        directory, manifest = self.override()
        manifest["files"][0]["path"] = "../LICENSE-MIT"
        (directory / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "Unsafe notice path"):
            self.collect()

    def test_registry_and_upstream_path_collision_is_not_overwritten(self):
        self.write("LICENSE-MIT", b"different crate notice")
        self.override()
        with self.assertRaisesRegex(ValueError, "Conflicting original notice"):
            self.collect()

    def test_case_only_collision_is_rejected_for_windows_payloads(self):
        self.write("LICENSE")
        self.write("license")
        with patch("release_packaging.license_payload.original_notices", return_value=["LICENSE", "license"]):
            with self.assertRaisesRegex(ValueError, "Case-insensitive notice collision"):
                self.collect()

    def test_matching_registry_and_override_files_keep_both_sources(self):
        self.write("LICENSE-MIT", b"original upstream notice")
        self.override()
        record = self.collect()
        provenance = json.loads((self.output / record["notice_provenance"]).read_text())
        self.assertEqual(len(record["license_files"]), 1)
        self.assertEqual(len(provenance["files"][0]["sources"]), 2)

    def test_missing_files_remain_visible_without_failing_platform_inventory(self):
        record = self.collect()
        self.assertEqual(record["license_files"], [])
        self.assertEqual(record["notice_status"], "no_separate_notice_files")
        self.assertEqual(record["license"], "MIT OR Apache-2.0")

    def test_fetch_source_rejects_unpinned_or_arbitrary_urls(self):
        _, manifest = self.override()
        entry = manifest["files"][0]
        self.assertEqual(upstream_url(manifest, entry), entry["url"])
        for url in ("http://127.0.0.1/LICENSE", "https://example.com/LICENSE"):
            with self.assertRaises(ValueError):
                upstream_url(manifest, {**entry, "url": url})
        with self.assertRaises(ValueError):
            upstream_url({**manifest, "upstream_commit": "main"}, entry)

    def test_dependency_notices_allows_uncached_targets_with_locked_metadata(self):
        from package_release import dependency_notices

        self.write("LICENSE", b"original")
        (self.root / "Cargo.lock").write_text(
            'version = 4\n[[package]]\nname = "example"\nversion = "1.2.3"\n'
            'source = "registry+example"\nchecksum = "' + "a" * 64 + '"\n', encoding="utf-8")
        cargo_package = {key: value for key, value in self.package.items() if key != "checksum"}
        metadata = {"packages": [cargo_package], "workspace_root": str(self.root)}
        result = SimpleNamespace(returncode=0, stdout=json.dumps(metadata), stderr="")
        with patch("package_release.subprocess.run", return_value=result) as run:
            dependency_notices(self.root, self.output)
        self.assertNotIn("--offline", run.call_args.args[0])
        self.assertIn("--locked", run.call_args.args[0])
        record = json.loads((self.output / "THIRD-PARTY.json").read_text())[0]
        provenance = json.loads((self.output / record["notice_provenance"]).read_text())
        self.assertEqual(provenance["crate_checksum"], "a" * 64)
        self.assertEqual(record["license"], "MIT OR Apache-2.0")

    def test_cargo_errors_keep_the_diagnostic(self):
        from package_release import dependency_notices

        result = SimpleNamespace(returncode=1, stdout="", stderr="missing cached crate")
        with patch("package_release.subprocess.run", return_value=result):
            with self.assertRaisesRegex(RuntimeError, "missing cached crate"):
                dependency_notices(self.root, self.output)


if __name__ == "__main__":
    unittest.main()
