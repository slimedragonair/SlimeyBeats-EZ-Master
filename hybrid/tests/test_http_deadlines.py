"""Actual trickling TCP peers must release bounded HTTP slots before auth."""

from http.client import RemoteDisconnected
import socket
import threading
import time
import unittest

from hybrid.tests.support import TOKEN, running_service, wait_until


class HttpDeadlineTests(unittest.TestCase):
    def test_trickling_headers_and_bodies_have_absolute_deadlines(self):
        for phase in ("headers", "body"):
            with self.subTest(phase=phase), running_service(max_connections=2, request_timeout_seconds=0.15) as (service, client):
                connections, threads = [], []
                stop = threading.Event()

                def trickle(connection):
                    try:
                        while not stop.wait(0.02):
                            connection.sendall(b"x")
                    except OSError:
                        pass

                def health_status():
                    try:
                        return client.request("GET", "/health", auth=False)[0]
                    except (ConnectionError, RemoteDisconnected):
                        # An overload close can reset unread request bytes on Windows.
                        return None

                def health_ready():
                    status = health_status()
                    self.assertIn(status, (200, 503, None))
                    return status == 200

                try:
                    for _ in range(2):
                        connection = socket.create_connection(("127.0.0.1", client.port), timeout=1)
                        connections.append(connection)
                        header = (b"GET /v1/capabilities HTTP/1.1\r\nHost: localhost\r\nX-Slow: " if phase == "headers"
                                  else ("POST /v1/jobs HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer " + TOKEN +
                                        "\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n").encode())
                        connection.sendall(header)
                        thread = threading.Thread(target=trickle, args=(connection,))
                        thread.start()
                        threads.append(thread)
                    wait_until(lambda: service.http.slots._value == 0, timeout=1)
                    self.assertIn(health_status(), (503, None))
                    started = time.monotonic()
                    wait_until(health_ready, timeout=0.6)
                    self.assertLess(time.monotonic() - started, 0.6)
                    for connection in connections:
                        try:
                            self.assertEqual(connection.recv(1), b"")
                        except (ConnectionResetError, ConnectionAbortedError):
                            pass  # Windows may reset a closed socket with unread incoming bytes.
                finally:
                    stop.set()
                    for connection in connections:
                        connection.close()
                    for thread in threads:
                        thread.join(timeout=1)
