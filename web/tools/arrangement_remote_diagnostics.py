"""Retain Firefox's native cancellation messages with explicit control evidence."""
import re

from browser_capture import BrowserDiagnostics


class ArrangementRemoteDiagnostics(BrowserDiagnostics):
    def __init__(self, page, browser, result):
        result['media_cancellation_diagnostics'] = []
        super().__init__(page, browser, result)

    def console(self, message):
        # Firefox 144 reports an intentional, already-audible progressive
        # response cancellation as a SW error. Do not suppress network/startup
        # failures: only a matching recent A/B, Stop or Pause retirement counts.
        if self.browser == 'firefox' and message.type == 'error' and re.fullmatch(
                r'\[JavaScript Error: "Failed to load .*. A ServiceWorker intercepted the request and encountered an unexpected error\." '
                r'\{file: "http://127\.0\.0\.1:\d+/remote-media-worker\.js\?v=[a-f0-9]+" line: \d+\}\]', message.text):
            evidence = self.page.evaluate('()=>arrangementRemoteAudit.consumeCancellation()')
            if evidence:
                self.result['media_cancellation_diagnostics'].append({
                    'reason': 'Firefox 144 native response cancellation after an explicit transport action',
                    'message': message.text, **evidence})
                return
        super().console(message)
