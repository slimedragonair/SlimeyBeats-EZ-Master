"""Bundle the matching MCP/VST workers, configuration helper and their notices."""
from pathlib import Path
import shutil
import subprocess
import tomllib


def companion_binaries(repository):
    repository = Path(repository)
    return {
        repository / 'mcp/server/target/release/slimeybeats-mcp.exe': 'slimeybeats-mcp.exe',
        repository / 'plugins/host/target/release/slimeybeats-vst3-host.exe': 'slimeybeats-vst3-host.exe',
    }


def copy_companion(repository, destination, notice_writer):
    repository, destination = Path(repository), Path(destination)
    for source, name in companion_binaries(repository).items():
        if not source.is_file():
            raise FileNotFoundError(source)
        manifest = source.parents[2] / 'Cargo.toml'
        version = tomllib.loads(manifest.read_text())['package']['version']
        output = subprocess.check_output([str(source), '--version'], text=True).strip()
        if not output.endswith(' ' + version):
            raise ValueError(f'Build the matching {name} before packaging')
        shutil.copy2(source, destination / name)
    shutil.copy2(repository / 'mcp/tools/Connect-AI-Assistant.ps1', destination / 'Connect-AI-Assistant.ps1')
    (destination / 'Connect AI Assistant.bat').write_text(
        '@echo off\r\npowershell.exe -NoProfile -STA -ExecutionPolicy Bypass -File "%~dp0Connect-AI-Assistant.ps1"\r\npause\r\n',
        encoding='ascii', newline='')
    for name, project in [('MCP-dependencies', 'mcp/server'), ('Plugin-dependencies', 'plugins/host')]:
        notices = destination / name
        notices.mkdir()
        notice_writer(repository / project, notices)
