import unittest
from release_packaging.installation_identity import for_channel, for_version, numeric_version


class InstallationIdentityTests(unittest.TestCase):
    def test_stable_paths_are_unchanged_and_preview_is_separate(self):
        stable = for_version('0.4.2')
        preview = for_version('0.5.0-beta.1')
        self.assertEqual(stable.app_id, 'SlimeyBeats-EZ-Master')
        self.assertEqual(stable.app_key, r'Software\SlimeDragon Audio\SlimeyBeats-EZ-Master')
        self.assertNotEqual(stable.app_key, preview.app_key)
        self.assertNotEqual(stable.uninstall_key, preview.uninstall_key)
        self.assertNotEqual(stable.start_folder, preview.start_folder)
        self.assertNotEqual(stable.product, preview.product)
        self.assertEqual(preview, for_channel('studio-preview'))

    def test_prerelease_metadata_is_numeric_for_windows(self):
        self.assertEqual(numeric_version('0.5.0-beta.1'), '0.5.0.0')
        self.assertEqual(numeric_version('1.2.3+build.4'), '1.2.3.0')
        self.assertEqual(for_version('1.2.3+build.4').channel, 'stable')
        for bad in ['not-a-version', '0.5', '65536.0.0', '1.2.3/unsafe']:
            with self.assertRaises(ValueError):
                numeric_version(bad)


if __name__ == '__main__':
    unittest.main()
