"""Fast policy and real Git event-diff regressions; no application builds."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import check_scope as scope


class ClassificationTests(unittest.TestCase):
    def assert_scopes(self, files, expected):
        result = scope.classify_files(files)
        self.assertEqual(set(result), set(scope.SCOPES))
        self.assertTrue(all(value in {"true", "false"} for value in result.values()))
        self.assertEqual({name for name, value in result.items() if value == "true"}, set(expected))

    def test_current_mcp_docs_packaging_and_policy_push_is_narrow(self):
        self.assert_scopes([
            "mcp/server/src/protocol.rs", "mcp/tools/verify_compact_catalog.py", "mcp/server/Cargo.lock",
            "README.md", "docs/MCP.md", "mcp/ARRANGEMENT_PROJECTS.md",
            "rust/tools/release_packaging/public_source.py", "rust/tools/public_source.py",
            "rust/tools/test_documentation_payload.py", "rust/tools/test_documentation_links.py",
            ".github/workflows/ci.yml", ".github/workflows/web.yml", ".github/ci/check_scope.py",
        ], {"mcp", "docs", "ci"})

    def test_documentation_never_selects_an_application(self):
        self.assert_scopes(["docs/STUDIO.md", "rust/src/support_report/README.md", "web/README.md"], {"docs"})

    def test_ci_policy_changes_select_only_ci(self):
        self.assert_scopes([".github/workflows/change-scope.yml", ".github/ci/test_check_scope.py"], {"ci"})

    def test_guarded_publication_recovery_changes_do_not_repeat_unrelated_app_suites(self):
        self.assert_scopes(["web/tools/region_edit_ui_checks.py", "web/tools/test_region_edit_download.py",
                            ".github/ci/browser_resume.py", ".github/workflows/browser-publication-resume.yml",
                            "web/tools/pages_artifacts.py", "web/tools/pages_baseline.py",
                            "web/tools/compose_pages_preview.py", "web/tools/test_pages_preview.py"], {"ci"})
        self.assert_scopes(["web/site/studio-exports.js"], {"web", "arrangement"})

    def test_shared_rust_and_manifests_select_all_consumers(self):
        for path in ["rust/src/audio.rs", "rust/src/studio/arrangement/playback.rs", "rust/src/hybrid_worker/mod.rs", "rust/Cargo.toml", "rust/Cargo.lock"]:
            with self.subTest(path=path): self.assert_scopes([path], scope.SHARED_RUST)

    def test_native_desktop_and_tests_stay_native(self):
        for path in ["rust/src/main.rs", "rust/src/gui/studio/panel.rs", "rust/src/desktop_integration.rs", "rust/src/playback.rs", "rust/src/dsp/true_peak_tests.rs", "rust/src/studio/arrangement/tests/render.rs", "rust/tests/export.rs", "rust/tools/verify_arrangement_native.py"]:
            with self.subTest(path=path): self.assert_scopes([path], {"native"})

    def test_web_hybrid_plugins_and_assets_have_explicit_consumers(self):
        for path, expected in [
            ("web/site/studio-app.js", {"web", "arrangement"}),
            ("web/tools/verify_browser.py", {"web", "arrangement"}),
            ("web/dsp/Cargo.lock", {"web", "arrangement"}),
            ("hybrid/server/project_http.py", {"hybrid", "web", "arrangement"}),
            ("plugins/host/src/main.rs", {"plugins", "mcp"}),
            ("rust/assets/learning-guide.json", {"native", "web"}),
            ("examples/audio/demo-input.wav", {"docs", "native", "web"}),
            ("examples/audio/CREDITS.md", {"docs", "native", "web"}),
            ("LICENSE", {"docs", "native", "web"}),
        ]:
            with self.subTest(path=path): self.assert_scopes([path], expected)

    def test_empty_changes_select_nothing_and_unknown_source_selects_everything(self):
        self.assert_scopes([], set())
        for path in ["future-engine/process.py", "../mcp/server/src/lib.rs", "/rust/src/audio.rs"]:
            with self.subTest(path=path): self.assert_scopes([path], scope.SCOPES)

    def test_windows_fixture_paths_and_spaces_remain_classifiable(self):
        self.assert_scopes([r"mcp\server\src\lib.rs", "docs/Agent workflow.md"], {"mcp", "docs"})

    def test_cli_writes_all_eight_lowercase_workflow_outputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "outputs"
            environment = dict(os.environ, GITHUB_OUTPUT=str(output))
            result = subprocess.run([sys.executable, str(Path(scope.__file__)), "--files", "mcp/server/src/lib.rs", "README.md", ".github/workflows/ci.yml"],
                                    env=environment, check=True, capture_output=True, text=True)
            expected = scope.outputs({"mcp", "docs", "ci"})
            self.assertEqual(json.loads(result.stdout), expected)
            self.assertEqual(dict(line.split("=", 1) for line in output.read_text().splitlines()), expected)


class EventTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.repo = Path(cls.temporary.name)
        cls.hooks = cls.repo / "empty-hooks"; cls.hooks.mkdir()
        cls.run_git("init", "--initial-branch=main")
        cls.base = cls.add_commit("README.md", "Initial scope fixture")
        cls.run_git("checkout", "-b", "feature")
        cls.feature = cls.add_commit("mcp/server/src/protocol.rs", "MCP-only change")
        cls.run_git("checkout", "main")
        cls.main = cls.add_commit("rust/src/gui/app.rs", "Unrelated base branch GUI change")
        cls.run_git("checkout", "feature")
        (cls.repo / "docs").mkdir()
        cls.run_git("mv", "mcp/server/src/protocol.rs", "docs/protocol.md")
        cls.run_git("commit", "-m", "Move MCP code into docs")
        cls.renamed = cls.run_git("rev-parse", "HEAD").strip()

    @classmethod
    def tearDownClass(cls): cls.temporary.cleanup()

    @classmethod
    def run_git(cls, *arguments):
        return subprocess.run(["git", "-C", str(cls.repo), "-c", "user.name=Scope test", "-c", "user.email=scope@example.test",
                               "-c", "commit.gpgsign=false", "-c", f"core.hooksPath={cls.hooks}", *arguments],
                              check=True, capture_output=True, text=True).stdout

    @classmethod
    def add_commit(cls, path, message):
        target = cls.repo / path; target.parent.mkdir(parents=True, exist_ok=True); target.write_text(message)
        cls.run_git("add", "--", path); cls.run_git("commit", "-m", message)
        return cls.run_git("rev-parse", "HEAD").strip()

    def test_push_uses_exact_before_and_after(self):
        self.assertEqual(scope.event_scope("push", {"before": self.base, "after": self.feature}, self.repo), scope.outputs({"mcp"}))

    def test_pr_three_dot_excludes_unrelated_base_branch_changes(self):
        event = {"pull_request": {"base": {"sha": self.main}, "head": {"sha": self.feature}}}
        self.assertEqual(scope.event_scope("pull_request", event, self.repo), scope.outputs({"mcp"}))

    def test_renames_include_old_and_new_owners(self):
        self.assertEqual(scope.event_scope("push", {"before": self.feature, "after": self.renamed}, self.repo), scope.outputs({"mcp", "docs"}))

    def test_workflow_run_uses_only_parent_to_parent_head(self):
        self.assertEqual(scope.event_scope("workflow_run", {"workflow_run": {"head_sha": self.renamed}}, self.repo), scope.outputs({"mcp", "docs"}))

    def test_valid_empty_diff_skips_app_jobs(self):
        self.assertEqual(scope.event_scope("push", {"before": self.feature, "after": self.feature}, self.repo), scope.outputs(set()))

    def test_dispatch_and_missing_invalid_or_initial_bases_are_conservative(self):
        for name, event in [
            ("workflow_dispatch", {}), ("push", {}), ("push", {"before": "0" * 40, "after": self.feature}),
            ("push", {"before": "f" * 40, "after": self.feature}), ("pull_request", {"pull_request": {}}),
            ("workflow_run", {"workflow_run": {"head_sha": self.base}}), ("unknown", {}), ("push", []),
        ]:
            with self.subTest(name=name, event=event): self.assertEqual(scope.event_scope(name, event, self.repo), scope.outputs(scope.SCOPES))

    def test_cli_invalid_event_file_selects_all(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"; event.write_text("not JSON")
            environment = dict(os.environ, GITHUB_EVENT_NAME="push", GITHUB_EVENT_PATH=str(event))
            environment.pop("GITHUB_OUTPUT", None)
            result = subprocess.run([sys.executable, str(Path(scope.__file__))], env=environment, check=True, capture_output=True, text=True)
            self.assertEqual(json.loads(result.stdout), scope.outputs(scope.SCOPES))


if __name__ == "__main__": unittest.main()
