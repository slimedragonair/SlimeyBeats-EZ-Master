"""Version the connected JS/WASM/CSS graph in a fresh static build."""
import hashlib
from pathlib import Path
import re

# Relative module/worker URLs and wasm-bindgen's generated WASM URL. A single
# version keeps old browser caches from mixing incompatible bindings and WASM.
MODULE = re.compile(r'''((?:from\s+|import\s*(?:\(\s*)?|new\s+URL\(\s*))(["'])((?:\.{1,2}/[^"'?#]+\.(?:js|wasm))|dsp_bg\.wasm)\2''')
HTML_ASSET = re.compile(r'''((?:src|href)=)(["'])([^"'?#]+\.(?:js|css))\2''')


def version_runtime(root):
    root = Path(root)
    files = sorted(path for path in root.rglob('*') if path.suffix in {'.js','.wasm','.css','.html'})
    digest = hashlib.sha256()
    digest.update(Path(__file__).read_bytes())
    for path in files:
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(path.read_bytes())
    version = digest.hexdigest()[:16]
    for path in files:
        if path.suffix == '.js':
            text = path.read_text(encoding='utf-8')
            text = MODULE.sub(lambda m:f'{m[1]}{m[2]}{m[3]}?v={version}{m[2]}',text)
            path.write_text(text,encoding='utf-8',newline='\n')
        elif path.suffix == '.html':
            text = path.read_text(encoding='utf-8')
            text = HTML_ASSET.sub(lambda m:f'{m[1]}{m[2]}{m[3]}?v={version}{m[2]}',text)
            path.write_text(text,encoding='utf-8',newline='\n')
    return version
