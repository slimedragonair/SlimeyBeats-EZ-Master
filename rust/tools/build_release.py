"""Build local portable/source archives and Setup.exe; never publishes anything."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tomllib
import zipfile
from package_release import package
from release_packaging.public_source import copy_public_source
from release_packaging.nsis_toolchain import ensure_nsis
from release_packaging.installer_builder import build_installer

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True)
    parser.add_argument('--nsis',type=Path)
    parser.add_argument('--evidence',type=Path)
    args=parser.parse_args()
    project=Path(__file__).resolve().parents[1]
    version=tomllib.loads((project/'Cargo.toml').read_text())['package']['version']
    binary=project/'target/release/slimeybeats-ez-master-cli.exe'
    result=subprocess.run([str(binary),'--version'],capture_output=True,text=True,check=True)
    if result.stdout.strip()!=f'SlimeyBeats-EZ-Master {version}':
        raise RuntimeError('Build the matching release binaries before packaging')
    output=args.out.resolve(); output.mkdir(parents=True,exist_ok=True)
    source=output/f'SlimeyBeats-EZ-Master-{version}-Source'
    source_zip=source.with_name(source.name+'.zip')
    if source.exists() or source_zip.exists(): raise FileExistsError(source)
    compiler=args.nsis.resolve() if args.nsis else ensure_nsis(project/'target/release-tools')
    package(project,output,version,args.evidence)
    copy_public_source(project.parent,source)
    with zipfile.ZipFile(source_zip,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as archive:
        for path in sorted(source.rglob('*')):
            if path.is_file(): archive.write(path,path.relative_to(output))
    portable=output/f'SlimeyBeats-EZ-Master-{version}'
    installer=build_installer(project,portable,output,compiler,version)
    artifacts=[installer,portable.with_name(portable.name+'.zip'),source_zip]
    checksums=''.join(hashlib.sha256(path.read_bytes()).hexdigest()+'  '+path.name+'\n' for path in artifacts)
    (output/f'SlimeyBeats-EZ-Master-{version}-SHA256SUMS.txt').write_text(checksums,encoding='ascii')
    print(json.dumps({'version':version,'installer':str(installer),'portable':str(artifacts[1]),'source':str(source_zip)},indent=2))

if __name__=='__main__': main()
