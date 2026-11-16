"""One owned native process tree, bounded pipe drains, cancellation and watchdog."""

import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time

from .errors import ProcessingError
from .storage import directory_bytes

LOG_BYTES = 32768


def terminate_and_reap(process):
    if sys.platform.startswith("linux"):
        # Let the subreaper kill and reap helpers even when they created their
        # own process groups. killpg alone cannot reach those native helpers.
        if process.poll() is None:
            process.send_signal(signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            from .process_tree import kill_children
            kill_children(process.pid, process.pid)
    elif os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        process.kill()  # Supervisor teardown closes its private descendant job.
    try:
        process.wait(timeout=10)
    except subprocess.TimeoutExpired as error:
        raise ProcessingError("Owned worker could not be reaped") from error


class ProcessRunner:
    def __init__(self, config):
        self.config = config

    def run(self, command, job, timeout, stdout_sink=None):
        config = self.config
        wrapper = [sys.executable, str(Path(__file__).with_name("child.py")), str(os.getpid()),
                   str(config.memory_bytes), str(config.cpu_seconds), str(config.max_job_disk_bytes),
                   *map(str, command)]
        environment = {key: value for key, value in os.environ.items()
                       if not key.upper().startswith("SLIMEYBEATS_HYBRID_")}
        threads = str(getattr(config, "worker_threads", 4))
        environment.update({"RAYON_NUM_THREADS": threads, "OMP_NUM_THREADS": threads,
                            "TMP": str(job.directory), "TEMP": str(job.directory),
                            "TMPDIR": str(job.directory)})
        log = bytearray()
        pipe_errors = []

        def drain(pipe, sink=None):
            try:
                while True:
                    block = pipe.read(65536)
                    if not block:
                        break
                    if sink is not None:
                        sink(block)
                    else:
                        log.extend(block)
                        if len(log) > LOG_BYTES:
                            del log[:-LOG_BYTES]
            except Exception:
                pipe_errors.append(True)
            finally:
                pipe.close()

        process = subprocess.Popen(wrapper, cwd=job.directory, env=environment, shell=False,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   start_new_session=os.name == "posix",
                                   creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0)
        job.unreaped = True
        threads = [threading.Thread(target=drain, args=(process.stdout, stdout_sink), daemon=True),
                   threading.Thread(target=drain, args=(process.stderr,), daemon=True)]
        for thread in threads:
            thread.start()
        deadline = time.monotonic() + timeout
        failure = None
        try:
            # Continue polling after pipe EOF; a child can close stdout then stall.
            while process.poll() is None:
                if job.cancel.is_set():
                    raise ProcessingError("Processing cancelled")
                if time.monotonic() >= deadline:
                    raise ProcessingError("Processing timed out")
                if pipe_errors:
                    raise ProcessingError("Processing pipe limit or write failure")
                directory_bytes(job.directory, config.max_job_disk_bytes, getattr(config, "max_storage_files", 128))
                job.cancel.wait(0.05)
            if process.returncode != 0 or job.cancel.is_set():
                raise ProcessingError("Native processing failed")
        except BaseException as error:
            failure = error
        finally:
            # Also remove surviving descendants after the native program has exited.
            terminate_and_reap(process)
            for thread in threads:
                thread.join(timeout=10)
            log.clear()
        if any(thread.is_alive() for thread in threads):
            raise ProcessingError("Processing pipes did not close")
        job.unreaped = False
        if failure is not None:
            raise failure
        if pipe_errors:
            raise ProcessingError("Processing pipe limit or write failure")
        directory_bytes(job.directory, config.max_job_disk_bytes, getattr(config, "max_storage_files", 128))
