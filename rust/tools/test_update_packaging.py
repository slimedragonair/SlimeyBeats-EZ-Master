"""Exercise producer/client update contracts without launching an installer."""
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile
from release_packaging.artifact_labels import stem
from release_packaging import native_tool_bundles, update_manifest
from release_packaging.license_payload import copy_dependency_licenses

class UpdatePackagingTests(unittest.TestCase):
    def helper_fixture(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        portable = root / 'portable'
        portable.mkdir()
        (portable / 'LICENSE').write_bytes(b'application MIT notice')
        crate = root / 'crate'
        crate.mkdir()
        originals = {'LICENSE': b'original root notice',
                     'vendor/one/LICENSE': b'original vendored notice one',
                     'vendor/two/LICENSE': b'original vendored notice two'}
        for relative, data in originals.items():
            file = crate / relative
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(data)
        empty = root / 'metadata-only-crate'
        empty.mkdir()
        package = {'name': 'example', 'version': '1.2.3', 'source': 'registry+example',
                   'manifest_path': str(crate / 'Cargo.toml'), 'license': 'MIT',
                   'license_file': None, 'repository': 'https://github.com/example/project',
                   'checksum': 'a' * 64}
        packages = [package, {**package, 'name': 'metadata_only', 'version': '0.1.0',
                              'manifest_path': str(empty / 'Cargo.toml')}]
        notices = []
        for binary, folder in [('slimeybeats-vst3-host.exe', 'Plugin-dependencies'),
                               ('slimeybeats-mcp.exe', 'MCP-dependencies')]:
            (portable / binary).write_bytes(binary.encode())
            directory = portable / folder
            records = copy_dependency_licenses(packages, directory)
            (directory / 'THIRD-PARTY.json').write_text(json.dumps(records), encoding='utf-8')
            (directory / 'THIRD-PARTY-NOTICES.txt').write_bytes(b'dependency attribution fixture')
            notices.append(directory)
        return SimpleNamespace(root=root, portable=portable, notices=notices, originals=originals)

    def reject_helper(self, fixture, message=None):
        with self.assertRaisesRegex(ValueError, message or '.'):
            native_tool_bundles.build(fixture.portable, fixture.root, '0.5.0-beta.3', 'Hotfix2')
        self.assertEqual(list(fixture.root.glob('*.zip')), [])
        self.assertEqual(list(fixture.root.glob('tool-bundle-*')), [])

    def provenance(self, fixture):
        path = fixture.notices[1] / 'licenses/example-1.2.3/LICENSE-PROVENANCE.json'
        return path, json.loads(path.read_text())

    def test_hotfix_label_changes_filenames_without_changing_application_version(self):
        self.assertEqual(stem('0.5.0-beta.3','Hotfix2'),'SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix2')
        for unsafe in ['../other','C:\\outside','bad/label','..','foo..bar','x'*60]:
            with self.assertRaises(ValueError):stem('0.5.0-beta.3',unsafe)

    def test_actual_executable_identity_must_match_source_before_packaging(self):
        expected={'version':'0.5.0-beta.3','build_revision':2026111402,'channel':'studio-preview'}
        with patch.object(update_manifest.subprocess,'run',return_value=SimpleNamespace(stdout=json.dumps(expected))) as command:
            update_manifest.verify_binary_identity(Path('gui.exe'),expected)
            self.assertEqual(command.call_args.args[0],['gui.exe','--build-info'])
        with patch.object(update_manifest.subprocess,'run',return_value=SimpleNamespace(stdout=json.dumps({**expected,'build_revision':1}))):
            with self.assertRaises(ValueError):update_manifest.verify_binary_identity(Path('old.exe'),expected)

    def test_catalog_records_real_bytes_and_refuses_to_overwrite_an_existing_revision(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);setup=root/'SlimeyBeats-EZ-Master-0.5.0-beta.3-Hotfix2-Setup.exe';exe=root/'gui.exe'
            setup.write_bytes(b'installer fixture');exe.write_bytes(b'gui fixture')
            identity={'version':'0.5.0-beta.3','build_revision':2026111402,'channel':'studio-preview'}
            file=update_manifest.write(root,identity,setup,exe,{})
            self.assertEqual(file.name,'UPDATE-MANIFEST-2026111402.json')
            value=json.loads(file.read_text());windows=value['platforms']['windows_x86_64']
            self.assertEqual(windows['installer_bytes'],len(b'installer fixture'))
            self.assertEqual(windows['installer_sha256'],hashlib.sha256(setup.read_bytes()).hexdigest())
            self.assertEqual(windows['executable_sha256'],hashlib.sha256(exe.read_bytes()).hexdigest())
            with self.assertRaises(FileExistsError):update_manifest.write(root,identity,setup,exe,{})

    def test_helper_bundles_retain_original_notices_and_every_file_fingerprint(self):
        fixture = self.helper_fixture()
        bundles = native_tool_bundles.build(fixture.portable, fixture.root, '0.5.0-beta.3', 'Hotfix2')
        for value in bundles.values():
            archive = fixture.root / value['asset']
            self.assertEqual(value['bytes'], archive.stat().st_size)
            with zipfile.ZipFile(archive) as bundle:
                self.assertEqual(set(bundle.namelist()), set(value['files']))
                for name, digest in value['files'].items():
                    self.assertEqual(hashlib.sha256(bundle.read(name)).hexdigest(), digest)
                self.assertEqual(bundle.read('THIRD-PARTY.json'),
                                 (fixture.notices[0] / 'THIRD-PARTY.json').read_bytes())
                with zipfile.ZipFile(io.BytesIO(bundle.read('licenses.zip'))) as notices:
                    for name, original in fixture.originals.items():
                        self.assertEqual(notices.read('example-1.2.3/' + name), original)
                    provenance = json.loads(notices.read('example-1.2.3/LICENSE-PROVENANCE.json'))
                    for entry in provenance['files']:
                        self.assertEqual(hashlib.sha256(notices.read('example-1.2.3/' + entry['path'])).hexdigest(), entry['sha256'])

    def test_helper_rejects_missing_license_tree_before_emitting_either_bundle(self):
        fixture = self.helper_fixture()
        shutil.rmtree(fixture.notices[1] / 'licenses')
        self.reject_helper(fixture, 'Missing dependency notice file')

    def test_helper_rejects_missing_declared_notice_or_provenance(self):
        for name in ['LICENSE', 'LICENSE-PROVENANCE.json']:
            with self.subTest(name=name):
                fixture = self.helper_fixture()
                (fixture.notices[1] / 'licenses/example-1.2.3' / name).unlink()
                self.reject_helper(fixture, 'Missing dependency notice file')

    def test_helper_rejects_corrupt_original_notice(self):
        fixture = self.helper_fixture()
        (fixture.notices[1] / 'licenses/example-1.2.3/LICENSE').write_bytes(b'corrupted original notice')
        self.reject_helper(fixture, 'hash mismatch')

    def test_helper_rejects_provenance_schema_identity_and_hash_corruption(self):
        for field, value in [('format_version', 2), ('name', 'different'),
                             ('version', '9.9.9'), ('sha256', 'bad'), ('sha256', '0' * 64)]:
            with self.subTest(field=field, value=value):
                fixture = self.helper_fixture()
                path, data = self.provenance(fixture)
                if field == 'sha256':
                    data['files'][0][field] = value
                else:
                    data[field] = value
                path.write_text(json.dumps(data))
                self.reject_helper(fixture)

    def test_helper_rejects_inventory_paths_outside_their_package(self):
        for relative in ['../outside', '/outside', 'C:\\outside',
                         'licenses/example-1.2.3/../../../outside', 'licenses/other/LICENSE']:
            with self.subTest(relative=relative):
                fixture = self.helper_fixture()
                path = fixture.notices[1] / 'THIRD-PARTY.json'
                data = json.loads(path.read_text())
                data[0]['license_files'][0] = relative
                path.write_text(json.dumps(data))
                self.reject_helper(fixture)

    def test_helper_rejects_escaping_provenance_paths_and_entries(self):
        for field in ['notice_provenance', 'path']:
            with self.subTest(field=field):
                fixture = self.helper_fixture()
                if field == 'notice_provenance':
                    path = fixture.notices[1] / 'THIRD-PARTY.json'
                    data = json.loads(path.read_text())
                    data[0][field] = '../outside.json'
                else:
                    path, data = self.provenance(fixture)
                    data['files'][0][field] = '../../../outside'
                path.write_text(json.dumps(data))
                self.reject_helper(fixture)

    def test_helper_requires_matching_unique_inventory_and_provenance_file_sets(self):
        for mutation in ['missing', 'extra', 'duplicate', 'case_collision']:
            with self.subTest(mutation=mutation):
                fixture = self.helper_fixture()
                path, data = self.provenance(fixture)
                if mutation == 'missing':
                    data['files'].pop()
                elif mutation == 'extra':
                    data['files'].append({**data['files'][0], 'path': 'not-in-inventory'})
                elif mutation == 'duplicate':
                    data['files'].append(data['files'][0])
                else:
                    inventory = fixture.notices[1] / 'THIRD-PARTY.json'
                    records = json.loads(inventory.read_text())
                    records[0]['license_files'].append(records[0]['license_files'][0].lower())
                    inventory.write_text(json.dumps(records))
                path.write_text(json.dumps(data))
                self.reject_helper(fixture)

    def test_helper_rejects_unlisted_license_payload_files(self):
        fixture = self.helper_fixture()
        (fixture.notices[1] / 'licenses/example-1.2.3/unlisted.txt').write_bytes(b'not inventoried')
        self.reject_helper(fixture, 'Unlisted dependency notice file')

    def test_helper_permits_explicit_no_separate_notice_files_without_a_license_tree(self):
        fixture = self.helper_fixture()
        notice = fixture.notices[1]
        path = notice / 'THIRD-PARTY.json'
        data = [record for record in json.loads(path.read_text())
                if record['notice_status'] == 'no_separate_notice_files']
        self.assertEqual(len(data), 1)
        path.write_text(json.dumps(data))
        shutil.rmtree(notice / 'licenses')
        bundles = native_tool_bundles.build(fixture.portable, fixture.root, '0.5.0-beta.3', 'Hotfix2')
        with zipfile.ZipFile(fixture.root / bundles['mcp_bridge']['asset']) as bundle:
            self.assertEqual(json.loads(bundle.read('THIRD-PARTY.json')), data)
            with zipfile.ZipFile(io.BytesIO(bundle.read('licenses.zip'))) as licenses:
                self.assertEqual(licenses.namelist(), [])

    def test_helper_rejects_contradictory_or_implicit_missing_notice_status(self):
        for status in ['no_separate_notice_files', None, 'unknown']:
            with self.subTest(status=status):
                fixture = self.helper_fixture()
                path = fixture.notices[1] / 'THIRD-PARTY.json'
                data = json.loads(path.read_text())
                data[0]['notice_status'] = status
                path.write_text(json.dumps(data))
                self.reject_helper(fixture)

    def test_helper_rejects_duplicate_inventory_keys_and_an_empty_inventory(self):
        for data in ['[]', '{}', '[{"name":"example","name":"other"}]']:
            with self.subTest(data=data):
                fixture = self.helper_fixture()
                (fixture.notices[1] / 'THIRD-PARTY.json').write_text(data)
                self.reject_helper(fixture)

    def test_helper_rejects_a_pinned_upstream_hash_conflicting_with_original_bytes(self):
        fixture = self.helper_fixture()
        path, data = self.provenance(fixture)
        entry = data['files'][0]
        entry['sources'] = [{'kind': 'pinned_upstream', 'path': entry['path'], 'sha256': '0' * 64}]
        path.write_text(json.dumps(data))
        self.reject_helper(fixture, 'Pinned upstream notice hash mismatch')

    @unittest.skipIf(os.name == 'nt', 'POSIX symlink fixture; lexical escapes are tested on Windows')
    def test_helper_rejects_a_notice_symlink_escaping_the_package(self):
        fixture = self.helper_fixture()
        path = fixture.notices[1] / 'licenses/example-1.2.3/LICENSE'
        outside = fixture.root / 'outside.txt'
        outside.write_bytes(fixture.originals['LICENSE'])
        path.unlink()
        path.symlink_to(outside)
        self.reject_helper(fixture, 'escapes package directory')
        self.assertEqual(outside.read_bytes(), fixture.originals['LICENSE'])

    @unittest.skipIf(os.name == 'nt', 'POSIX symlink fixture; lexical escapes are tested on Windows')
    def test_helper_rejects_cross_package_notice_links_even_when_original_bytes_match(self):
        fixture = self.helper_fixture()
        crate = fixture.root / 'other-crate'
        crate.mkdir()
        (crate / 'LICENSE').write_bytes(fixture.originals['LICENSE'])
        notice = fixture.notices[1]
        package = {'name': 'other', 'version': '1.0.0', 'source': 'registry+example',
                   'manifest_path': str(crate / 'Cargo.toml'), 'license': 'MIT',
                   'license_file': None, 'repository': 'https://github.com/example/other',
                   'checksum': 'b' * 64}
        records = copy_dependency_licenses([package], notice)
        inventory = notice / 'THIRD-PARTY.json'
        inventory.write_text(json.dumps(json.loads(inventory.read_text()) + records))
        alias = notice / 'licenses/example-1.2.3/LICENSE'
        alias.unlink()
        alias.symlink_to(notice / 'licenses/other-1.0.0/LICENSE')
        self.reject_helper(fixture, 'escapes package directory')

    def test_helper_archives_the_validated_snapshot_if_source_changes_after_preflight(self):
        fixture = self.helper_fixture()
        path = fixture.notices[1] / 'licenses/example-1.2.3/LICENSE'
        original_copy = shutil.copy2

        def change_after_preflight(source, destination):
            path.write_bytes(b'changed after validation')
            return original_copy(source, destination)

        with patch.object(native_tool_bundles.shutil, 'copy2', side_effect=change_after_preflight):
            bundles = native_tool_bundles.build(fixture.portable, fixture.root, '0.5.0-beta.3', 'Hotfix2')
        with zipfile.ZipFile(fixture.root / bundles['mcp_bridge']['asset']) as bundle:
            with zipfile.ZipFile(io.BytesIO(bundle.read('licenses.zip'))) as licenses:
                self.assertEqual(licenses.read('example-1.2.3/LICENSE'), fixture.originals['LICENSE'])
        self.assertNotEqual(path.read_bytes(), fixture.originals['LICENSE'])

if __name__=='__main__':unittest.main()
