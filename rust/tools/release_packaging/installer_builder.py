"""Build a per-user NSIS installer and an explicit, data-preserving uninstall list."""
from pathlib import Path
import shutil
import subprocess

def escaped(text):
    return str(text).replace('$', '$$').replace('"', '$\\"')

def build_installer(project, portable, output, compiler, version):
    project, portable, output = map(lambda path: Path(path).resolve(), (project, portable, output))
    build = project / 'target/installer' / version
    if build.exists():
        raise FileExistsError(f'Installer staging already exists: {build}')
    payload = build / 'payload'
    payload.mkdir(parents=True)
    root_files = ('SlimeyBeats-EZ-Master.exe', 'SlimeyBeats-EZ-Master-CLI.exe', 'README.txt',
                  'LICENSE', 'OUTPUT_OWNERSHIP.md', 'THIRD-PARTY.json', 'THIRD-PARTY-NOTICES.txt')
    for name in root_files:
        shutil.copy2(portable / name, payload / name)
    for name in ('docs', 'licenses', 'Examples'):
        shutil.copytree(portable / name, payload / name)
    shutil.copy2(project / 'assets/slimeybeats-installer.ico', payload / 'SlimeyBeats.ico')
    # NSIS is a build tool; keep the redistributable stub's original notice with the release.
    notice = Path(compiler).parent / 'COPYING'
    if not notice.is_file():
        notice = Path(compiler).parent / 'Docs/AppendixI.html'
    if not notice.is_file():
        raise FileNotFoundError('NSIS license notice not found')
    notice_dir = payload / 'licenses/nsis-3.13'
    notice_dir.mkdir(parents=True)
    shutil.copy2(notice, notice_dir / notice.name)
    files = sorted(path for path in payload.rglob('*') if path.is_file())
    install_lines, remove_lines = [], []
    folders = set()
    for path in files:
        relative = path.relative_to(payload)
        folder = str(relative.parent).replace('/', '\\')
        destination = '$INSTDIR' + ('' if folder == '.' else '\\' + escaped(folder))
        install_lines.append(f'SetOutPath "{destination}"')
        if relative.parts[0] == 'Examples':
            install_lines.append('SetOverwrite off')
        install_lines.append(f'File "/oname={escaped(path.name)}" "{escaped(path)}"')
        if relative.parts[0] == 'Examples':
            install_lines.append('SetOverwrite on')
        if relative.parts[0] != 'Examples':
            remove_lines.append(f'Delete "$INSTDIR\\{escaped(relative)}"')
            for parent in relative.parents:
                if str(parent) != '.':
                    folders.add(parent)
    for folder in sorted(folders, key=lambda path: (-len(path.parts), str(path))):
        remove_lines.append(f'RMDir "$INSTDIR\\{escaped(folder)}"')
    install_include = build / 'install-files.nsh'
    uninstall_include = build / 'uninstall-files.nsh'
    install_include.write_text('\n'.join(install_lines) + '\n', encoding='utf-8')
    uninstall_include.write_text('\n'.join(remove_lines) + '\n', encoding='utf-8')
    installer = output / f'SlimeyBeats-EZ-Master-{version}-Setup.exe'
    if installer.exists():
        raise FileExistsError(installer)
    subprocess.run([str(compiler), '/V3', '/WX', f'/DAPP_VERSION={version}',
                    f'/DOUTPUT_FILE={installer}', f'/DINSTALL_FILES={install_include}',
                    f'/DUNINSTALL_FILES={uninstall_include}',
                    f'/DAPP_ICON={project / "assets/slimeybeats-installer.ico"}',
                    f'/DESTIMATED_KIB={sum(path.stat().st_size for path in files) // 1024}',
                    str(project / 'tools/installer/SlimeyBeatsInstaller.nsi')], check=True)
    return installer
