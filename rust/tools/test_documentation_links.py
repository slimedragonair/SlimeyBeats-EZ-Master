"""Bundle checks cover newly added guides, encoded targets and path boundaries."""
from pathlib import Path
import tempfile
import unittest
from release_packaging.documentation_links import verify_local_links


class DocumentationLinkTests(unittest.TestCase):
    def test_encoded_local_targets_and_remote_anchors(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Example audio.wav').write_bytes(b'fixture')
            guide = root / 'Help.md'
            guide.write_text('[Audio](<Example%20audio.wav>) [Here](#terms) '
                             '[Remote](https://example.com/guide?q=a#b)', encoding='utf-8')
            self.assertEqual(verify_local_links([guide], root), 1)

    def test_missing_target_in_a_new_learning_guide_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            guide = root / 'LEARNING_AND_HELP.md'
            guide.write_text('[Setup](missing-guide.md)', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'LEARNING_AND_HELP.md.*missing-guide.md'):
                verify_local_links([guide], root)

    def test_existing_target_outside_the_bundle_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory)
            (parent / 'private.txt').write_text('outside fixture', encoding='utf-8')
            root = parent / 'bundle'
            root.mkdir()
            guide = root / 'Help.md'
            guide.write_text('[Outside](../private.txt)', encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Broken bundled link'):
                verify_local_links([guide], root)


if __name__ == '__main__':
    unittest.main()
