"""A stalled authenticated Range reader cannot keep a deleted/expired project."""

import socket
import unittest

from hybrid.tests.project_support import create_project, project_service
from hybrid.tests.support import TOKEN, wait_until
from hybrid.tests.test_project_http import seed_asset


class ProjectDownloadTests(unittest.TestCase):
    def stalled(self, client, project_id, audio_id):
        connection = socket.socket()
        connection.settimeout(3)
        connection.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
        connection.connect(("127.0.0.1", client.port))
        connection.sendall((f"GET /v2/projects/{project_id}/assets/{audio_id}/preview HTTP/1.1\r\n"
                            f"Host: localhost\r\nAuthorization: Bearer {TOKEN}\r\nRange: bytes=0-\r\n\r\n").encode())
        return connection

    def test_delete_and_expiry_stop_stalled_range_downloads(self):
        for expired in (False, True):
            with self.subTest(expired=expired), project_service(request_timeout_seconds=0.25,
                    v2_retention_seconds=1 if expired else 3600, cleanup_interval_seconds=0.05) as (service, client):
                manifest = create_project(client)
                project, audio_id = seed_asset(service, manifest["id"], b"x" * (8 * 1024**2))
                connection = self.stalled(client, project.id, audio_id)
                try:
                    wait_until(lambda: project.downloads > 0)
                    if not expired:
                        self.assertEqual(client.request("DELETE", "/v2/projects/" + project.id)[2], {"deleted": True})
                    wait_until(lambda: not project.directory.exists(), timeout=4)
                    self.assertEqual(project.downloads, 0)
                finally:
                    connection.close()
