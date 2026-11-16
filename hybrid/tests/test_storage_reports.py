"""Confined restart cleanup, exclusive ownership and projected public reports."""

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from hybrid.server.config import Config, parse_origins
from hybrid.server.reports import sanitize_report
from hybrid.server.requests import parse_master_request
from hybrid.server.storage import Storage
from hybrid.tests.support import TOKEN, running_service


class StorageReportTests(unittest.TestCase):
    def test_restart_removes_only_marked_owned_job_directories(self):
        with tempfile.TemporaryDirectory(prefix="hybrid-restart-test-") as temp:
            config = Config(cli=Path(sys.executable), token=TOKEN, data_dir=Path(temp) / "store")
            storage = Storage(config)
            owned = storage.create("a" * 32)
            (owned / "partial.wav").write_bytes(b"private audio")
            unrelated = storage.root / ("job-" + "b" * 32)
            unrelated.mkdir()
            (unrelated / "keep.txt").write_text("unrelated")
            (storage.root / "unrelated.txt").write_text("keep")
            with self.assertRaises(OSError):
                Storage(config)
            self.assertTrue(owned.exists())
            storage.close()
            restarted = Storage(config)
            try:
                self.assertFalse(owned.exists())
                self.assertEqual((unrelated / "keep.txt").read_text(), "unrelated")
                self.assertEqual((storage.root / "unrelated.txt").read_text(), "keep")
            finally:
                restarted.close()

    def test_refuses_to_adopt_unmarked_storage(self):
        with tempfile.TemporaryDirectory(prefix="hybrid-unowned-") as temp:
            path = Path(temp)
            (path / "keep.txt").write_text("keep")
            with self.assertRaises(ValueError):
                Storage(Config(cli=Path(sys.executable), token=TOKEN, data_dir=path))
            self.assertEqual((path / "keep.txt").read_text(), "keep")

    def test_public_report_discards_all_free_text_and_nested_private_keys(self):
        with tempfile.TemporaryDirectory(prefix="hybrid-report-") as temp:
            directory = Path(temp)
            raw = directory / "raw.json"
            public = directory / "report.json"
            private = "C:/Users/private-owner/token-secret/audio.wav"
            raw.write_text(json.dumps({"file": private, "out_dir": private,
                                       "configuration": {"path": private}, "notes": [private],
                                       "profile": private, "version": private, "log": [{"detail": private}],
                                       "exports": [{"path": private}], "target_lufs": -14,
                                       "input": {"metrics": {"integrated_lufs": -18, "path": private}},
                                       "output": {"metrics": {"integrated_lufs": -14, "host": private}},
                                       "limiter": {"reachable": True, "error": private}}))
            config = Config(cli=Path(sys.executable), token=TOKEN)
            request = parse_master_request({"filename": "song.wav", "bytes": 1, "consent": True}, config)
            sanitize_report(raw, public, request)
            text = public.read_text()
            for secret in ("private-owner", "token-secret", '"file"', '"path"', '"out_dir"', '"host"', '"notes"', '"exports"'):
                self.assertNotIn(secret, text)
            self.assertEqual(json.loads(text)["output"]["metrics"]["integrated_lufs"], -14)

    def test_disk_reservation_failure_is_an_explicit_http_error(self):
        with running_service() as (service, client):
            with patch("hybrid.server.storage.shutil.disk_usage") as usage:
                usage.return_value.free = 0
                status, _, result = client.create(b"x")
            self.assertEqual(status, 507)
            self.assertEqual(len(service.store.jobs), 0)

    def test_token_and_origin_configuration_is_fail_closed(self):
        for token in ("", "short", "x" * 31, "é" * 32, "x" * 32 + "\n"):
            with self.assertRaises(ValueError):
                Config(cli=Path(sys.executable), token=token)
        for origin in ("*", "null", "https://owner.example/", "https://owner.example/path", "https://user:pass@owner.example"):
            with self.assertRaises(ValueError):
                parse_origins(origin)
        self.assertNotIn(TOKEN, repr(Config(cli=Path(sys.executable), token=TOKEN)))
        if not sys.platform.startswith("linux"):
            with self.assertRaises(ValueError):
                Config(cli=Path(sys.executable), token=TOKEN, host="0.0.0.0")
