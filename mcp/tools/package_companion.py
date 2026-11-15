"""Package a Windows MCP Studio preview without replacing existing release assets."""
import argparse
import json
from pathlib import Path
import shutil
import sys
import tomllib
import zipfile

REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "rust/tools"))
from package_release import dependency_notices  # noqa: E402
from release_packaging.public_source import DEMO_FILES  # noqa: E402
from release_packaging.documentation_payload import copy_documentation, copy_project_notices
from release_packaging.artifact_labels import suffix


def package(destination, server, desktop, label=None):
    version = tomllib.loads((REPOSITORY / "mcp/server/Cargo.toml").read_text())["package"]["version"]
    root = destination / f"SlimeyBeats-MCP-{version}{suffix(label)}-preview-Windows-x64"
    archive = destination / f"{root.name}.zip"
    if root.exists() or archive.exists():
        raise FileExistsError(root)
    for binary in (server, desktop):
        if not binary.is_file():
            raise FileNotFoundError(binary)
    root.mkdir(parents=True)
    shutil.copy2(server, root / "slimeybeats-mcp.exe")
    shutil.copy2(desktop, root / "SlimeyBeats-EZ-Master.exe")
    host = REPOSITORY / 'plugins/host/target/release/slimeybeats-vst3-host.exe'
    if not host.is_file(): raise FileNotFoundError(host)
    shutil.copy2(host, root / host.name)
    copy_project_notices(REPOSITORY, root)
    copy_documentation(REPOSITORY, root)
    shutil.copy2(REPOSITORY / "mcp/tools/Connect-AI-Assistant.ps1", root / "Connect-AI-Assistant.ps1")
    (root / "Connect AI Assistant.bat").write_text(
        '@echo off\r\npowershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0Connect-AI-Assistant.ps1"\r\npause\r\n',
        encoding="ascii", newline="")
    examples = root / "Examples"
    examples.mkdir()
    for name in DEMO_FILES:
        shutil.copy2(REPOSITORY / "examples/audio" / name, examples / name)
    for name, project in (("MCP-dependencies", "mcp/server"), ("Desktop-dependencies", "rust"), ("Plugin-dependencies", "plugins/host")):
        notices = root / name
        notices.mkdir()
        dependency_notices(REPOSITORY / project, notices)
    label_line = f"Package label: {label}\n" if label else ""
    (root / "README.txt").write_text(
        f"SlimeyBeats MCP {version} - Studio preview for Windows x64\n{label_line}\n"
        "Extract this entire folder somewhere writable. Open Connect AI Assistant.bat\n"
        "to choose an input audio folder and an export folder, then save a configuration\n"
        "example. Add that connection to your MCP-compatible AI client. The setup helper\n"
        "does not change client settings. See docs/MCP.md.\n\n"
        "Try: Analyze this track and explain your suggested changes. Prepare a gentle\n"
        "master, then open it in the desktop app so I can listen before exporting.\n\n"
        "Included: local stdio MCP server, desktop build with session handoff, CC0 example,\n"
        "setup helper and dependency notices. The published v0.4.2 installer is unchanged.\n"
        "This preview includes desktop Studio, self-contained project handoff and\n"
        "the optional VST3 worker. The MCP companion exposes 42 tools; start with\n"
        "get_capabilities and get_help. See the component --version commands.\n\n"
        "WAV works without codecs. Compressed imports, FLAC/MP3 exports and resampling\n"
        "need FFmpeg/ffprobe. In MCP, inspect get_optional_tool_status; only after owner\n"
        "agreement call prepare_optional_tool(tool=audio_codecs, allow_download=true).\n"
        "Poll its job, then explicitly retry the original operation. This reuses capable\n"
        "system tools or prepares the verified private cache shared with the GUI.\n"
        "For vst3_host this tool explicitly prepares only the private recovery cache;\n"
        "it may download despite a bundled host and does not validate or replace it.\n"
        "In the GUI, requested actions automatically prepare missing optional tools\n"
        "in the private user cache and then continue. Opening Studio or using native\n"
        "WAV/voice cleanup/repair does not install a stem engine. No AI model is bundled.\n"
        "Audio is processed locally. Your AI client controls metadata sent to its provider.\n"
        "For support use prepare_support_report, show the full preview to the owner,\n"
        "then save_support_report only after explicit review/save approval with\n"
        "owner_review_confirmed=true. Returned report text may reach the AI client's\n"
        "model provider; filtering is best-effort. No report is uploaded or posted.\n"
        "One source and one heavy job at a time. Keep this folder while the client uses it.\n"
        "Your music remains yours. MIT software attribution applies to software copies.\n",
        encoding="utf-8")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED, compresslevel=6, strict_timestamps=False) as bundle:
        for path in sorted(root.rglob("*")):
            if path.is_file():
                bundle.write(path, path.relative_to(destination))
    print(json.dumps({"folder": str(root), "zip": str(archive)}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--server", type=Path, default=REPOSITORY / "mcp/server/target/release/slimeybeats-mcp.exe")
    parser.add_argument("--desktop", type=Path, default=REPOSITORY / "rust/target/release/slimeybeats-ez-master-live.exe")
    parser.add_argument("--label", help="Optional filename label, e.g. Hotfix2; application/component versions stay unchanged")
    args = parser.parse_args()
    package(args.out.resolve(), args.server.resolve(), args.desktop.resolve(), label=args.label)
