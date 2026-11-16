"""Linux owned-session cleanup, including descendants in nested process groups."""

import os
from pathlib import Path
import signal


def _members(session_id, root):
    records = {}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            text = (entry / "stat").read_text()
            fields = text[text.rfind(")") + 2:].split()
            records[int(entry.name)] = (int(fields[1]), int(fields[3]))
        except (FileNotFoundError, ProcessLookupError, PermissionError, ValueError, IndexError):
            continue
    owned = {pid for pid, (_, sid) in records.items() if sid == session_id}
    owned.add(root)
    while True:
        children = {pid for pid, (parent, _) in records.items() if parent in owned}
        if children <= owned:
            return owned
        owned |= children


def kill_children(session_id, root, exclude=()):
    # The supervisor is a Linux subreaper, so detached grandchildren stay owned
    # by it. Freeze observed descendants before the final scan to prevent forks
    # while killing nested process groups. pidfds pin identities where available.
    excluded = set(exclude)
    handles = {}
    try:
        for _ in range(3):
            for pid in _members(session_id, root) - excluded - handles.keys():
                try:
                    fd = os.pidfd_open(pid) if hasattr(os, "pidfd_open") else None
                    handles[pid] = fd
                    _signal(pid, fd, signal.SIGSTOP)
                except ProcessLookupError:
                    continue
        for pid, fd in handles.items():
            _signal(pid, fd, signal.SIGKILL)
    finally:
        for fd in handles.values():
            if fd is not None:
                os.close(fd)


def _signal(pid, descriptor, sig):
    try:
        if descriptor is not None and hasattr(signal, "pidfd_send_signal"):
            signal.pidfd_send_signal(descriptor, sig)
        else:
            os.kill(pid, sig)
    except ProcessLookupError:
        pass
