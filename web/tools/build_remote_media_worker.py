"""Bundle the small, explicit media worker graph as a classic Service Worker.

Firefox 144 cannot evaluate module Service Workers. Keep page/worker policy
sources shared while generating a dependency-free classic entry for all engines.
This deliberately does not rewrite arbitrary JS or generated WASM bindings.
"""
from pathlib import Path
import re

IMPORTS = ('import { parseMediaUrl, validByteRange } from "./hybrid/remote-media-policy.js";',
           'import { serverEndpoint } from "./hybrid/endpoint-policy.js";')


def bundle_remote_media_worker(site):
    site = Path(site)
    worker = site / 'remote-media-worker.js'
    entry = worker.read_text(encoding='utf-8')
    # A fresh site copy is required; fail rather than accidentally bundling an
    # already-generated worker or silently dropping a new dependency.
    for statement in IMPORTS:
        if entry.count(statement) != 1:
            raise ValueError('Unexpected media worker dependency graph')
        entry = entry.replace(statement, '')
    sources = [site / 'hybrid/remote-media-policy.js', site / 'hybrid/endpoint-policy.js']
    parts = [path.read_text(encoding='utf-8') for path in sources] + [entry]
    parts = [re.sub(r'^export (?=(?:const|function)\b)', '', part, flags=re.M) for part in parts]
    if any(re.search(r'^\s*(?:import|export)\s|\bimport\.meta\b', part, flags=re.M) for part in parts):
        raise ValueError('Unsupported media worker module syntax')
    worker.write_text('// Generated classic Service Worker; edit web/site sources.\n'
                      '"use strict";\n(() => {\n' + '\n'.join(parts) + '\n})();\n',
                      encoding='utf-8', newline='\n')
