"""Same-origin test reverse proxy; never suitable as a public web server."""
import functools
from contextlib import closing
import http.client
import http.server
import threading


def start_site(site, api_port):
    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def proxy(self):
            count = int(self.headers.get('Content-Length', '0'))
            if count > 2 * 1024 * 1024:
                self.send_error(413)
                return
            headers = {key: self.headers[key] for key in
                       ('Authorization', 'Content-Type', 'Origin', 'Idempotency-Key')
                       if key in self.headers}
            with closing(http.client.HTTPConnection('127.0.0.1', api_port, timeout=60)) as remote:
                remote.request(self.command, self.path, self.rfile.read(count), headers)
                response = remote.getresponse()
                self.send_response(response.status)
                for key in ('Content-Type', 'Content-Length', 'Content-Disposition', 'Cache-Control'):
                    if response.getheader(key):
                        self.send_header(key, response.getheader(key))
                self.end_headers()
                while chunk := response.read(64 * 1024):
                    self.wfile.write(chunk)

        def do_GET(self):
            if self.path.startswith('/v1/') or self.path == '/health':
                self.proxy()
            else:
                super().do_GET()

        do_POST = proxy
        do_PUT = proxy
        do_DELETE = proxy

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0),
        functools.partial(Handler, directory=str(site)))
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server
