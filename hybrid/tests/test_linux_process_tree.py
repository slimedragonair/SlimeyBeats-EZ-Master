"""Linux regression: native helpers may create nested groups and sessions."""

import os
from pathlib import Path
import sys
import threading
import time
import unittest

from hybrid.server.errors import ProcessingError
from hybrid.server.jobs import Job
from hybrid.server.processes import ProcessRunner
from hybrid.tests.project_support import project_service
from hybrid.tests.support import process_alive, wait_until


@unittest.skipUnless(sys.platform.startswith("linux"), "Linux subreaper/session cleanup")
class LinuxProcessTreeTests(unittest.TestCase):
    def test_nested_process_group_and_detached_session_are_cancelled_and_reaped(self):
        with project_service() as (service, client):
            job_id = client.create(b"x")[2]["id"]
            job = service.store.jobs[job_id]
            child = "import os,pathlib,time;pathlib.Path('nested.pid').write_text(str(os.getpid()));time.sleep(60)"
            for detached in (False, True):
                with self.subTest(detached=detached):
                    job.cancel.clear()
                    pidfile = job.directory / "nested.pid"
                    pidfile.unlink(missing_ok=True)
                    main = "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c'," + repr(child) + "]," + (
                        "start_new_session=True" if detached else "process_group=0") + ");time.sleep(60)"
                    errors = []
                    def run():
                        try:
                            service.worker.runner.run([sys.executable, "-c", main], job, 10)
                        except ProcessingError as error:
                            errors.append(error)
                    thread = threading.Thread(target=run)
                    thread.start()
                    wait_until(pidfile.exists)
                    pid = int(pidfile.read_text())
                    job.cancel.set()
                    thread.join(15)
                    self.assertFalse(thread.is_alive())
                    self.assertFalse(job.unreaped)
                    self.assertTrue(errors)
                    self.assertFalse(process_alive(pid), "Nested helper or zombie survived cancellation")

    def test_successful_native_exit_also_reaps_detached_helpers(self):
        with project_service() as (service, client):
            job = service.store.jobs[client.create(b"x")[2]["id"]]
            child = "import os,pathlib,time;pathlib.Path('nested.pid').write_text(str(os.getpid()));time.sleep(60)"
            main = "import subprocess,sys,time;subprocess.Popen([sys.executable,'-c'," + repr(child) + "],start_new_session=True);time.sleep(.3)"
            service.worker.runner.run([sys.executable, "-c", main], job, 10)
            pid = int((job.directory / "nested.pid").read_text())
            self.assertFalse(process_alive(pid), "Native success left detached child alive")
