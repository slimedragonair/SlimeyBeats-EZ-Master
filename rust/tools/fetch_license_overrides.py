"""Explicit maintainer-only verification/restoration of already reviewed license pins.

Normal package_release runs never import this module or access the network.
This command cannot discover/update pins; review manifests separately for a new crate.
"""
import argparse
import json
import urllib.request

from release_packaging.license_payload import OVERRIDES, contained_file, sha256, upstream_url


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError(f"Refusing license-source redirect: HTTP {code}")


def verify_upstream(restore=False):
    opener = urllib.request.build_opener(NoRedirect())
    retrieved = {}
    count = 0
    for path in sorted(OVERRIDES.glob("*/manifest.json")):
        manifest = json.loads(path.read_text(encoding="utf-8"))
        for entry in manifest["files"]:
            url = upstream_url(manifest, entry)
            if url not in retrieved:
                request = urllib.request.Request(url, headers={"User-Agent": "license-payload-maintenance"})
                with opener.open(request, timeout=20) as response:
                    data = response.read(1024 * 1024 + 1)
                if len(data) > 1024 * 1024:
                    raise ValueError(f"Unexpectedly large notice: {url}")
                retrieved[url] = data
            data = retrieved[url]
            if sha256(data) != entry["sha256"]:
                raise ValueError(f"Upstream content no longer matches pin: {url}")
            target = contained_file(path.parent, entry["path"])
            if restore:
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(data)
            elif target.read_bytes() != data:
                raise ValueError(f"Local override differs from verified upstream: {target}")
            count += 1
    return {"verified_files": count, "distinct_upstream_files": len(retrieved), "restored": restore}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    action = parser.add_mutually_exclusive_group(required=True)
    action.add_argument("--verify-upstream", action="store_true")
    action.add_argument("--restore", action="store_true", help="Restore files only if their reviewed hashes match")
    args = parser.parse_args()
    print(json.dumps(verify_upstream(restore=args.restore), indent=2))
