import tempfile
from pathlib import Path
import unittest
from runtime_cache import version_runtime


class RuntimeCacheTests(unittest.TestCase):
    def build(self, root, wasm):
        root.mkdir()
        (root/'pkg').mkdir()
        (root/'index.html').write_text('<script src="app.js"></script><link href="styles.css">')
        (root/'app.js').write_text('import "./player.js"; new Worker(new URL("./worker.js",import.meta.url));')
        (root/'worker.js').write_text('import init from "./pkg/dsp.js";')
        (root/'player.js').write_text('fetch(new URL("./pkg/dsp_bg.wasm",import.meta.url));')
        (root/'pkg/dsp.js').write_text('const imports={"./dsp_bg.js":{}}; new URL("dsp_bg.wasm",import.meta.url)')
        (root/'pkg/dsp_bg.wasm').write_bytes(wasm)
        (root/'styles.css').write_text('body{}')
        return version_runtime(root)

    def test_bindings_workers_and_wasm_share_a_version(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)/'site'
            version=self.build(root,b'wasm-a')
            for name in ['index.html','app.js','worker.js','player.js','pkg/dsp.js']:
                self.assertIn('?v='+version,(root/name).read_text())
            self.assertIn('"./dsp_bg.js":{}',(root/'pkg/dsp.js').read_text())

    def test_changed_wasm_invalidates_the_entrypoint_and_dependencies(self):
        with tempfile.TemporaryDirectory() as temp:
            first=self.build(Path(temp)/'first',b'wasm-a')
            second=self.build(Path(temp)/'second',b'wasm-b')
            same=self.build(Path(temp)/'same',b'wasm-a')
            self.assertNotEqual(first,second)
            self.assertEqual(first,same)


if __name__=='__main__':
    unittest.main()
