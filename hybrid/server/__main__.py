"""python -m hybrid.server --cli ABS --data-dir ABS [--host IP --port PORT]."""

import logging
import signal
import sys
import threading

from .config import from_arguments
from .service import Service


def main(argv=None):
    logging.basicConfig(level=logging.INFO, format="%(levelname)s %(message)s")
    try:
        config = from_arguments(argv)
        service = Service(config)
    except (ValueError, OSError):
        logging.error("Hybrid startup refused: check executable, token, origins, limits and owned storage configuration")
        return 2

    def stop(signum, frame):
        # HTTPServer.shutdown must run outside its serve_forever thread.
        threading.Thread(target=service.shutdown, daemon=True).start()

    signal.signal(signal.SIGINT, stop)
    signal.signal(signal.SIGTERM, stop)
    logging.info("Private hybrid preview listening on %s:%s", config.host, service.http.server_port)
    try:
        service.serve_forever()
    except KeyboardInterrupt:
        service.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
