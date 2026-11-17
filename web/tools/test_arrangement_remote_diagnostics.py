"""Only a correlated explicit transport retirement separates a Firefox message."""
from types import SimpleNamespace
import unittest

from arrangement_remote_diagnostics import ArrangementRemoteDiagnostics

MESSAGE = '[JavaScript Error: "Failed to load ‘’. A ServiceWorker intercepted the request and encountered an unexpected error." {file: "http://127.0.0.1:12345/remote-media-worker.js?v=123abc" line: 141}]'


class RemoteDiagnosticsTests(unittest.TestCase):
    def probe(self, evidence):
        probe = object.__new__(ArrangementRemoteDiagnostics)
        probe.browser, probe.result = 'firefox', {'errors': [], 'media_cancellation_diagnostics': []}
        probe.page = SimpleNamespace(evaluate=lambda _: evidence)
        return probe

    def test_a_console_failure_without_an_explicit_retirement_remains_an_error(self):
        probe = self.probe(None); probe.console(SimpleNamespace(type='error', text=MESSAGE))
        self.assertEqual(probe.result['errors'], [MESSAGE])
        self.assertFalse(probe.result['media_cancellation_diagnostics'])

    def test_correlated_message_is_retained_and_other_errors_still_fail(self):
        probe = self.probe({'control': 's-stop', 'elapsed_ms': 20})
        probe.console(SimpleNamespace(type='error', text=MESSAGE))
        probe.console(SimpleNamespace(type='error', text='unexpected application failure'))
        self.assertEqual(probe.result['errors'], ['unexpected application failure'])
        self.assertEqual(probe.result['media_cancellation_diagnostics'][0]['message'], MESSAGE)


if __name__ == '__main__': unittest.main()
