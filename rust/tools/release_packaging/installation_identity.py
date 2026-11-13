"""Stable/preview install identities and Windows numeric version metadata."""
from dataclasses import dataclass
import re


@dataclass(frozen=True)
class InstallationIdentity:
    channel: str
    app_id: str
    product: str

    @property
    def app_key(self):
        return 'Software\\SlimeDragon Audio\\' + self.app_id

    @property
    def uninstall_key(self):
        return 'Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\' + self.app_id

    @property
    def start_folder(self):
        return 'SlimeDragon Audio/' + self.app_id


def for_channel(channel):
    if channel == 'stable':
        return InstallationIdentity(channel, 'SlimeyBeats-EZ-Master', 'SlimeyBeats-EZ-Master')
    if channel == 'studio-preview':
        return InstallationIdentity(channel, 'SlimeyBeats-EZ-Master-Studio-Preview', 'SlimeyBeats-EZ-Master Studio Preview')
    raise ValueError('Unknown installation channel')


def numeric_version(version):
    matched = re.fullmatch(r'(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?', version)
    if not matched or any(int(value) > 65535 for value in matched.groups()):
        raise ValueError('Unsupported Windows product version')
    return '.'.join(str(int(value)) for value in matched.groups()) + '.0'


def for_version(version):
    numeric_version(version)
    return for_channel('studio-preview' if '-' in version.partition('+')[0] else 'stable')
