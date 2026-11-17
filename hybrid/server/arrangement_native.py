"""One bounded persistent native arrangement actor; paths never cross public RPC."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time

from .errors import ProcessingError
from .processes import terminate_and_reap
from .studio_driver import unique, reject_constant

MAX_REQUEST_BYTES = 17 * 1024 * 1024
MAX_RESPONSE_BYTES = 34 * 1024 * 1024


def spawn_owned(config, directory, command):
    wrapper = [sys.executable, str(Path(__file__).with_name('child.py')), str(os.getpid()),
               str(config.memory_bytes), str(config.cpu_seconds), str(config.max_job_disk_bytes),
               '--forward-stdin', *map(str, command)]
    environment = {key: value for key, value in os.environ.items()
                   if not key.upper().startswith('SLIMEYBEATS_HYBRID_')}
    environment.update({'TMP': str(directory), 'TEMP': str(directory), 'TMPDIR': str(directory),
                        'RAYON_NUM_THREADS': str(config.worker_threads)})
    return subprocess.Popen(wrapper, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.DEVNULL, env=environment, start_new_session=os.name != 'nt')


class ArrangementNative:
    def __init__(self, config, directory):
        self.config, self.directory = config, Path(directory).resolve(strict=True)
        self.lock = threading.RLock()
        self.sequence, self.closed = 0, False
        self.last_error = None
        self.responses = queue.Queue(maxsize=1)
        self.process = spawn_owned(config, self.directory, [config.studio_cli, '--arrangement-server', self.directory,
                                                           '--threads', config.worker_threads])
        self.reader = threading.Thread(target=self._read, name='arrangement-native-responses', daemon=True)
        self.reader.start()

    def _read(self):
        try:
            while not self.closed:
                line = self.process.stdout.readline(MAX_RESPONSE_BYTES + 1)
                if not line or len(line) > MAX_RESPONSE_BYTES or not line.endswith(b'\n'):
                    raise ProcessingError('Native arrangement response framing failed')
                value = json.loads(line, object_pairs_hook=unique, parse_constant=reject_constant)
                self.responses.put(value)
        except (OSError, ValueError, ProcessingError):
            try:
                self.responses.put_nowait(None)
            except queue.Full:
                pass

    def call(self, request, *, timeout=None, cancel=None, safety_check=None):
        with self.lock:
            if self.closed or self.process.poll() is not None:
                raise ProcessingError('Native arrangement worker is unavailable')
            self.sequence += 1
            wire = json.dumps({'id': self.sequence, 'request': request}, allow_nan=False,
                              ensure_ascii=False, separators=(',', ':')).encode() + b'\n'
            if len(wire) > MAX_REQUEST_BYTES:
                raise ProcessingError('Arrangement request exceeds the metadata budget')
            deadline = time.monotonic() + (timeout or self.config.worker_timeout_seconds)
            write_done, write_error = threading.Event(), []

            def write():
                try:
                    self.process.stdin.write(wire)
                    self.process.stdin.flush()
                except (OSError, ValueError) as error:
                    write_error.append(error)
                finally:
                    write_done.set()

            writer = threading.Thread(target=write, name='arrangement-native-request', daemon=True)
            writer.start()
            try:
                while not write_done.wait(0.025):
                    if safety_check:
                        safety_check()
                    if time.monotonic() >= deadline or cancel and cancel.is_set():
                        raise ProcessingError('Native arrangement request cancelled or timed out')
                if write_error:
                    raise ProcessingError('Native arrangement input failed')
                while True:
                    if safety_check:
                        safety_check()
                    if time.monotonic() >= deadline or cancel and cancel.is_set():
                        raise ProcessingError('Native arrangement request cancelled or timed out')
                    try:
                        value = self.responses.get(timeout=0.025)
                        break
                    except queue.Empty:
                        continue
                if not isinstance(value, dict) or value.get('id') != self.sequence:
                    raise ProcessingError('Native arrangement response identity failed')
                if 'error' in value:
                    self.last_error = str(value['error'])[:2048]
                    # Native errors can contain private file paths. Public HTTP
                    # uses a fixed code and keeps the prior workspace selected.
                    raise ArrangementCommandError('Native arrangement command rejected')
                if set(value) != {'id', 'value'}:
                    raise ProcessingError('Native arrangement response shape failed')
                return value['value']
            except ArrangementCommandError:
                raise
            except BaseException:
                self.close()
                raise

    def close(self):
        with self.lock:
            if self.closed:
                return
            self.closed = True
            terminate_and_reap(self.process)
            for pipe in (self.process.stdin, self.process.stdout):
                if pipe:
                    pipe.close()
            if self.reader is not threading.current_thread():
                self.reader.join(timeout=2)


class ArrangementCommandError(ProcessingError):
    """Rejected command; the persistent actor and previous session remain valid."""
