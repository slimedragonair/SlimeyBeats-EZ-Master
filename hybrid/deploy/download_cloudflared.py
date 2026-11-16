"""Stage one explicitly pinned Linux release binary; never install or execute it."""

from __future__ import annotations

import argparse
import hashlib
import http.client
import os
from pathlib import Path
import re
import tempfile
import urllib.error
import urllib.parse
import urllib.request

MAX_BINARY_BYTES = 128 * 1024**2
READ_BYTES = 1024 * 1024
PUBLIC_HEADERS = {"Accept": "application/octet-stream", "User-Agent": "slimeybeats-cloudflared-preparation/1"}


class DownloadError(Exception):
    """A preparation failure whose message contains no response or redirect data."""


def _allowed_url(url: str, release_url: str) -> bool:
    try:
        parsed = urllib.parse.urlsplit(url)
        if (any(ord(char) < 33 for char in url) or parsed.scheme != "https"
                or parsed.username or parsed.password or parsed.port not in (None, 443) or parsed.fragment):
            return False
        return url == release_url or (
            parsed.hostname == "release-assets.githubusercontent.com"
            and parsed.path.startswith("/github-production-release-asset/")
        )
    except ValueError:
        return False


class _ReleaseRedirects(urllib.request.HTTPRedirectHandler):
    max_redirections = 3

    def __init__(self, release_url: str):
        super().__init__()
        self.release_url = release_url

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if not _allowed_url(newurl, self.release_url):
            raise DownloadError("Release redirect outside the HTTPS GitHub asset allowlist.")
        # Rebuild every request: no authorization, cookies or inherited request headers.
        return urllib.request.Request(newurl, headers=PUBLIC_HEADERS, method="GET")


def stage_binary(version: str, sha256: str, arch: str, destination: Path) -> None:
    if not re.fullmatch(r"20\d{2}\.\d{1,2}\.\d{1,2}", version):
        raise DownloadError("Use an explicit release version such as 2026.11.0.")
    if not re.fullmatch(r"[0-9a-fA-F]{64}", sha256):
        raise DownloadError("An explicit 64-character SHA-256 is required.")
    if arch not in ("amd64", "arm64"):
        raise DownloadError("Choose the target Linux architecture: amd64 or arm64.")
    if hasattr(os, "geteuid") and os.geteuid() == 0:
        raise DownloadError("Run as an unprivileged operator and stage in a writable directory.")

    release_url = f"https://github.com/cloudflare/cloudflared/releases/download/{version}/cloudflared-linux-{arch}"
    temporary = None
    try:
        # No directory creation, existing-file replacement, shell, installer or execution.
        destination = destination.parent.resolve(strict=True) / destination.name
        if os.path.lexists(destination):
            raise DownloadError("Destination already exists; choose a new staging path.")
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), _ReleaseRedirects(release_url))
        request = urllib.request.Request(release_url, headers=PUBLIC_HEADERS, method="GET")
        digest, size = hashlib.sha256(), 0
        with opener.open(request, timeout=30) as response:
            if response.status != 200 or not _allowed_url(response.geturl(), release_url):
                raise DownloadError("Unexpected release response.")
            length = response.headers.get("Content-Length")
            if length is not None and (not length.isdecimal() or int(length) > MAX_BINARY_BYTES):
                raise DownloadError("Unexpected release size.")
            with tempfile.NamedTemporaryFile(prefix=".cloudflared-", suffix=".tmp", dir=destination.parent, delete=False) as output:
                temporary = Path(output.name)
                while block := response.read(READ_BYTES):
                    size += len(block)
                    if size > MAX_BINARY_BYTES:
                        raise DownloadError("Release exceeds the staging size limit.")
                    digest.update(block)
                    output.write(block)
                output.flush()
                os.fsync(output.fileno())
        if size == 0 or digest.hexdigest() != sha256.lower():
            raise DownloadError("Release SHA-256 mismatch; no binary staged.")
        temporary.chmod(0o700)
        # A hard link publishes atomically and fails if any file/symlink now occupies dest.
        os.link(temporary, destination)
    except urllib.error.HTTPError as error:
        error.close()
        raise DownloadError("Release download failed; no binary staged.") from None
    except (OSError, ValueError, urllib.error.URLError, http.client.HTTPException):
        raise DownloadError("Release download or staging failed; no binary installed.") from None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True, help="Exact reviewed release tag; never latest")
    parser.add_argument("--sha256", required=True, help="Expected digest from the official release")
    parser.add_argument("--arch", required=True, choices=("amd64", "arm64"), help="Target Ubuntu architecture")
    parser.add_argument("--dest", required=True, type=Path, help="New file in an existing unprivileged staging directory")
    args = parser.parse_args()
    try:
        stage_binary(args.version, args.sha256, args.arch, args.dest)
    except DownloadError as error:
        print(f"Preparation failed: {error}")
        return 1
    print("Verified binary staged. Installation and activation remain operator actions.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
