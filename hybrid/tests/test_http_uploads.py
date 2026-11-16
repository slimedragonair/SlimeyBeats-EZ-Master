"""HTTP upload transaction, quota and request-idempotency regressions."""

from concurrent.futures import ThreadPoolExecutor
from datetime import datetime
import unittest

from hybrid.tests.support import running_service


class HttpUploadTests(unittest.TestCase):
    def test_append_exact_offset_and_total_are_transactional(self):
        with running_service(chunk_bytes=4, max_upload_bytes=8) as (service, client):
            status, _, job = client.create(b"abcdefgh")
            self.assertEqual(status, 201)
            self.assertEqual(set(job), {"id", "state", "uploaded_bytes", "total_bytes", "progress", "message", "expires_at", "artifacts"})
            self.assertIsNotNone(datetime.fromisoformat(job["expires_at"].replace("Z", "+00:00")).tzinfo)
            job_id = job["id"]
            route = "/v1/jobs/" + job_id
            self.assertEqual(client.request("PUT", route + "/upload?offset=1", raw=b"abcd")[0], 409)
            self.assertEqual(client.request("PUT", route + "/upload?offset=0", raw=b"abcde")[0], 413)
            self.assertEqual(client.request("GET", route)[2]["uploaded_bytes"], 0)
            self.assertEqual(client.request("PUT", route + "/upload?offset=0", raw=b"abcd")[2]["uploaded_bytes"], 4)
            self.assertEqual(client.request("PUT", route + "/upload?offset=0", raw=b"ZZZZ")[0], 409)
            self.assertEqual(client.request("POST", route + "/run", {})[0], 409)
            self.assertEqual(client.request("PUT", route + "/upload?offset=4", raw=b"efgh")[2]["uploaded_bytes"], 8)
            self.assertEqual(client.request("PUT", route + "/upload?offset=8", raw=b"z")[0], 413)
            self.assertEqual((service.store.jobs[job_id].directory / "upload.wav").read_bytes(), b"abcdefgh")
            for query in ("offset=-1", "offset=0&offset=0", "offset=4&path=x", "offset=NaN", ""):
                self.assertEqual(client.request("PUT", route + "/upload?" + query, raw=b"x")[0], 400)

    def test_concurrent_append_has_one_winner(self):
        with running_service(chunk_bytes=4, max_upload_bytes=8) as (service, client):
            job = client.create(b"abcdefgh")[2]["id"]
            with ThreadPoolExecutor(max_workers=2) as pool:
                results = list(pool.map(lambda block: client.request("PUT", f"/v1/jobs/{job}/upload?offset=0", raw=block)[0],
                                        (b"aaaa", b"bbbb")))
            self.assertEqual(sorted(results), [200, 409])
            self.assertEqual(service.store.snapshot(job)["uploaded_bytes"], 4)

    def test_reservations_and_retained_job_cap(self):
        with running_service(max_jobs=2, max_upload_bytes=8, chunk_bytes=4) as (service, client):
            self.assertEqual(client.create(b"x" * 9)[0], 413)
            jobs = [client.create(b"x" * 8)[2]["id"] for _ in range(2)]
            self.assertEqual(len(set(jobs)), 2)
            self.assertEqual(client.create(b"x")[0], 429)
            self.assertEqual(client.request("DELETE", "/v1/jobs/" + jobs[0])[2], {"deleted": True})
            self.assertEqual(client.create(b"x")[0], 201)

    def test_creation_idempotency_and_conflicting_retry(self):
        with running_service() as (service, client):
            value = {"filename": "x.wav", "bytes": 1, "consent": True}
            headers = {"Idempotency-Key": "owner_retry_0123456789"}
            first = client.request("POST", "/v1/jobs", value, headers=headers)
            second = client.request("POST", "/v1/jobs", value, headers=headers)
            self.assertEqual(first[0], 201)
            self.assertEqual(first[2], second[2])
            self.assertEqual(len(service.store.jobs), 1)
            self.assertEqual(client.request("POST", "/v1/jobs", {**value, "bytes": 2}, headers=headers)[0], 409)
            self.assertEqual(client.request("POST", "/v1/jobs", value, headers={"Idempotency-Key": "../bad"})[0], 400)

    def test_path_traversal_and_arbitrary_artifact_names(self):
        with running_service() as (_, client):
            job = client.create(b"x")[2]["id"]
            for suffix in ("../health", "%2e%2e/health", "artifacts/../input.wav", "artifacts/input.wav",
                           "artifacts/%2fetc%2fpasswd", "artifacts/" + "a" * 32 + "?path=/private"):
                status = client.request("GET", f"/v1/jobs/{job}/" + suffix)[0]
                self.assertIn(status, (400, 404))


if __name__ == "__main__":
    unittest.main()
