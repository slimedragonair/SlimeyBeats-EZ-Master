"""Exercise the actual per-user installer in a fresh folder, preserving sentinel user data."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import winreg
from release_packaging.installation_identity import for_channel

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
    timeout = options.pop('timeout', 60)
    return subprocess.run(args, capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=timeout, **options)


def installation_snapshot(identity):
    values = {name: registry_value(identity.app_key, name) for name in ['InstallDir']}
    values.update({name: registry_value(identity.uninstall_key, name) for name in ['InstallLocation', 'DisplayVersion', 'DisplayName']})
    folder = values['InstallLocation']
    files = {}
    if folder:
        for name in ['SlimeyBeats-EZ-Master.exe', 'SlimeyBeats-EZ-Master-CLI.exe',
                     'user_data/rust-settings.json', 'user_data/desktop-preferences.json',
                     'user_data/interface-preferences.json', 'user_data/tutorial-dismissed.marker']:
            path = Path(folder) / name
            if path.is_file(): files[name] = digest(path)
    shortcuts = Path(os.environ['APPDATA'])/'Microsoft/Windows/Start Menu/Programs'/identity.start_folder
    for name in [identity.product+'.lnk','Example audio.lnk','User guide.lnk','Uninstall.lnk']:
        path = shortcuts/name
        if path.is_file(): files['shortcut/'+name] = digest(path)
    return {'registry': values, 'files': files}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--installer', type=Path, required=True)
    parser.add_argument('--test-root', type=Path, required=True)
    parser.add_argument('--report', type=Path, required=True)
    parser.add_argument('--skip-gui', action='store_true', help='Verify command-line lifecycle only; record GUI capture as untested')
    parser.add_argument('--channel', choices=['stable', 'studio-preview'], default='stable')
    args = parser.parse_args()
    captures = {name:args.report.resolve().with_name(args.report.stem+'-'+name+'.png') for name in ['app','studio','preferences']}
    for path in [args.report.resolve(),*captures.values(),*(path.with_suffix('.state.json') for path in captures.values())]:
        if path.exists(): raise FileExistsError('Choose a fresh verification report: '+str(path))
    installer, root = args.installer.resolve(), args.test_root.resolve()
    if root.exists() or root == Path(root.anchor) or '"' in str(root):
        raise ValueError('Test root must be a new dedicated folder')
    if any((ancestor / 'mastercraft/data/builtin_profiles.json').is_file() for ancestor in root.parents):
        raise ValueError('Installer QA must be outside a development checkout')
    identity = for_channel(args.channel)
    uninstall_key, app_key = identity.uninstall_key, identity.app_key
    other = for_channel('stable' if args.channel == 'studio-preview' else 'studio-preview')
    other_before = installation_snapshot(other)
    shortcuts = Path(os.environ['APPDATA']) / 'Microsoft/Windows/Start Menu/Programs' / identity.start_folder
    if has_key(uninstall_key) or has_key(app_key) or shortcuts.exists():
        raise RuntimeError('An existing installation/shortcut group is present; it will not be disturbed')
    # NSIS /D= must be the last, unquoted parameter, including when its value contains spaces.
    rejected = command(f'"{installer}" /S /D={root.anchor}')
    assert rejected.returncode != 0 and not has_key(uninstall_key), 'Drive-root installation was not rejected'
    foreign = root.with_name(root.name + ' foreign channel')
    foreign.mkdir(exist_ok=False)
    marker = foreign / 'install-info.ini'
    marker.write_text('[Install]\nChannel=' + other.channel + '\n')
    sentinel = foreign / 'SlimeyBeats-EZ-Master.exe'
    sentinel.write_bytes(b'QA other-channel sentinel')
    rejected = command(f'"{installer}" /S /D={foreign}')
    assert rejected.returncode != 0 and sentinel.read_bytes() == b'QA other-channel sentinel' and not has_key(uninstall_key)
    marker.unlink(); sentinel.unlink(); foreign.rmdir()
    result = command(f'"{installer}" /S /D={root}')
    if result.returncode: raise RuntimeError(f'Installer failed: {result.returncode}')
    registered = registry_value(uninstall_key, 'InstallLocation')
    if not registered or Path(registered).resolve() != root:
        raise RuntimeError('Installer did not use the explicit QA folder; refusing unrelated cleanup')
    preserved = {}
    record = {'installer_sha256': digest(installer), 'channel':args.channel, 'drive_root_rejected': True, 'other_channel_marker_rejected':True, 'install_exit': result.returncode}
    try:
        gui, cli = root/'SlimeyBeats-EZ-Master.exe', root/'SlimeyBeats-EZ-Master-CLI.exe'
        gui_command = [str(gui),'--studio-data',str(root/'QA Studio runtime')]
        assert gui.is_file() and cli.is_file()
        assert all((shortcuts/name).is_file() for name in (identity.product+'.lnk','Example audio.lnk','User guide.lnk','Connect AI assistant.lnk','Uninstall.lnk'))
        assert all((root/name).is_file() for name in ['slimeybeats-mcp.exe','slimeybeats-vst3-host.exe','Connect-AI-Assistant.ps1','Connect AI Assistant.bat'])
        for binary in ['slimeybeats-mcp.exe', 'slimeybeats-vst3-host.exe']:
            assert command([str(root/binary), '--version']).returncode == 0
        record['mcp_and_plugin_worker_installed'] = True
        repository = Path(__file__).resolve().parents[2]
        for script in ['verify_studio.py', 'verify_studio_origin.py', 'verify_plugin_faults.py']:
            checked = command([sys.executable, str(repository/'mcp/tools'/script), '--server', str(root/'slimeybeats-mcp.exe')], cwd=root, timeout=300 if script == 'verify_studio_origin.py' else 60)
            assert checked.returncode == 0, (script, checked.stdout, checked.stderr)
        record['installed_mcp_studio_origin_and_plugin_faults'] = True
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
        sys.path.insert(0,str(repository/'mcp/tools'))
        from protocol_client import Client
        client = Client([str(root/'slimeybeats-mcp.exe'),'--input-root',str(root/'Examples'),'--output-root',str(root/'Studio projects')])
        try:
            loaded = client.run('load_audio',{'library':0,'path':'demo-input.wav'})
            client.run('clean_voice',{'audio_id':loaded['source_id'],'settings':{'noise_reduction':0,'deess':0}})
            saved = client.run('save_studio_project')
            studio_document = root/'Studio projects'/saved['project_path']
        finally: client.close()
        if args.skip_gui:
            record['installed_gui_processing_capture'] = False
            record['gui_capture_skip_reason'] = 'Explicit --skip-gui: command-line installer lifecycle only'
        else:
            screenshot = captures['app']
            screenshot.parent.mkdir(parents=True, exist_ok=True)
            capture = command([*gui_command, str(root/'Examples/demo-input.wav'), '--auto-master', '--view', 'eq', '--capture-ui', str(screenshot)], cwd=root)
            assert capture.returncode == 0 and screenshot.is_file(), f'Installed GUI capture failed (exit {capture.returncode}):\n{capture.stdout}\n{capture.stderr}'
            record['installed_gui_processing_capture'] = True
            studio_capture = captures['studio']
            checked = command([*gui_command,'--studio-session',str(studio_document),'--capture-ui',str(studio_capture)],cwd=root)
            assert checked.returncode == 0, checked.stderr
            state = json.loads(studio_capture.with_suffix('.state.json').read_text())
            assert state['error'] is None and state['preview_ready'] and state['playback_ready'],state
            record['installed_studio_project_reopened'] = True
        settings = root/'user_data/rust-settings.json'
        settings.parent.mkdir(exist_ok=True)
        native_report = json.loads(next((root/'output').glob('*.report.json')).read_text())
        config = json.loads(settings.read_text()) if settings.exists() else native_report['configuration']
        config['intensity'] = .37
        config['out_dir'] = str(root/'Preserved mastering exports')
        settings.write_text(json.dumps(config, indent=2))
        desktop = settings.with_name('desktop-preferences.json')
        desktop.write_text(json.dumps({'studio_output_dir':str(root/'Preserved Studio exports'),
                                      'studio_export_format':'wav24', 'volume':0,
                                      'studio_volume':0, 'two_stems':False}, indent=2))
        interface = settings.with_name('interface-preferences.json')
        interface.write_text(json.dumps({'advanced':True, 'tutorial_status':'completed',
                                        'tutorial_step':7}, indent=2))
        tutorial_marker = settings.with_name('tutorial-dismissed.marker')
        tutorial_marker.write_bytes(b'')
        profiles = root/'profiles'; profiles.mkdir(exist_ok=True)
        profile_source = Path(__file__).resolve().parents[2]/'mastercraft/data/builtin_profiles.json'
        profile = json.loads(profile_source.read_text())['profiles']['neutral']
        profile['display_name'] = 'QA preserved profile'
        custom = profiles/'qa-user-profile.json'; custom.write_text(json.dumps(profile))
        example = root/'Examples/demo-input.wav'
        with example.open('ab') as stream: stream.write(b'QA preserved user edit')
        project_files = [path for path in studio_document.parent.iterdir() if path.is_file()]
        preserved = {str(path):digest(path) for path in
                     [settings, desktop, interface, tutorial_marker, custom, wave, example, *project_files]}
        upgraded = command(f'"{installer}" /S')
        assert upgraded.returncode == 0
        assert Path(registry_value(uninstall_key,'InstallLocation')).resolve() == root
        assert all(digest(path)==checksum for path,checksum in preserved.items())
        record['upgrade_kept_custom_install_path_and_user_files'] = True
        record['upgrade_preserved_export_destinations_and_tutorial_choice'] = True
        if not args.skip_gui:
            # Loading and saving through the installed app verifies the actual
            # portable data root, beyond merely retaining sentinel files.
            reopened = command([*gui_command,'--capture-ui',str(captures['preferences']),
                                '--capture-save-settings'],cwd=root)
            assert reopened.returncode == 0, reopened.stderr
            restored = json.loads(settings.read_text())
            restored_desktop = json.loads(desktop.read_text())
            restored_interface = json.loads(interface.read_text())
            assert restored == config, 'Installed GUI changed saved mastering settings'
            assert restored_desktop['studio_output_dir'] == str(root/'Preserved Studio exports')
            assert restored_desktop['volume'] == 0 and restored_desktop['studio_volume'] == 0
            assert not restored_desktop['two_stems'] and restored_desktop['studio_export_format'] == 'wav24'
            assert restored_interface['advanced'] and restored_interface['tutorial_status'] == 'completed'
            assert restored_interface['tutorial_step'] == 7
            assert tutorial_marker.is_file()
            status = json.loads(captures['preferences'].with_suffix('.state.json').read_text())
            assert status['error'] is None, status
            record['installed_gui_restored_preferences_after_upgrade'] = True
            # Save settings may normalize JSON formatting; uninstall must retain
            # those newly saved bytes as well as the untouched music/projects.
            preference_files = {str(path) for path in [settings,desktop,interface]}
            assert all(digest(path) == checksum for path,checksum in preserved.items()
                       if path not in preference_files), 'Reopening changed non-preference user files'
            preserved.update({path:digest(path) for path in preference_files})
    finally:
        uninstall = root/'Uninstall.exe'
        if uninstall.is_file():
            command([str(uninstall), '/S'], cwd=root.parent)
            deadline = time.monotonic()+30
            while time.monotonic()<deadline and (has_key(uninstall_key) or (root/'SlimeyBeats-EZ-Master.exe').exists()):
                time.sleep(.1)
    assert not has_key(uninstall_key) and not has_key(app_key)
    assert not (root/'SlimeyBeats-EZ-Master.exe').exists() and not (root/'SlimeyBeats-EZ-Master-CLI.exe').exists()
    assert not shortcuts.exists()
    assert all(not (root/name).exists() for name in ['slimeybeats-mcp.exe','slimeybeats-vst3-host.exe','Connect-AI-Assistant.ps1','Connect AI Assistant.bat'])
    assert installation_snapshot(other) == other_before, 'Other installed channel changed'
    assert preserved and all(Path(path).is_file() and digest(path)==checksum for path,checksum in preserved.items())
    record.update(uninstall_removed_program_shortcuts_registration=True,
                  uninstall_preserved_settings_profile_export_edited_example=True,
                  uninstall_preserved_studio_project=True,
                  uninstall_preserved_desktop_interface_and_tutorial_marker=True)
    record['other_installed_channel_unchanged'] = True
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(record, indent=2)+'\n')
    print(json.dumps(record, indent=2))

if __name__=='__main__': main()
