"""Assemble a portable Windows release from an explicit source allowlist."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import zipfile
import tomllib
from release_packaging.public_source import copy_public_source, DEMO_FILES
from release_packaging.companion_payload import companion_binaries, copy_companion
from release_packaging.documentation_payload import copy_documentation, copy_project_notices
from release_packaging.license_payload import copy_dependency_licenses


def dependency_notices(project, destination):
    # A target build may leave other platforms uncached; honor Cargo's offline opt-in.
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=project, capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"Locked Cargo license inventory failed:\n{result.stderr.strip()}")
    metadata = json.loads(result.stdout)
    packages = metadata["packages"]
    lock = tomllib.loads((Path(metadata["workspace_root"]) / "Cargo.lock").read_text(encoding="utf-8"))
    checksums = {(item["name"], item["version"], item.get("source")): item.get("checksum")
                 for item in lock["package"]}
    for item in packages:
        item["checksum"] = checksums.get((item["name"], item["version"], item["source"]))
    records = copy_dependency_licenses(packages, destination)
    (destination / "THIRD-PARTY.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    (destination / "THIRD-PARTY-NOTICES.txt").write_text(
        "Rust dependency attribution\n\n"
        "THIRD-PARTY.json lists Cargo dependencies and their declared licenses.\n"
        "Original license/notice files supplied by those packages are in licenses/.\n"
        "Nested paths are retained; reviewed upstream supplements are pinned locally.\n"
        "Each package's LICENSE-PROVENANCE.json records sources and SHA-256 hashes.\n"
        "Packages without separate files are marked explicitly in THIRD-PARTY.json.\n"
        "The inventory also includes platform dependencies not used on Windows.\n"
        "FFmpeg is an optional external installation and is not bundled.\n"
        "The project's original source/docs/artwork are MIT licensed; see LICENSE.\n"
        "Separately credited assets keep their licenses. Music outputs require no app credit.\n",
        encoding="utf-8",
    )


def package(project, parent, version, evidence):
    destination = parent / f"SlimeyBeats-EZ-Master-{version}"
    archive = parent / f"{destination.name}.zip"
    # A fresh directory protects earlier releases and any user-created settings/exports.
    if destination.exists() or archive.exists():
        raise FileExistsError(f"Release destination already exists: {destination}")
    for source in companion_binaries(project.parent):
        if not source.is_file():
            raise FileNotFoundError(source)
    destination.mkdir(parents=True)
    binaries = {
        "slimeybeats-ez-master-live.exe": "SlimeyBeats-EZ-Master.exe",
        "slimeybeats-ez-master-cli.exe": "SlimeyBeats-EZ-Master-CLI.exe",
    }
    for source, target in binaries.items():
        shutil.copy2(project / "target/release" / source, destination / target)
    copy_public_source(project.parent, destination / "source")
    copy_project_notices(project.parent, destination)
    copy_documentation(project.parent, destination)
    example = destination / "Examples"
    example.mkdir()
    for name in DEMO_FILES:
        shutil.copy2(project.parent / "examples/audio" / name, example / name)
    if evidence:
        shutil.copytree(evidence, destination / "verification")
    dependency_notices(project, destination)
    copy_companion(project.parent, destination, dependency_notices)
    (destination / "Start SlimeyBeats.bat").write_text(
        '@echo off\r\ncd /d "%~dp0"\r\nstart "" "%~dp0SlimeyBeats-EZ-Master.exe" %*\r\n',
        encoding="ascii", newline="",
    )
    (destination / "README.txt").write_text(
        f"SlimeyBeats-EZ-Master {version} - Windows x64\n\n"
        "Extract the complete folder to a writable location. Open Start SlimeyBeats.bat\n"
        "or SlimeyBeats-EZ-Master.exe. Settings/exports are saved beside the app.\n\n"
        "Add audio -> Analyze (top right) -> Master -> compare/tweak -> Export.\n"
        "Mastering prepares an in-memory preview. Export is a separate action.\n"
        "Manual EQ updates while playing; its live spectrum shares the EQ graph.\n"
        "Hover names and controls for help. Suggestions are editable starting points.\n\n"
        "Basic starts with everyday controls. Advanced reveals detail without\n"
        "resetting your settings. The optional first-use tour remembers a completed\n"
        "or declined offer. Bottom-right Help (F1) has replay, terms, troubleshooting\n"
        "and desktop AI connection guidance. Read docs/LEARNING_AND_HELP.md.\n\n"
        "Studio: import aligned stem files or a Suno/stem ZIP without a fixed\n"
        "part-count ceiling. All parts start selected; review before importing.\n"
        "Choose the preferred WAV/MP3 encoding. Original names survive edits/projects.\n"
        "Capacity depends on memory and retained input/ZIP metadata safety budgets.\n"
        "Decoded parts must match rate, channels and frames; no timeline, beat\n"
        "matching or new resampling controls are added. Read docs/STUDIO.md.\n"
        "Optional local separation offers vocal/instrumental or four stems.\n"
        "Rebalance, clean a voice, repair a region or use trusted VST3 effects.\n"
        "Neural separation needs the optional isolated engine download; no audio\n"
        "is uploaded. Models/runtime are not bundled. Read docs/STUDIO.md.\n"
        "Candidate qualification: docs/STUDIO_BETA3_VALIDATION.md.\n\n"
        "AI control: run Connect AI Assistant.bat to prepare a local MCP connection.\n"
        "The companion and VST3 worker are included. Native plugin access is disabled\n"
        "for AI until an owner adds trusted --plugin-root folders. See docs/MCP.md.\n\n"
        "Example audio: open Examples/Listen.html or add Examples/demo-input.wav.\n"
        "Offline illustrated guide: docs/GETTING_STARTED.html.\n\n"
        "MIT software: use/change/redistribute freely while retaining its notice.\n"
        "Your music: no app ownership, royalties, watermark or required app credit.\n"
        "See OUTPUT_OWNERSHIP.md and the example/dependency license notices.\n\n"
        "Expand Export settings in the right pane to choose formats and a folder.\n"
        "The smaller layout scrolls editing content while playback stays pinned.\n\n"
        "WAV input/export works without Python or FFmpeg. Compressed inputs,\n"
        "FLAC/MP3 exports and sample-rate conversion need ffmpeg and ffprobe on PATH.\n"
        "No FFmpeg installer or binaries are included.\n\n"
        "Batch CLI: SlimeyBeats-EZ-Master-CLI.exe --help\n"
        "Build source: cd source/rust, then cargo build --release --bins\n"
        "Tests: cargo test --release --all-targets\n"
        "Export validation: python tools/verify_exports.py --input YOUR-STEREO-WAV\n\n"
        "See docs/REVIEW.md for benchmarks, tests and verification limits, and\n"
        "source/rust/README.md for detailed operation and compatibility.\n"
        "This revised DSP is not sample-identical to the original Python tool.\n"
        "Listening across more songs/devices remains a separate acceptance step.\n"
        "The private browser companion is documented in docs/WEB.md. Imported-stem\n"
        "editing keeps its 128 MiB combined decoded-input budget and 15-minute\n"
        "per-track guard; neural extraction and native VST3 remain desktop tools.\n\n"
        "Source, dependencies, artwork provenance, research and review evidence are\n"
        "included. Only the documented CC0 demo is bundled, not personal audio/settings.\n",
        encoding="utf-8",
    )
    # Cargo packages can retain Unix-epoch license timestamps; ZIP starts at 1980.
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6,
                         strict_timestamps=False) as bundle:
        for path in sorted(destination.rglob("*")):
            if path.is_file():
                bundle.write(path, path.relative_to(parent))
    print(json.dumps({"folder": str(destination), "zip": str(archive),
                      "zip_bytes": archive.stat().st_size}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    default_version = tomllib.loads((Path(__file__).resolve().parents[1] / "Cargo.toml").read_text())["package"]["version"]
    parser.add_argument("--version", default=default_version)
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    package(Path(__file__).resolve().parents[1], args.out, args.version, args.evidence)
