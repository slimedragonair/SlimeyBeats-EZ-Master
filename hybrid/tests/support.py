"""Private temporary service fixture and HTTP client shared by backend tests."""

from contextlib import contextmanager
import http.client
import io
import json
import math
from pathlib import Path
import struct
import sys
import tempfile
import threading
import time
import wave

from hybrid.server.config import Config
from hybrid.server.service import Service

TOKEN = "private-test-token-" + "a" * 40
ORIGIN = "https://owner.example"


def process_alive(pid):
    if sys.platform == "win32":
        import ctypes
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
        kernel.OpenProcess.restype = wintypes.HANDLE
        kernel.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        handle = kernel.OpenProcess(0x1000, False, pid)
        if not handle:
            return False
        code = wintypes.DWORD()
        try:
            return bool(kernel.GetExitCodeProcess(handle, __import__("ctypes").byref(code))) and code.value == 259
        finally:
            kernel.CloseHandle(handle)
    try:
        __import__("os").kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def wait_until(predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(0.02)
    raise AssertionError("Condition did not become true before deadline")


def wav_bytes(seconds=0.1, rate=48000):
    stream = io.BytesIO()
    with wave.open(stream, "wb") as output:
        output.setparams((1, 2, rate, 0, "NONE", "not compressed"))
        output.writeframes(b"".join(struct.pack("<h", int(6000 * math.sin(i * 2 * math.pi * 440 / rate)))
                                    for i in range(int(seconds * rate))))
    return stream.getvalue()


class Client:
    def __init__(self, port):
        self.port = port

    def request(self, method, path, value=None, raw=None, auth=True, origin=None, headers=None):
        supplied = dict(headers or {})
        if auth:
            supplied.setdefault("Authorization", "Bearer " + TOKEN)
        if origin:
            supplied["Origin"] = origin
        if value is not None:
            raw = json.dumps(value).encode()
            supplied.setdefault("Content-Type", "application/json")
        if raw is not None:
            supplied.setdefault("Content-Length", str(len(raw)))
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=40)
        try:
            connection.request(method, path, body=raw, headers=supplied)
            response = connection.getresponse()
            data = response.read()
            result = json.loads(data) if data and response.getheader("Content-Type") == "application/json" else data
            return response.status, dict(response.getheaders()), result
        finally:
            connection.close()

    def create(self, data=None, filename="song.wav", **fields):
        data = wav_bytes() if data is None else data
        return self.request("POST", "/v1/jobs", {"filename": filename, "bytes": len(data), "consent": True,
                                                 "profile": "neutral", "format": "wav24", **fields})

    def upload(self, job, data, chunk_bytes=1024 * 1024):
        result = None
        for offset in range(0, len(data), chunk_bytes):
            result = self.request("PUT", f"/v1/jobs/{job}/upload?offset={offset}", raw=data[offset:offset + chunk_bytes])
            assert result[0] == 200, result
        return result

    def wait(self, job, timeout=90):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            status, headers, snapshot = self.request("GET", "/v1/jobs/" + job)
            assert status == 200, (status, snapshot)
            if snapshot["state"] in ("completed", "failed", "cancelled"):
                return snapshot
            time.sleep(0.05)
        raise AssertionError("Job did not finish within the test deadline")


@contextmanager
def running_service(**options):
    with tempfile.TemporaryDirectory(prefix="hybrid-http-test-") as directory:
        defaults = dict(cli=Path(sys.executable), token=TOKEN, data_dir=Path(directory) / "store", port=0,
                        origins=frozenset({ORIGIN}), min_free_bytes=1, max_job_disk_bytes=16 * 1024 * 1024,
                        cleanup_interval_seconds=0.1)
        defaults.update(options)
        config = Config(**defaults)
        service = Service(config)
        service.start()
        thread = threading.Thread(target=service.http.serve_forever, kwargs={"poll_interval": 0.02})
        thread.start()
        try:
            yield service, Client(service.http.server_port)
        finally:
            service.shutdown()
            thread.join(timeout=5)
            service.close()
