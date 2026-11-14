"""Compact, independently verifiable recovery bundles for the two shipped helpers."""
import hashlib
from pathlib import Path
import shutil
import tempfile
import zipfile
from .license_payload import contained_file
from .notice_preflight import collect

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def build(portable, output, version, label=None):
    from .artifact_labels import suffix
    portable, output = Path(portable), Path(output)
    result = {}
    plans = []
    for key, binary, notices in [('vst3_host', 'slimeybeats-vst3-host.exe', 'Plugin-dependencies'),
                                  ('mcp_bridge', 'slimeybeats-mcp.exe', 'MCP-dependencies')]:
        name = f'SlimeyBeats-{key}-{version}{suffix(label)}-Windows-x64.zip'
        archive = output/name
        if archive.exists():
            raise FileExistsError(archive)
        payload = collect(contained_file(portable, notices))
        plans.append((key, binary, name, archive, payload))
    # A bad second helper must not leave the first helper's ZIP ready to publish.
    for key, binary, name, archive, payload in plans:
        with tempfile.TemporaryDirectory(prefix='tool-bundle-', dir=output) as scratch:
            folder = Path(scratch)
            shutil.copy2(portable/binary, folder/binary)
            shutil.copy2(portable/'LICENSE', folder/'LICENSE')
            (folder/'THIRD-PARTY.json').write_bytes(payload.inventory)
            (folder/'THIRD-PARTY-NOTICES.txt').write_bytes(payload.summary)
            with zipfile.ZipFile(folder/'licenses.zip','w',zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
                for relative, data in sorted(payload.licenses.items()):
                    z.writestr(relative, data)
            files={p.name:sha(p) for p in sorted(folder.iterdir()) if p.is_file()}
            with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
                for file in sorted(folder.iterdir()): z.write(file,file.name)
        with zipfile.ZipFile(archive) as z:
            assert z.testzip() is None
            assert set(z.namelist())==set(files)
            assert all(hashlib.sha256(z.read(n)).hexdigest()==h for n,h in files.items())
        result[key]={'asset':name,'sha256':sha(archive),'bytes':archive.stat().st_size,'files':files,'protocol':1}
    return result
