"""Download failures retain evidence without retrying or changing acceptance."""
import ast
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock


def save_function(ready):
    # These checks also run before Playwright is installed in browser CI.
    source = Path(__file__).with_name('region_edit_ui_checks.py')
    tree = ast.parse(source.read_text(encoding='utf-8'))
    function = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'save')
    namespace = {'json': json, 'ready': ready}
    exec(compile(ast.Module(body=[function], type_ignores=[]), str(source), 'exec'), namespace)
    return namespace['save']


class DownloadPage:
    def __init__(self, *, timeout=False, failure=None):
        self.timeout, self.failure = timeout, failure
        self.clicks, self.timeouts, self.armed = 0, [], False
        self.payload = b'unchanged exported PCM fixture'
        self.state = {'selector': '#s-export', 'events': [{'kind': 'button_click', 'trusted': True}],
                      'calls': [{'action': 'export'}], 'replies': [], 'errors': [], 'worker_pending': [7]}

    def evaluate(self, expression, *arguments):
        return None if arguments else self.state

    def locator(self, selector):
        def click():
            if not self.armed:
                raise AssertionError('Download observer must be armed before the click')
            self.clicks += 1
        return SimpleNamespace(click=click)

    def expect_download(self, *, timeout):
        self.timeouts.append(timeout)
        page = self
        class Pending:
            def __enter__(self):
                page.armed = True
                return self

            def __exit__(self, *unused):
                page.armed = False
                if page.timeout:
                    raise TimeoutError('native download event missing')

            @property
            def value(self):
                return SimpleNamespace(failure=lambda: page.failure,
                    save_as=lambda path: Path(path).write_bytes(page.payload))
        return Pending()


class RegionDownloadTests(unittest.TestCase):
    def test_success_saves_the_actual_download_bytes_once(self):
        ready = Mock()
        page = DownloadPage()
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'preview.wav'
            self.assertEqual(save_function(ready)(page, '#s-export', target), page.payload)
            self.assertEqual(target.read_bytes(), page.payload)
            self.assertFalse(target.with_name(target.name + '.download.json').exists())
        self.assertEqual(page.clicks, 1)
        self.assertEqual(page.timeouts, [60000])
        ready.assert_called_once_with(page)

    def test_timeout_retains_trusted_click_and_pending_worker_without_retry(self):
        ready = Mock()
        page = DownloadPage(timeout=True)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'restored.wav'
            with self.assertRaises(AssertionError) as caught:
                save_function(ready)(page, '#s-export', target)
            self.assertIsInstance(caught.exception.__cause__, TimeoutError)
            self.assertIn('restored.wav', str(caught.exception))
            self.assertEqual(json.loads(target.with_name(target.name + '.download.json').read_text()), page.state)
            self.assertFalse(target.exists())
        self.assertEqual(page.clicks, 1)
        self.assertEqual(page.timeouts, [60000])
        ready.assert_not_called()

    def test_failed_native_download_is_not_saved_or_accepted(self):
        ready = Mock()
        page = DownloadPage(failure='download cancelled by browser')
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'cancelled.wav'
            with self.assertRaises(AssertionError) as caught:
                save_function(ready)(page, '#s-export', target)
            self.assertIn('download cancelled by browser', str(caught.exception.__cause__))
            self.assertFalse(target.exists())
        self.assertEqual(page.clicks, 1)
        ready.assert_not_called()

    def test_worker_error_is_preserved_in_timeout_evidence(self):
        page = DownloadPage(timeout=True)
        page.state['errors'] = [{'action': 'export', 'error': 'fixture worker failed'}]
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'failed.wav'
            with self.assertRaises(AssertionError) as caught:
                save_function(Mock())(page, '#s-export', target)
            self.assertIn('fixture worker failed', str(caught.exception))
            self.assertEqual(json.loads(target.with_name(target.name + '.download.json').read_text())['errors'], page.state['errors'])


if __name__ == '__main__':
    unittest.main()
