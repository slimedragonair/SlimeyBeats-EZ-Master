"""Keep optional same-version build labels out of semantic/application versions."""
import re

def suffix(label):
    if label is None:
        return ''
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,47}', label) or '..' in label:
        raise ValueError('Artifact labels must be short safe filenames, such as Hotfix2')
    return '-' + label

def stem(version, label=None):
    return f'SlimeyBeats-EZ-Master-{version}' + suffix(label)
