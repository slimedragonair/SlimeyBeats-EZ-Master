"""Small v2 test helpers; real worker tests require explicit built binaries."""

from contextlib import contextmanager
import os
from pathlib import Path
import sys
import time

from hybrid.tests.support import running_service


@contextmanager
def project_service(**options):
    defaults = {"studio_cli": Path(sys.executable), "v2_max_upload_bytes": 16 * 1024**2,
                "v2_max_decoded_bytes": 16 * 1024**2, "v2_max_project_disk_bytes": 128 * 1024**2}
    defaults.update(options)
    with running_service(**defaults) as context:
        yield context


def create_project(client):
    status, _, manifest = client.request("POST", "/v2/projects", {})
    assert status == 201, (status, manifest)
    return manifest


def upload(client, project_id, data, role="original", filename="source.wav", label=None, kind=None):
    base = "/v2/projects/" + project_id
    fields = {"filename": filename, "bytes": len(data), "role": role}
    if label is not None:
        fields["label"] = label
    if kind is not None:
        fields["kind"] = kind
    status, _, entry = client.request("POST", base + "/uploads", fields)
    assert status == 201, (status, entry)
    for offset in range(0, len(data), 1024**2):
        status, _, entry = client.request("PUT", base + "/uploads/" + entry["id"] + "?offset=" + str(offset),
                                         raw=data[offset:offset + 1024**2])
        assert status == 200, (status, entry)
    return entry["id"]


def command(client, manifest, operation, timeout=90, **fields):
    base = "/v2/projects/" + manifest["id"]
    status, _, result = client.request("POST", base + "/commands",
                                      {"operation": operation, "base_revision": manifest["revision"], **fields})
    assert status == 202, (status, result)
    deadline = time.monotonic() + timeout
    while result["command"]["state"] in ("queued", "running"):
        if time.monotonic() >= deadline:
            raise AssertionError("Project command did not finish")
        time.sleep(0.03)
        status, _, result = client.request("GET", base)
        assert status == 200, (status, result)
    return result


def native_studio(test):
    value = os.environ.get("HYBRID_TEST_STUDIO_CLI")
    if not value:
        test.skipTest("Set HYBRID_TEST_STUDIO_CLI for real Studio HTTP integration")
    ffmpeg = os.environ.get("HYBRID_TEST_FFMPEG")
    if not ffmpeg:
        test.skipTest("Set HYBRID_TEST_FFMPEG for real Studio codec integration")
    return Path(value).resolve(strict=True), Path(ffmpeg).resolve(strict=True)
