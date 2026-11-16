"""Trusted subprocess supervisor: limits apply before native code or codecs start.

This file is executed by the worker, never exposed as an HTTP operation. On
Windows its non-inheritable job handle kills descendants if the supervisor dies.
Linux uses an owned session and subreaper for nested codec/model process groups.
"""

import ctypes
import os
import signal
import subprocess
import sys


def windows_job(memory_bytes):
    from ctypes import wintypes

    class Basic(ctypes.Structure):
        _fields_ = [("process_time", ctypes.c_longlong), ("job_time", ctypes.c_longlong),
                    ("flags", wintypes.DWORD), ("min_ws", ctypes.c_size_t),
                    ("max_ws", ctypes.c_size_t), ("active", wintypes.DWORD),
                    ("affinity", ctypes.c_size_t), ("priority", wintypes.DWORD),
                    ("scheduling", wintypes.DWORD)]

    class Io(ctypes.Structure):
        _fields_ = [(key, ctypes.c_ulonglong) for key in
                    ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]

    class Extended(ctypes.Structure):
        _fields_ = [("basic", Basic), ("io", Io), ("process_memory", ctypes.c_size_t),
                    ("job_memory", ctypes.c_size_t), ("peak_process", ctypes.c_size_t),
                    ("peak_job", ctypes.c_size_t)]

    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.CreateJobObjectW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR]
    kernel.CreateJobObjectW.restype = wintypes.HANDLE
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.SetInformationJobObject.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD]
    kernel.AssignProcessToJobObject.argtypes = [wintypes.HANDLE, wintypes.HANDLE]
    handle = kernel.CreateJobObjectW(None, None)
    if not handle:
        raise OSError("Cannot create private worker job")
    limits = Extended()
    limits.basic.flags = 0x2000 | 0x200  # KILL_ON_JOB_CLOSE | JOB_MEMORY
    limits.job_memory = memory_bytes
    if (not kernel.SetInformationJobObject(handle, 9, ctypes.byref(limits), ctypes.sizeof(limits))
            or not kernel.AssignProcessToJobObject(handle, kernel.GetCurrentProcess())):
        raise OSError("Cannot apply private worker job limits")
    return handle  # Keep alive until OS process teardown; never inherited by the CLI.


def main():
    parent, memory_bytes, cpu_seconds, file_bytes = map(int, sys.argv[1:5])
    command = sys.argv[5:]
    if not command:
        return 125
    job_handle = None
    if os.name == "nt":
        job_handle = windows_job(memory_bytes)
    else:
        import resource
        for kind, value in ((resource.RLIMIT_AS, memory_bytes), (resource.RLIMIT_CPU, cpu_seconds),
                            (resource.RLIMIT_FSIZE, file_bytes), (resource.RLIMIT_CORE, 0),
                            (resource.RLIMIT_NOFILE, 64)):
            resource.setrlimit(kind, (value, value))

        def kill_group(signum, frame):
            if sys.platform.startswith("linux"):
                from process_tree import kill_children
                kill_children(os.getsid(0), os.getpid(), exclude=(os.getpid(),))
            else:
                os.killpg(os.getpgrp(), signal.SIGKILL)

        signal.signal(signal.SIGTERM, kill_group)
        if sys.platform.startswith("linux"):
            libc = ctypes.CDLL(None, use_errno=True)
            if libc.prctl(36, 1, 0, 0, 0) != 0:  # PR_SET_CHILD_SUBREAPER
                raise OSError("Cannot own orphaned descendants")
            if libc.prctl(1, signal.SIGTERM, 0, 0, 0) != 0:  # PR_SET_PDEATHSIG
                raise OSError("Cannot protect orphan worker")
            if os.getppid() != parent:
                return 125
    result = subprocess.run(command, shell=False, stdin=subprocess.DEVNULL)
    if sys.platform.startswith("linux"):
        from process_tree import kill_children
        kill_children(os.getsid(0), os.getpid(), exclude=(os.getpid(),))
        while True:
            try:
                os.waitpid(-1, 0)
            except ChildProcessError:
                break
    # job_handle intentionally remains open: closing it would kill this supervisor.
    return result.returncode


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception:
        # Tracebacks can contain local paths. The worker needs only a failure code.
        sys.exit(125)
