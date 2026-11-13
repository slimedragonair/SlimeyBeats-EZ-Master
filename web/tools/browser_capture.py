"""Keep WebKit screenshot-tool CSP diagnostics separate from application errors."""
STYLE_REFUSAL = "Refused to apply a stylesheet because its hash, its nonce, or 'unsafe-inline' does not appear in the style-src directive of the Content Security Policy."


class BrowserDiagnostics:
    def __init__(self, page, browser, result):
        self.page, self.browser, self.result = page, browser, result
        self.capturing = False
        self.capture_messages = []
        result['automation_diagnostics'] = []
        # Playwright 1.57 screenshotter.js synchronizes WebKit animations by
        # appending precisely `body {}`. Observe that insertion without changing
        # the DOM operation or relaxing the production CSP.
        page.add_init_script("""window.screenshotSyncStyles=0;
            new MutationObserver(records=>{
                for(const record of records)for(const node of record.addedNodes)
                    if(record.target===document.head && node.nodeName==='STYLE' && node.textContent==='body {}')window.screenshotSyncStyles++;
            }).observe(document,{childList:true,subtree:true});""")
        page.on('pageerror', lambda error: result['errors'].append(str(error)))
        page.on('console', self.console)

    def console(self, message):
        if message.type != 'error':
            return
        if self.browser == 'webkit' and self.capturing and message.text == STYLE_REFUSAL:
            self.capture_messages.append(message.text)
        else:
            self.result['errors'].append(message.text)

    def capture(self, path, full_page=False):
        before = self.page.evaluate('()=>window.screenshotSyncStyles || 0')
        self.capture_messages = []
        self.capturing = True
        try:
            self.page.screenshot(path=str(path), full_page=full_page, caret='initial')
            self.page.wait_for_timeout(50)
            inserted = self.page.evaluate('()=>window.screenshotSyncStyles || 0') - before
        finally:
            self.capturing = False
        if self.capture_messages:
            if inserted == len(self.capture_messages):
                self.result['automation_diagnostics'].append({
                    'capture': path.name,
                    'reason': 'Playwright 1.57 WebKit animation sync body {} blocked by unchanged CSP',
                    'observed_style_insertions': inserted,
                    'matching_console_diagnostics': len(self.capture_messages),
                })
            else:
                self.result['errors'].extend(self.capture_messages)
