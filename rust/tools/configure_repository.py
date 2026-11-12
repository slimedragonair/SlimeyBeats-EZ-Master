"""Wire public download links after the owner creates a GitHub repository; no network or push."""
import argparse
from pathlib import Path
import re

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repository', required=True, help='GitHub OWNER/REPOSITORY')
args=parser.parse_args()
if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+',args.repository):
    raise ValueError('Use OWNER/REPOSITORY, without a URL or spaces')
root=Path(__file__).resolve().parents[2]
url='https://github.com/'+args.repository
readme=root/'README.md'
text=readme.read_text(encoding='utf-8')
for label in ('Download Windows','Windows Setup.exe'):
    pattern=r'\['+re.escape(label)+r'\]\([^)]*\)'
    text,count=re.subn(pattern,f'[{label}]({url}/releases/latest)',text)
    if count!=1: raise ValueError(f'Expected one {label} link')
readme.write_text(text,encoding='utf-8')
manifest=root/'rust/Cargo.toml'
text=manifest.read_text()
if re.search(r'^repository\s*=',text,re.M):
    text=re.sub(r'^repository\s*=.*$',f'repository = "{url}"',text,flags=re.M)
else:
    text=text.replace('license = "MIT"',f'license = "MIT"\nrepository = "{url}"',1)
manifest.write_text(text)
print('Configured public download/Cargo links for '+url+'; nothing was uploaded.')
