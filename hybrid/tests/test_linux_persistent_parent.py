"""Persistent native workers follow the backend process, not its request thread."""
import ctypes
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
from types import SimpleNamespace
import unittest

from hybrid.server.arrangement_native import spawn_owned
from hybrid.server.processes import terminate_and_reap
from hybrid.tests.support import process_alive, wait_until


@unittest.skipUnless(sys.platform.startswith('linux'), 'Linux persistent process ownership')
class LinuxPersistentParentTests(unittest.TestCase):
    def test_creator_thread_exit_keeps_forwarded_worker_alive(self):
        with tempfile.TemporaryDirectory() as temp:
            processes = []
            config = SimpleNamespace(memory_bytes=512*1024**2, cpu_seconds=30,
                                     max_job_disk_bytes=16*1024**2, worker_threads=1)
            code = "import sys;print('ready',flush=True);[print(line.strip(),flush=True) for line in sys.stdin]"
            def launch():
                child = spawn_owned(config, Path(temp), [sys.executable, '-c', code])
                processes.append(child)
                self.assertEqual(child.stdout.readline(), b'ready\n')
            thread = threading.Thread(target=launch)
            thread.start()
            try:
                thread.join(5)
                self.assertFalse(thread.is_alive())
                child = processes[0]
                time.sleep(.3)
                self.assertIsNone(child.poll(), 'HTTP creator thread exit killed the persistent worker')
                child.stdin.write(b'ping\n')
                child.stdin.flush()
                self.assertEqual(child.stdout.readline(), b'ping\n')
            finally:
                for child in processes:
                    terminate_and_reap(child)
                    child.stdin.close()
                    child.stdout.close()

    def test_parent_process_exit_kills_and_reaps_native_and_detached_helper(self):
        # Adopt the orphan supervisor in this test so waitpid verifies its exit
        # without depending on a container/WSL init process's reaping cadence.
        libc = ctypes.CDLL(None, use_errno=True)
        previous = ctypes.c_int()
        self.assertEqual(libc.prctl(37, ctypes.byref(previous), 0, 0, 0), 0)
        self.assertEqual(libc.prctl(36, 1, 0, 0, 0), 0)
        try:
            with tempfile.TemporaryDirectory() as temp:
                root = Path(temp)
                helper = "import os,pathlib,time;pathlib.Path('helper.pid').write_text(str(os.getpid()));time.sleep(60)"
                native = ("import os,pathlib,subprocess,sys,time;pathlib.Path('native.pid').write_text(str(os.getpid()));"
                          "subprocess.Popen([sys.executable,'-c'," + repr(helper) + "],start_new_session=True);time.sleep(60)")
                child_script = Path(__file__).resolve().parents[1] / 'server' / 'child.py'
                parent_code = ("import os,pathlib,subprocess,sys,time;child=subprocess.Popen([sys.executable," + repr(str(child_script)) +
                    ",str(os.getpid()),str(512*1024**2),'30',str(16*1024**2),'--forward-stdin',sys.executable,'-c'," + repr(native) +
                    "],start_new_session=True);pathlib.Path('supervisor.pid').write_text(str(child.pid));time.sleep(60)")
                parent = subprocess.Popen([sys.executable, '-c', parent_code], cwd=root,
                                          stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                supervisor = None
                try:
                    wait_until(lambda: all((root / name).exists() for name in ('supervisor.pid', 'native.pid', 'helper.pid')))
                    supervisor, native_pid, helper_pid = [int((root / name).read_text()) for name in ('supervisor.pid', 'native.pid', 'helper.pid')]
                    parent.terminate()
                    parent.wait(timeout=5)
                    result = []
                    def reaped():
                        found, status = os.waitpid(supervisor, os.WNOHANG)
                        if found:
                            result.append(status)
                        return bool(found)
                    wait_until(reaped, timeout=5)
                    self.assertTrue(result)
                    self.assertFalse(process_alive(native_pid))
                    self.assertFalse(process_alive(helper_pid), 'Detached helper survived backend exit')
                finally:
                    if parent.poll() is None:
                        parent.terminate()
                        parent.wait(timeout=5)
                    if supervisor is not None and process_alive(supervisor):
                        os.kill(supervisor, 15)
                        os.waitpid(supervisor, 0)
        finally:
            libc.prctl(36, previous.value, 0, 0, 0)


if __name__ == '__main__':
    unittest.main()
