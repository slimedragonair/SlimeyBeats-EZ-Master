"""Owned loopback v2 backend and streaming same-origin proxy for UI acceptance."""
from contextlib import closing
import functools
import http.client
import http.server
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import subprocess
import sys
import threading

from verify_remote_media import ROOT, await_backend, free_port, sha


class RemoteBrowserFixture:
    def __init__(self, args):
        self.args = args
        self.out = args.out.resolve()
        self.site = self.out / "candidate"
        self.data = self.out / "private-store"
        self.backend = self.http = self.log = None
        self.owner_token = secrets.token_urlsafe(40)
        self.requests = []

    def __enter__(self):
        args = self.args
        self.out.mkdir(parents=True, exist_ok=True)
        if self.site.exists():
            raise RuntimeError("Choose a new output directory; existing candidate is preserved")
        shutil.copytree(args.site.resolve(strict=True), self.site)
        native_cli, native_worker = args.cli.resolve(strict=True), args.worker.resolve(strict=True)
        if getattr(args, 'arrangement', False):
            binaries = self.out / 'native-binaries'
            binaries.mkdir()
            shutil.copy2(native_cli, binaries / native_cli.name)
            shutil.copy2(native_worker, binaries / native_worker.name)
            native_cli, native_worker = binaries / native_cli.name, binaries / native_worker.name
        api_port = free_port()
        fixture = self

        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *unused):
                pass

            def send_response(self, code, message=None):
                fixture.requests.append({"method":self.command,"path":self.path.split("?",1)[0],"status":code})
                super().send_response(code, message)

            def proxy(self):
                size = int(self.headers.get("Content-Length", "0"))
                budget = 16 * 1024**2 if re.fullmatch(r'/v2/projects/[a-f0-9]{32}/arrangement', self.path) else 2 * 1024**2
                if size > budget:
                    self.send_error(413)
                    return
                headers = {key:self.headers[key] for key in ("Authorization","Content-Type","Origin","Range",
                           "Idempotency-Key","Access-Control-Request-Method","Access-Control-Request-Headers") if key in self.headers}
                with closing(http.client.HTTPConnection("127.0.0.1",api_port,timeout=180)) as remote:
                    remote.request(self.command,self.path,self.rfile.read(size) if size else None,headers)
                    response=remote.getresponse()
                    self.send_response(response.status)
                    for key in ("Content-Type","Content-Length","Content-Disposition","Content-Range","Accept-Ranges",
                                "Cache-Control","Access-Control-Allow-Origin","Access-Control-Expose-Headers",
                                "Access-Control-Allow-Methods","Access-Control-Allow-Headers","Retry-After"):
                        if response.getheader(key):
                            self.send_header(key,response.getheader(key))
                    self.end_headers()
                    try:
                        while block := response.read1(8192):
                            self.wfile.write(block)
                            self.wfile.flush()
                    except (BrokenPipeError, ConnectionResetError):
                        pass

            def do_GET(self):
                if self.path.startswith(("/v1/","/v2/","/health")):
                    self.proxy()
                else:
                    super().do_GET()

            do_POST=proxy
            do_PUT=proxy
            do_DELETE=proxy
            do_OPTIONS=proxy

        class Server(http.server.ThreadingHTTPServer):
            request_queue_size=128
            daemon_threads=True

        self.http=Server(("127.0.0.1",0),functools.partial(Handler,directory=str(self.site)))
        self.origin="http://127.0.0.1:"+str(self.http.server_port)
        policy=self.site / "hybrid/endpoint-policy.js"
        original=sha(policy)
        source=policy.read_text(encoding="utf-8")
        source,count=re.subn(r'export const PUBLIC_API_ORIGIN\s*=\s*[^;]+;',
                             'export const PUBLIC_API_ORIGIN = '+json.dumps(self.origin)+';',source)
        if count != 1:
            raise RuntimeError("Cannot identify the copied candidate's default API origin")
        policy.write_text(source,encoding="utf-8")
        worker=self.site / "remote-media-worker.js"
        worker_original=sha(worker)
        worker_text=worker.read_text(encoding="utf-8")
        worker_text,worker_pins=re.subn(r'((?:export\s+)?const PUBLIC_API_ORIGIN\s*=\s*)[^;]+;',
            lambda match:match[1]+json.dumps(self.origin)+';',worker_text)
        if worker_pins not in (0,1):
            raise RuntimeError("Unexpected inlined media-worker API origin count")
        if worker_pins:
            worker.write_text(worker_text,encoding="utf-8")
        self.provenance={"source":str(args.site.resolve()),"origin_adapter":"copied PUBLIC_API_ORIGIN only; same-origin streaming HTTP proxy",
            "policy_original_sha256":original,"policy_fixture_sha256":sha(policy),
            "worker_original_sha256":worker_original,"worker_fixture_sha256":sha(worker),
            "worker_inlined_origin_overrides":worker_pins,"native_sha256":sha(native_worker)}
        self.thread=threading.Thread(target=self.http.serve_forever,daemon=True)
        self.thread.start()
        self.log=(self.out / "service.log").open("wb")
        environment=dict(os.environ,SLIMEYBEATS_HYBRID_TOKEN=self.owner_token,SLIMEYBEATS_HYBRID_PUBLIC="1",
                         SLIMEYBEATS_HYBRID_ORIGINS=self.origin,PYTHONDONTWRITEBYTECODE="1")
        command=[sys.executable,"-m","hybrid.server","--cli",str(native_cli),
            "--studio-cli",str(native_worker),"--ffmpeg",str(args.ffmpeg.resolve(strict=True)),
            "--data-dir",str(self.data),"--port",str(api_port),"--min-free-bytes","1",
            "--v2-max-upload-bytes",str(32*1024**2),"--v2-max-decoded-bytes",str(64*1024**2),
            "--v2-max-project-disk-bytes",str(256*1024**2)]
        if getattr(args, 'arrangement', False):
            command.append('--enable-arrangement')
        self.backend=subprocess.Popen(command,cwd=ROOT,env=environment,
            stdin=subprocess.DEVNULL,stdout=self.log,stderr=self.log)
        try:
            await_backend(self.backend,api_port)
        except BaseException:
            self.__exit__(None,None,None)
            raise
        return self

    def __exit__(self, *unused):
        if self.backend and self.backend.poll() is None:
            self.backend.terminate()
            try:
                self.backend.wait(timeout=45)
            except subprocess.TimeoutExpired:
                self.backend.kill()
                self.backend.wait(timeout=10)
        if self.http:
            self.http.shutdown()
            self.http.server_close()
            self.thread.join(timeout=5)
        if self.log:
            self.log.close()

    def redact(self, value):
        return value.replace(self.owner_token,"[credential removed]")
