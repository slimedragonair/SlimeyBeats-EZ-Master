"""Fetch the pinned official portable NSIS compiler into the build cache."""
import hashlib
from pathlib import Path, PurePosixPath
import urllib.request
import zipfile

VERSION = '3.13'
URL = 'https://downloads.sourceforge.net/project/nsis/NSIS%203/3.13/nsis-3.13.zip'
SHA256 = 'ba63dffc4410ee89193e1cb5a41989991bd77c61068da17e3156d136b7b0b3d8'

def ensure_nsis(cache):
    cache = Path(cache).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / f'nsis-{VERSION}.zip'
    if not archive.is_file():
        with urllib.request.urlopen(URL, timeout=60) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != SHA256:
            raise ValueError('NSIS download checksum mismatch')
        archive.write_bytes(data)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise ValueError('Cached NSIS checksum mismatch')
    with zipfile.ZipFile(archive) as bundle:
        for item in bundle.infolist():
            path = PurePosixPath(item.filename.replace('\\', '/'))
            if path.is_absolute() or '..' in path.parts or ':' in item.filename:
                raise ValueError('Unsafe compiler archive path')
        if bundle.testzip() is not None:
            raise ValueError('Corrupt NSIS archive')
        bundle.extractall(cache)
    compiler = cache / f'nsis-{VERSION}/makensis.exe'
    if not compiler.is_file():
        raise FileNotFoundError(compiler)
    return compiler
