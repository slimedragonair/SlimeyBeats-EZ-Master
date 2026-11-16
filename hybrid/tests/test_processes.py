"""Real process trees qualify timeout, cancellation, disk caps and env isolation."""

import json
import os
import sys
import time
import unittest
from unittest.mock import patch

from hybrid.server.errors import ProcessingError
from hybrid.tests.support import TOKEN, process_alive, running_service


class ProcessTests(unittest.TestCase):
    def test_timeout_survives_closed_stdout_and_reaps_child(self):
        with running_service() as (service, client):
            job_id = client.create(b"x")[2]["id"]
            job = service.store.jobs[job_id]
            code = ("import os,pathlib,time; pathlib.Path('native.pid').write_text(str(os.getpid())); "
                    "os.close(1); os.close(2); time.sleep(30)")
            started = time.monotonic()
            with self.assertRaises(ProcessingError):
                service.worker.runner.run([sys.executable, "-c", code], job, timeout=0.5)
            self.assertLess(time.monotonic() - started, 5)
            self.assertFalse(job.unreaped)
            self.assertFalse(process_alive(int((job.directory / "native.pid").read_text())))

    def test_large_output_is_drained_and_token_environment_removed(self):
        with running_service() as (service, client):
            job = service.store.jobs[client.create(b"x")[2]["id"]]
            code = ("import os,sys; assert not any(k.upper().startswith('SLIMEYBEATS_HYBRID_') for k in os.environ); "
                    "sys.stdout.buffer.write(b'x'*2097152); sys.stderr.buffer.write(b'y'*2097152)")
            with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN,
                                         "SLIMEYBEATS_HYBRID_PUBLIC": "1",
                                         "SLIMEYBEATS_HYBRID_ORIGINS": "https://private.example"}):
                service.worker.runner.run([sys.executable, "-c", code], job, timeout=10)
            self.assertFalse(job.unreaped)

    def test_output_disk_limit_stops_and_reaps_owned_tree(self):
        with running_service(max_job_disk_bytes=1024 * 1024) as (service, client):
            job = service.store.jobs[client.create(b"x")[2]["id"]]
            code = ("import os,pathlib,time; pathlib.Path('native.pid').write_text(str(os.getpid())); "
                    "pathlib.Path('oversized.out').write_bytes(b'x'*2097152); time.sleep(30)")
            with self.assertRaises(ProcessingError):
                service.worker.runner.run([sys.executable, "-c", code], job, timeout=5)
            self.assertFalse(process_alive(int((job.directory / "native.pid").read_text())))
            self.assertFalse(job.unreaped)

    @unittest.skipUnless(os.name == "posix", "Unix resource limits require a Unix runner")
    def test_unix_children_inherit_memory_cpu_and_file_limits(self):
        with running_service() as (service, client):
            job = service.store.jobs[client.create(b"x")[2]["id"]]
            captured = bytearray()
            code = ("import json,resource; print(json.dumps([resource.getrlimit(k) for k in "
                    "(resource.RLIMIT_AS,resource.RLIMIT_CPU,resource.RLIMIT_FSIZE)]))")
            service.worker.runner.run([sys.executable, "-c", code], job, 5, captured.extend)
            self.assertEqual(json.loads(captured), [[service.config.memory_bytes] * 2,
                                                   [service.config.cpu_seconds] * 2,
                                                   [service.config.max_job_disk_bytes] * 2])
