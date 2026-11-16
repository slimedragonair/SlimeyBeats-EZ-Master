"""Real HTTP authentication, CORS, schema rejection and privacy regressions."""

import io
import json
import logging
import unittest

from hybrid.tests.support import ORIGIN, TOKEN, running_service


class HttpAccessTests(unittest.TestCase):
    def test_health_and_capability_contract(self):
        with running_service() as (_, client):
            status, headers, value = client.request("GET", "/health", auth=False)
            self.assertEqual((status, value), (200, {"status": "up"}))
            status, headers, value = client.request("GET", "/v1/capabilities", origin=ORIGIN)
            self.assertEqual(status, 200)
            self.assertEqual(set(value), {"api_version", "operations", "formats", "profiles", "limits", "privacy", "features"})
            self.assertEqual(value["api_version"], 1)
            self.assertEqual(value["formats"], ["wav24", "mp3", "wav32f"])
            self.assertEqual(value["features"], {"profile_blends": True, "reference_analysis": True,
                                                  "safety": True, "editable_master": True})
            self.assertEqual(value["operations"], ["master"])
            self.assertEqual(len(value["profiles"]), 16)
            self.assertTrue(all(set(profile) == {"id", "label"} for profile in value["profiles"]))
            self.assertEqual(value["limits"], {"max_upload_bytes": 536870912, "chunk_bytes": 1048576,
                                                "max_jobs": 4, "retention_seconds": 3600,
                                                "max_audio_seconds": 1800, "max_decoded_bytes": 536870912})
            self.assertEqual(value["privacy"], {"temporary_storage": True, "retention_seconds": 3600})
            self.assertEqual(headers["Access-Control-Allow-Origin"], ORIGIN)
            self.assertNotIn("Access-Control-Allow-Credentials", headers)

    def test_every_api_method_and_unknown_api_requires_bearer(self):
        with running_service() as (_, client):
            routes = [("GET", "/v1/capabilities"), ("POST", "/v1/jobs"),
                      ("GET", "/v1/jobs/" + "a" * 32), ("PUT", "/v1/jobs/" + "a" * 32 + "/upload?offset=0"),
                      ("POST", "/v1/jobs/" + "a" * 32 + "/run"), ("DELETE", "/v1/jobs/" + "a" * 32),
                      ("GET", "/v1/jobs/" + "a" * 32 + "/artifacts/" + "b" * 32), ("GET", "/v1/unknown")]
            for method, path in routes:
                with self.subTest(method=method, path=path):
                    self.assertEqual(client.request(method, path, auth=False)[0], 401)
            for token in ("Bearer wrong", "Basic " + TOKEN, "Bearer " + TOKEN[:-1], "Bearer " + TOKEN + "x"):
                self.assertEqual(client.request("GET", "/v1/capabilities", auth=False, headers={"Authorization": token})[0], 401)

    def test_exact_origin_and_preflight(self):
        with running_service() as (service, client):
            for origin in ("https://evil.example", ORIGIN + ".evil", "null", "http://owner.example", ORIGIN + "/"):
                with self.subTest(origin=origin):
                    response = client.request("GET", "/v1/capabilities", origin=origin)
                    self.assertEqual(response[0], 403)
                    self.assertNotIn("Access-Control-Allow-Origin", response[1])
                    self.assertEqual(client.request("POST", "/v1/jobs", {"filename": "a.wav", "bytes": 1, "consent": True}, origin=origin)[0], 403)
                    self.assertEqual(client.request("OPTIONS", "/v1/jobs", auth=False, origin=origin,
                                                   headers={"Access-Control-Request-Method": "POST"})[0], 403)
            self.assertEqual(len(service.store.jobs), 0)
            status, headers, _ = client.request("OPTIONS", "/v1/jobs", auth=False, origin=ORIGIN,
                                                headers={"Access-Control-Request-Method": "POST",
                                                         "Access-Control-Request-Headers": "authorization, content-type, idempotency-key"})
            self.assertEqual(status, 204)
            self.assertIn("Idempotency-Key", headers["Access-Control-Allow-Headers"])
            self.assertNotIn("Access-Control-Allow-Credentials", headers)
            self.assertEqual(client.request("OPTIONS", "/v1/jobs", auth=False,
                                           headers={"Access-Control-Request-Method": "POST"})[0], 403)
            self.assertEqual(client.request("OPTIONS", "/v1/jobs", auth=False, origin=ORIGIN,
                                           headers={"Access-Control-Request-Method": "POST", "Access-Control-Request-Headers": "X-Private"})[0], 403)

    def test_untrusted_fields_and_private_paths_are_rejected(self):
        with running_service() as (service, client):
            fields = ({"profile": "custom:../../private"}, {"profile": "metal+custom:edm"}, {"out": "/private"},
                      {"settings": "secret.json"}, {"filename": "../secret.wav"}, {"filename": "C:\\private.wav"},
                      {"filename": "https://host/x.mp3"}, {"filename": "x.m3u"}, {"consent": False},
                      {"intensity": True}, {"target_lufs": -1}, {"ceiling_dbtp": 1}, {"mode": "unsafe"},
                      {"profile": {}}, {"bytes": True}, {"intensity": 10 ** 500})
            for extra in fields:
                value = {"filename": "song.wav", "bytes": 5, "consent": True, **extra}
                with self.subTest(extra=list(extra)):
                    status, _, result = client.request("POST", "/v1/jobs", value)
                    self.assertEqual(status, 400, result)
                    self.assertNotIn("private", json.dumps(result))
            self.assertEqual(len(service.store.jobs), 0)

    def test_malformed_json_and_http_framing(self):
        with running_service() as (_, client):
            for value in (b'{"filename":"x.wav","bytes":1,"consent":true,"bytes":2}',
                          b'{"filename":"x.wav","bytes":1,"consent":true,"intensity":NaN}', b'[]', b'bad', b'\xff'):
                self.assertEqual(client.request("POST", "/v1/jobs", raw=value, headers={"Content-Type": "application/json"})[0], 400)
            self.assertEqual(client.request("POST", "/v1/jobs", raw=b"{}", headers={"Content-Type": "text/plain"})[0], 415)
            self.assertEqual(client.request("POST", "/v1/jobs", raw=b"{}", headers={"Transfer-Encoding": "chunked"})[0], 400)
            self.assertEqual(client.request("POST", "/v1/jobs", raw=b"x" * 16385)[0], 413)

    def test_no_token_path_or_request_logging(self):
        stream = io.StringIO()
        logger = logging.getLogger("slimeybeats.hybrid")
        handler = logging.StreamHandler(stream)
        logger.addHandler(handler)
        try:
            with running_service() as (_, client):
                client.request("GET", "/v1/capabilities", auth=False, headers={"Authorization": "Bearer " + TOKEN[:-1]})
                client.request("GET", "/private/" + TOKEN + "?secret=" + TOKEN)
                client.request("GET", "/v1/jobs/%2e%2e/%2fprivate")
                client.request("GET", "/health")
        finally:
            logger.removeHandler(handler)
        self.assertNotIn(TOKEN, stream.getvalue())
        self.assertNotIn("/private", stream.getvalue())


if __name__ == "__main__":
    unittest.main()
