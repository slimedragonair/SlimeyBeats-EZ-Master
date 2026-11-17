"""Select affected CI jobs from changed paths; never build the application here.

Rules are ordered: CI policy, shipped shared resources, documentation, then
source ownership. Unknown paths and unusable event bases select every scope.
PRs compare merge-base...head; pushes and completed preview runs compare the
specified before..after or parent..head. --files exercises the same classifier.
"""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys

SCOPES = ("native", "mcp", "web", "arrangement", "hybrid", "plugins", "docs", "ci")
SHARED_RUST = {"native", "mcp", "web", "arrangement", "hybrid"}
DESKTOP_FILES = {"rust/src/main.rs", "rust/src/desktop_integration.rs", "rust/src/playback.rs"}
PACKAGING_FILES = {"rust/tools/public_source.py"}
# These change validation/publication policy, not shipped app bytes. Their
# guarded workflow and offline policy tests run without replaying the app suite.
VALIDATION_ONLY_FILES = {
    "web/tools/region_edit_ui_checks.py", "web/tools/test_region_edit_download.py",
    "web/tools/pages_artifacts.py", "web/tools/pages_baseline.py",
    "web/tools/compose_pages_preview.py", "web/tools/test_pages_preview.py",
    "web/tools/test_pages_baseline.py", "web/tools/test_pages_artifacts.py",
}
SHARED_FILES = {"OUTPUT_OWNERSHIP.md", "THIRD-PARTY-NOTICES.txt", "examples/audio/CREDITS.md"}


def outputs(selected):
    return {name: str(name in selected).lower() for name in SCOPES}


def classify_files(files):
    selected = set()
    for raw in files:
        path = raw.replace("\\", "/")
        while path.startswith("./"):
            path = path[2:]
        parts = PurePosixPath(path).parts
        if not path or not parts or path.startswith("/") or ".." in parts or ":" in path:
            return outputs(SCOPES)
        name = parts[-1]
        if path.startswith((".github/workflows/", ".github/ci/")) or path in VALIDATION_ONLY_FILES:
            selected.add("ci")
        elif path in SHARED_FILES or (len(parts) == 1 and name.startswith("LICENSE")):
            selected.update({"docs", "native", "web"})
        elif (path.startswith(("docs/", "rust/tools/release_packaging/"))
              or path in PACKAGING_FILES or re.fullmatch(r"rust/tools/test_documentation[^/]*\.py", path)
              or path.lower().endswith(".md")
              or name.lower().startswith("readme")):
            selected.add("docs")
        elif path.startswith("mcp/"):
            selected.add("mcp")
        elif path.startswith("web/"):
            selected.update({"web", "arrangement"})
        elif path.startswith("hybrid/"):
            selected.update({"hybrid", "web", "arrangement"})
        elif path.startswith("plugins/host/"):
            selected.update({"plugins", "mcp"})
        elif path.startswith("rust/assets/"):
            selected.update({"native", "web"})
        elif path.startswith("examples/audio/"):
            selected.update({"docs", "native", "web"})
        elif path in {"Cargo.toml", "Cargo.lock", "rust/Cargo.toml", "rust/Cargo.lock"}:
            selected.update(SHARED_RUST)
        elif (path in DESKTOP_FILES or path.startswith(("rust/src/gui/", "rust/src/playback/", "rust/tests/", "rust/tools/"))
              or path.startswith("rust/src/") and (name.endswith("_tests.rs") or name == "tests.rs" or "tests" in parts)):
            selected.add("native")
        elif path.startswith("rust/src/"):
            # Arrangement playback is shared; do not classify every 'playback' filename as desktop-only.
            selected.update(SHARED_RUST)
        else:
            return outputs(SCOPES)
    return outputs(selected)


def git(repo, *arguments):
    return subprocess.run(["git", "-C", str(repo), *arguments], check=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE).stdout


def commit(repo, value):
    if not isinstance(value, str) or not re.fullmatch(r"(?:[a-fA-F0-9]{40}|[a-fA-F0-9]{64})", value) or not value.strip("0"):
        raise ValueError("Missing or invalid commit ID")
    git(repo, "cat-file", "-e", value + "^{commit}")
    return value


def event_scope(event_name, event, repo):
    if event_name == "workflow_dispatch":
        return outputs(SCOPES)
    try:
        if event_name == "push":
            base, head = commit(repo, event["before"]), commit(repo, event["after"])
            comparison = base + ".." + head
        elif event_name == "pull_request":
            request = event["pull_request"]
            base, head = commit(repo, request["base"]["sha"]), commit(repo, request["head"]["sha"])
            comparison = base + "..." + head
        elif event_name == "workflow_run":
            head = commit(repo, event["workflow_run"]["head_sha"])
            base = commit(repo, git(repo, "rev-parse", "--verify", head + "^").decode().strip())
            comparison = base + ".." + head
        else:
            return outputs(SCOPES)
        changed = git(repo, "diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", comparison, "--")
        return classify_files([os.fsdecode(path) for path in changed.split(b"\0") if path])
    except (KeyError, TypeError, ValueError, OSError, subprocess.CalledProcessError):
        return outputs(SCOPES)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--files", nargs="*", help="Classify explicit repository-relative paths without an event or Git diff")
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    args = parser.parse_args(argv)
    if args.files is not None:
        selected = classify_files(args.files)
    else:
        name = os.environ.get("GITHUB_EVENT_NAME", "")
        try:
            event = json.loads(Path(os.environ["GITHUB_EVENT_PATH"]).read_text(encoding="utf-8")) if name != "workflow_dispatch" else {}
            selected = event_scope(name, event, args.repo)
        except (KeyError, ValueError, OSError):
            selected = outputs(SCOPES)
    if target := os.environ.get("GITHUB_OUTPUT"):
        with Path(target).open("a", encoding="utf-8", newline="\n") as destination:
            destination.write("".join(f"{name}={selected[name]}\n" for name in SCOPES))
    print(json.dumps(selected, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
