"""Linux process lifetime guard for children created by short-lived HTTP threads."""
import os
import select
import signal
import threading
import time


def watch_parent_process(parent):
    descriptor = None
    try:
        descriptor = os.pidfd_open(parent)
    except (AttributeError, OSError):
        pass
    if os.getppid() != parent:
        if descriptor is not None:
            os.close(descriptor)
        raise OSError('Worker parent already exited')

    def wait():
        try:
            if descriptor is None:
                while os.getppid() == parent:
                    time.sleep(0.1)
            else:
                poller = select.poll()
                poller.register(descriptor, select.POLLIN | select.POLLHUP | select.POLLERR)
                poller.poll()
        finally:
            if descriptor is not None:
                os.close(descriptor)
            # Use the supervisor's existing handler to kill and reap its owned
            # descendants; this guard never targets unrelated processes.
            os.kill(os.getpid(), signal.SIGTERM)

    threading.Thread(target=wait, name='worker-parent-lifetime', daemon=True).start()
