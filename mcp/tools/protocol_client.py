"""Small stdlib JSON-RPC harness for testing the real stdio companion."""
import json
import queue
import subprocess
import threading
import time


class Client:
    def __init__(self, command):
        self.process = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.PIPE, text=True, encoding="utf-8")
        self.responses = queue.Queue()
        self.stderr = []
        self.sequence = 0
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.errors = threading.Thread(target=self._errors, daemon=True)
        self.reader.start()
        self.errors.start()
        self.initialized = self.request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                    "clientInfo": {"name": "SlimeyBeats-validation", "version": "1"}})
        self.notify("notifications/initialized", {})

    def _read(self):
        for line in self.process.stdout:
            try:
                self.responses.put(json.loads(line))
            except ValueError as error:
                self.responses.put(error)
        self.responses.put(EOFError("Companion stdout closed"))

    def _errors(self):
        for line in self.process.stderr:
            self.stderr.append(line)

    def notify(self, method, params):
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "method": method, "params": params}) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.sequence += 1
        self.process.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.sequence,
                                             "method": method, "params": params}) + "\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            response = self.responses.get(timeout=max(0.1, deadline - time.monotonic()))
            if isinstance(response, Exception):
                raise response
            if response.get("id") == self.sequence:
                return response
        raise TimeoutError(method)

    def call(self, name, arguments=None, error=False):
        response = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        result = response.get("result", {})
        failed = "error" in response or result.get("isError", False)
        assert failed == error, (name, response)
        if failed:
            return response
        return result["structuredContent"]

    def finish(self, submitted):
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            job = self.call("get_job_status", {"job_id": submitted["job_id"]})
            if job["status"] in ("completed", "failed", "cancelled"):
                return job
            time.sleep(0.05)
        raise TimeoutError(submitted)

    def run(self, name, arguments=None):
        job = self.finish(self.call(name, arguments))
        assert job["status"] == "completed", (name, job)
        return job["result"]

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
            raise
        self.reader.join(timeout=2)
        self.errors.join(timeout=2)
        assert self.process.returncode == 0, "".join(self.stderr)
