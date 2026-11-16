"""No-network regression tests for pinned, unprivileged release staging."""

import hashlib
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.request

DEPLOY = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(DEPLOY))

from download_cloudflared import DownloadError, _ReleaseRedirects, _allowed_url, stage_binary

VERSION = "2026.11.0"
RELEASE_URL = f"https://github.com/cloudflare/cloudflared/releases/download/{VERSION}/cloudflared-linux-amd64"
ASSET_URL = "https://release-assets.githubusercontent.com/github-production-release-asset/123/fixture?signature=public"
BINARY = b"fixture binary; never executed"
DIGEST = hashlib.sha256(BINARY).hexdigest()


class Response(io.BytesIO):
    status = 200
    headers = {}

    def geturl(self):
        return ASSET_URL


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.destination = Path(self.directory.name) / "cloudflared"
        self.uid = patch.object(os, "geteuid", return_value=1000, create=True)
        self.uid.start()
        self.addCleanup(self.uid.stop)

    def test_pinned_bytes_are_verified_before_atomic_staging_without_credentials(self):
        with patch("download_cloudflared.urllib.request.build_opener") as builder, \
                patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": "fixture-private", "HTTPS_PROXY": "http://untrusted.invalid"}):
            builder.return_value.open.return_value = Response(BINARY)
            stage_binary(VERSION, DIGEST, "amd64", self.destination)
        self.assertEqual(self.destination.read_bytes(), BINARY)
        self.assertEqual([path.name for path in self.destination.parent.iterdir()], ["cloudflared"])
        request = builder.return_value.open.call_args.args[0]
        self.assertEqual(request.full_url, RELEASE_URL)
        self.assertEqual({key.lower() for key in request.headers}, {"accept", "user-agent"})
        self.assertEqual(builder.call_args.args[0].proxies, {})

    def test_wrong_digest_never_publishes_and_removes_temporary_file(self):
        with patch("download_cloudflared.urllib.request.build_opener") as builder:
            builder.return_value.open.return_value = Response(BINARY)
            with self.assertRaisesRegex(DownloadError, "SHA-256 mismatch"):
                stage_binary(VERSION, "0" * 64, "amd64", self.destination)
        self.assertEqual(list(self.destination.parent.iterdir()), [])

    def test_missing_pins_invalid_architecture_root_and_existing_destinations_send_nothing(self):
        with patch("download_cloudflared.urllib.request.build_opener") as builder:
            for version, digest, arch in (("latest", DIGEST, "amd64"), ("../2026.11.0", DIGEST, "amd64"),
                                         (VERSION, "not-a-digest", "amd64"), (VERSION, DIGEST, "x86")):
                with self.subTest(version=version, arch=arch), self.assertRaises(DownloadError):
                    stage_binary(version, digest, arch, self.destination)
            with patch.object(os, "geteuid", return_value=0), self.assertRaises(DownloadError):
                stage_binary(VERSION, DIGEST, "amd64", self.destination)
            with self.assertRaises(DownloadError):
                stage_binary(VERSION, DIGEST, "amd64", self.destination / "missing-parent")
            self.destination.write_bytes(b"existing")
            with self.assertRaises(DownloadError):
                stage_binary(VERSION, DIGEST, "amd64", self.destination)
            builder.assert_not_called()
        self.assertEqual(self.destination.read_bytes(), b"existing")

    def test_redirects_allow_only_https_release_assets_and_strip_inherited_headers(self):
        handler = _ReleaseRedirects(RELEASE_URL)
        original = urllib.request.Request(RELEASE_URL, headers={"Authorization": "secret", "Cookie": "private"})
        redirected = handler.redirect_request(original, None, 302, "", {}, ASSET_URL)
        self.assertEqual(redirected.full_url, ASSET_URL)
        self.assertEqual({key.lower() for key in redirected.headers}, {"accept", "user-agent"})
        rejected = ("http://release-assets.githubusercontent.com/github-production-release-asset/123",
                    "https://release-assets.githubusercontent.com.evil.invalid/github-production-release-asset/123",
                    "https://user:secret@release-assets.githubusercontent.com/github-production-release-asset/123",
                    "https://release-assets.githubusercontent.com:444/github-production-release-asset/123",
                    "https://release-assets.githubusercontent.com/not-a-release/123", "file:///tmp/cloudflared",
                    "https://github.com/other/project/releases/download/tag/binary",
                    "https://example.invalid/cloudflared", ASSET_URL + "#fragment", ASSET_URL + "\n")
        for url in rejected:
            with self.subTest(url=url):
                self.assertFalse(_allowed_url(url, RELEASE_URL))
                with self.assertRaises(DownloadError):
                    handler.redirect_request(original, None, 302, "", {}, url)

    def test_oversized_declared_or_streamed_response_leaves_no_file(self):
        for headers in ({"Content-Length": "5"}, {}):
            with self.subTest(headers=headers), patch("download_cloudflared.MAX_BINARY_BYTES", 4), \
                    patch("download_cloudflared.urllib.request.build_opener") as builder:
                response = Response(b"12345")
                response.headers = headers
                builder.return_value.open.return_value = response
                with self.assertRaises(DownloadError):
                    stage_binary(VERSION, DIGEST, "amd64", self.destination)
                self.assertEqual(list(self.destination.parent.iterdir()), [])

    def test_destination_created_during_download_is_not_overwritten(self):
        def raced_link(source, destination):
            Path(destination).write_bytes(b"concurrent operator file")
            raise FileExistsError()

        with patch("download_cloudflared.urllib.request.build_opener") as builder, \
                patch("download_cloudflared.os.link", side_effect=raced_link):
            builder.return_value.open.return_value = Response(BINARY)
            with self.assertRaises(DownloadError):
                stage_binary(VERSION, DIGEST, "amd64", self.destination)
        self.assertEqual(self.destination.read_bytes(), b"concurrent operator file")
        self.assertEqual([path.name for path in self.destination.parent.iterdir()], ["cloudflared"])


if __name__ == "__main__":
    unittest.main()
