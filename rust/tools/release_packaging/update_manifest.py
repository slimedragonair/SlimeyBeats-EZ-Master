"""Produce immutable update catalogs from the actual executable identity and artifacts."""
import hashlib
import json
from pathlib import Path
import re
import subprocess
import tomllib

def expected_identity(project):
    project=Path(project)
    source=(project/'src/app_update/identity.rs').read_text(encoding='utf-8')
    revision=re.search(r'pub\s+const\s+BUILD_REVISION\s*:\s*u64\s*=\s*(\d+)\s*;',source)
    channel=re.search(r'pub\s+const\s+CHANNEL\s*:\s*&str\s*=\s*"([A-Za-z0-9_-]+)"\s*;',source)
    if not revision or not channel or int(revision[1])==0:
        raise ValueError('Missing or invalid update build identity')
    return {'version':tomllib.loads((project/'Cargo.toml').read_text())['package']['version'],
            'build_revision':int(revision[1]),'channel':channel[1]}

def verify_binary_identity(binary, expected):
    result=subprocess.run([str(binary),'--build-info'],capture_output=True,text=True,check=True)
    actual=json.loads(result.stdout)
    if actual!=expected:
        raise ValueError(f'GUI update identity differs from source: {actual!r} != {expected!r}')

def write(output, identity, installer, executable, components):
    output,installer,executable=map(Path,(output,installer,executable))
    name=f'UPDATE-MANIFEST-{identity["build_revision"]}.json'
    path=output/name
    if path.exists(): raise FileExistsError(path)
    digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
    value={'schema':1,**identity,'notes':'Application updates and automatic first-use setup for optional local audio tools.',
           'platforms':{'windows_x86_64':{'installer_asset':installer.name,'installer_sha256':digest(installer),
              'installer_bytes':installer.stat().st_size,'executable_sha256':digest(executable)}},'components':components}
    data=(json.dumps(value,indent=2)+'\n').encode('utf-8')
    if len(data)>65536: raise ValueError('Update manifest exceeds its 64 KiB client budget')
    path.write_bytes(data)
    return path
