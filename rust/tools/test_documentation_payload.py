"""Offline guides retain user links without pretending the source tree is installed."""
import unittest
from release_packaging.documentation_payload import offline_guide


class DocumentationPayloadTests(unittest.TestCase):
    def test_source_references_become_paths_and_user_workflows_keep_links(self):
        source = ('[Guide](USER_GUIDE.md#export) [Audio](../examples/audio/demo-input.wav) '
                  '[State](../rust/src/gui/user_experience.rs) [Web](../web/site/learning.js) '
                  '[MCP](../mcp/server/src/main.rs) [Workflow](../.github/workflows/web.yml) '
                  '[Host](../plugins/host/src/main.rs)')
        converted = offline_guide(source)
        self.assertIn('[Guide](USER_GUIDE.md#export)', converted)
        self.assertIn('[Audio](../Examples/demo-input.wav)', converted)
        self.assertIn('`rust/src/gui/user_experience.rs`', converted)
        self.assertIn('`web/site/learning.js`', converted)
        self.assertIn('`mcp/server/src/main.rs`', converted)
        self.assertIn('`.github/workflows/web.yml`', converted)
        self.assertIn('`plugins/host/src/main.rs`', converted)
        self.assertNotIn('](../rust/', converted)
        self.assertIn('separate source package', converted)


if __name__ == '__main__':
    unittest.main()
