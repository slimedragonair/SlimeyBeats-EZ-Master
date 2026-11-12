"""Exercise the actual per-user installer in a fresh folder, preserving sentinel user data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import winreg

UNINSTALL = r'Software\Microsoft\Windows\CurrentVersion\Uninstall\SlimeyBeats-EZ-Master'
APPKEY = r'Software\SlimeDragon Audio\SlimeyBeats-EZ-Master'
ACCESS = winreg.KEY_READ | winreg.KEY_WOW64_64KEY

def registry_value(key, name):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key, 0, ACCESS) as handle:
            return winreg.QueryValueEx(handle, name)[0]
    except FileNotFoundError:
        return None

def has_key(key):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, key, 0, ACCESS): return True
    except FileNotFoundError: return False

def digest(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def command(args, **options):
    return subprocess.run(args, capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=60, **options)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installer', type=Path, required=True)
    parser.add_argument('--test-root', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--skip-gui', action='store_true', help='Verify command-line lifecycle only; record GUI capture as untested')
    args = parser.parse_args()
    installer, root = args.installer.resolve(), args.test_root.resolve()
    if root.exists() or root == Path(root.anchor) or '"' in str(root):
        raise ValueError('Test root must be a new dedicated folder')
    if any((ancestor / 'mastercraft/data/builtin_profiles.json').is_file() for ancestor in root.parents):
        raise ValueError('Installer QA must be outside a development checkout')
    shortcuts = Path(os.environ['APPDATA']) / 'Microsoft/Windows/Start Menu/Programs/SlimeDragon Audio/SlimeyBeats-EZ-Master'
    if has_key(UNINSTALL) or has_key(APPKEY) or shortcuts.exists():
        raise RuntimeError('An existing installation/shortcut group is present; it will not be disturbed')
    # NSIS /D= must be the last, unquoted parameter, including when its value contains spaces.
    rejected = command(f'"{installer}" /S /D={root.anchor}')
    assert rejected.returncode != 0 and not has_key(UNINSTALL), 'Drive-root installation was not rejected'
    result = command(f'"{installer}" /S /D={root}')
    if result.returncode: raise RuntimeError(f'Installer failed: {result.returncode}')
    registered = registry_value(UNINSTALL, 'InstallLocation')
    if not registered or Path(registered).resolve() != root:
        raise RuntimeError('Installer did not use the explicit QA folder; refusing unrelated cleanup')
    preserved = {}
    record = {'installer_sha256': digest(installer), 'drive_root_rejected': True, 'install_exit': result.returncode}
    try:
        gui, cli = root/'SlimeyBeats-EZ-Master.exe', root/'SlimeyBeats-EZ-Master-CLI.exe'
        assert gui.is_file() and cli.is_file()
        assert all((shortcuts/name).is_file() for name in ('SlimeyBeats-EZ-Master.lnk','Example audio.lnk','User guide.lnk','Uninstall.lnk'))
        version = command([str(cli), '--version'])
        assert version.returncode == 0
        record['installed_version'] = version.stdout.strip()
        environment = dict(os.environ)
        environment['PATH'] = str(Path(os.environ['SystemRoot'])/'System32')
        assert shutil.which('ffmpeg', path=environment['PATH']) is None
        master = command([str(cli), str(root/'Examples/demo-input.wav'), '--format', 'wav24', '--out', str(root/'output'), '--threads', '4'], env=environment, cwd=root)
        assert master.returncode == 0, master.stderr
        wave = next((root/'output').glob('*.wav'))
        record['native_wav_without_ffmpeg'] = True
        if args.skip_gui:
            record['installed_gui_processing_capture'] = False
            record['gui_capture_skip_reason'] = 'Explicit --skip-gui: command-line installer lifecycle only'
        else:
            screenshot = args.report.resolve().with_name('installed-app.png')
            screenshot.parent.mkdir(parents=True, exist_ok=True)
            capture = command([str(gui), str(root/'Examples/demo-input.wav'), '--auto-master', '--view', 'eq', '--capture-ui', str(screenshot)], cwd=root)
            assert capture.returncode == 0 and screenshot.is_file(), f'Installed GUI capture failed (exit {capture.returncode}):\n{capture.stdout}\n{capture.stderr}'
            record['installed_gui_processing_capture'] = True
        settings = root/'user_data/rust-settings.json'
        settings.parent.mkdir(exist_ok=True)
        native_report = json.loads(next((root/'output').glob('*.report.json')).read_text())
        config = json.loads(settings.read_text()) if settings.exists() else native_report['configuration']
        config['intensity'] = .37
        settings.write_text(json.dumps(config, indent=2))
        profiles = root/'profiles'; profiles.mkdir(exist_ok=True)
        profile_source = Path(__file__).resolve().parents[2]/'mastercraft/data/builtin_profiles.json'
        profile = json.loads(profile_source.read_text())['profiles']['neutral']
        profile['display_name'] = 'QA preserved profile'
        custom = profiles/'qa-user-profile.json'; custom.write_text(json.dumps(profile))
        example = root/'Examples/demo-input.wav'
        with example.open('ab') as stream: stream.write(b'QA preserved user edit')
        preserved = {str(path):digest(path) for path in (settings, custom, wave, example)}
        upgraded = command(f'"{installer}" /S')
        assert upgraded.returncode == 0
        assert Path(registry_value(UNINSTALL,'InstallLocation')).resolve() == root
        assert all(digest(path)==checksum for path,checksum in preserved.items())
        record['upgrade_kept_custom_install_path_and_user_files'] = True
    finally:
        uninstall = root/'Uninstall.exe'
        if uninstall.is_file():
            command([str(uninstall), '/S'], cwd=root.parent)
            deadline = time.monotonic()+30
            while time.monotonic()<deadline and (has_key(UNINSTALL) or (root/'SlimeyBeats-EZ-Master.exe').exists()):
                time.sleep(.1)
    assert not has_key(UNINSTALL) and not has_key(APPKEY)
    assert not (root/'SlimeyBeats-EZ-Master.exe').exists() and not (root/'SlimeyBeats-EZ-Master-CLI.exe').exists()
    assert not shortcuts.exists()
    assert preserved and all(Path(path).is_file() and digest(path)==checksum for path,checksum in preserved.items())
    record.update(uninstall_removed_program_shortcuts_registration=True,
                  uninstall_preserved_settings_profile_export_edited_example=True)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps(record, indent=2))

if __name__=='__main__': main()
