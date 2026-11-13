"""Assemble a portable Windows release from an explicit source allowlist."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import zipfile
import tomllib
from release_packaging.public_source import copy_public_source, GUIDES, DEMO_FILES
from release_packaging.companion_payload import companion_binaries, copy_companion


def dependency_notices(project, destination):
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=project, capture_output=True, text=True,
    )
    if result.returncode:
        raise RuntimeError(f"Locked Cargo license inventory failed:\n{result.stderr.strip()}")
    packages = json.loads(result.stdout)["packages"]
    records = []
    for package in sorted(packages, key=lambda item: (item["name"], item["version"])):
        if package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent
        selected = set()
        for pattern in ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "UNLICENSE*"):
            selected.update(path for path in directory.glob(pattern) if path.is_file())
        if package["license_file"]:
            candidate = (directory / package["license_file"]).resolve()
            if candidate.is_file() and candidate.is_relative_to(directory.resolve()):
                selected.add(candidate)
        key = f"{package['name']}-{package['version']}"
        for path in sorted(selected):
            target = destination / "licenses" / key / path.name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, target)
        records.append({
            "name": package["name"], "version": package["version"],
            "license": package["license"], "repository": package["repository"],
            "license_files": [f"licenses/{key}/{path.name}" for path in sorted(selected)],
        })
    (destination / "THIRD-PARTY.json").write_text(json.dumps(records, indent=2) + "\n", encoding="utf-8")
    (destination / "THIRD-PARTY-NOTICES.txt").write_text(
        "Rust dependency attribution\n\n"
        "THIRD-PARTY.json lists Cargo dependencies and their declared licenses.\n"
        "Original license/notice files supplied by those packages are in licenses/.\n"
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
    shutil.copy2(project.parent / "LICENSE", destination / "LICENSE")
    credits = (project.parent / "THIRD_PARTY_NOTICES.md").read_text(encoding='utf-8')
    credits = credits.replace('examples/audio/', 'Examples/').replace('rust/assets/ASSETS.md','docs/ASSETS.md').replace('rust/tests/fixtures/README.md','docs/fixture-credits.md')
    (destination / 'THIRD_PARTY_NOTICES.md').write_text(credits,encoding='utf-8')
    policy = (project.parent / "OUTPUT_OWNERSHIP.md").read_text(encoding="utf-8")
    (destination / "OUTPUT_OWNERSHIP.md").write_text(policy.replace("examples/audio/CREDITS.md", "Examples/CREDITS.md"), encoding="utf-8")
    shutil.copytree(project / "docs", destination / "docs")
    for name in GUIDES:
        text = (project.parent / "docs" / name).read_text(encoding="utf-8")
        text = text.replace("../examples/audio/", "../Examples/").replace("examples/audio/demo-input.wav", "Examples/demo-input.wav")
        (destination / "docs" / name).write_text(text, encoding="utf-8")
    shutil.copytree(project.parent / "docs/images", destination / "docs/images")
    shutil.copytree(project.parent / 'docs/legacy', destination / 'docs/legacy')
    shutil.copy2(project / 'tests/fixtures/README.md', destination / 'docs/fixture-credits.md')
    example = destination / "Examples"
    example.mkdir()
    for name in DEMO_FILES:
        shutil.copy2(project.parent / "examples/audio" / name, example / name)
    shutil.copy2(project / "assets/ASSETS.md", destination / "docs/ASSETS.md")
    frame_notes = project / "assets/slime-frame-generation.md"
    if frame_notes.is_file():
        shutil.copy2(frame_notes, destination / "docs/slime-frame-generation.md")
    skin_notes = project / "assets/sovereign-skin-generation.md"
    if skin_notes.is_file():
        shutil.copy2(skin_notes, destination / "docs/sovereign-skin-generation.md")
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
        "Studio: local vocal/instrumental or four-stem separation, imported stems,\n"
        "rebalancing, voice cleanup, spectral repair and trusted VST3 effects.\n"
        "Neural separation needs the optional isolated engine download; no audio\n"
        "is uploaded. Models/runtime are not bundled. Read docs/STUDIO.md.\n\n"
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
        "editing works there; neural extraction and native VST3 remain desktop tools.\n\n"
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
