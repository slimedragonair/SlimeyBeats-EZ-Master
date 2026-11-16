"""Service lifetime: exclusive store, one worker, expiry sweeper and owned cleanup."""

import logging
import threading

from .errors import ProcessingError
from .http_server import HybridHTTPServer
from .jobs import JobStore
from .storage import Storage
from .worker import Worker


class Service:
    def __init__(self, config):
        self.config = config
        self.storage = Storage(config)
        self.store = JobStore(config, self.storage)
        try:
            self.http = HybridHTTPServer(config, self.store)
        except BaseException:
            self.storage.close()
            raise
        self.worker = Worker(config, self.store)
        self.sweep_stop = threading.Event()
        self.sweeper = threading.Thread(target=self._sweep, name="hybrid-expiry", daemon=False)
        self.closed = False

    def start(self):
        self.worker.start()
        self.sweeper.start()

    def _sweep(self):
        while not self.sweep_stop.wait(self.config.cleanup_interval_seconds):
            try:
                self.store.expire()
            except Exception:
                logging.getLogger("slimeybeats.hybrid").error("Hybrid expiry cleanup requires retry")

    def close(self):
        if self.closed:
            return
        self.closed = True
        self.sweep_stop.set()
        self.store.stop()
        if self.worker.ident is not None:
            self.worker.join(timeout=35)
        if self.sweeper.ident is not None:
            self.sweeper.join(timeout=35)
        self.http.server_close()
        if (self.worker.is_alive() or any(job.unreaped for job in self.store.jobs.values())
                or any(project.command and project.command.unreaped for project in self.store.projects.projects.values())):
            raise ProcessingError("Owned worker is still stopping; storage remains locked")
        try:
            for job_id in list(self.store.jobs):
                self.store.delete(job_id, expired=True)
            for project_id in list(self.store.projects.projects):
                self.store.projects.delete(project_id, expired=True)
        finally:
            self.storage.close()

    def serve_forever(self):
        self.start()
        try:
            self.http.serve_forever(poll_interval=0.1)
        finally:
            self.close()

    def shutdown(self):
        self.http.shutdown()
