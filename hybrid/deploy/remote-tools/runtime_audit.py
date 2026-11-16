"""Read-only runtime provenance and dependency inventory for an operator."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    hasher = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def audit(base):
    python = base / "env/bin/python"
    receipt = json.loads((base / "engine.json").read_text())
    model = base / "models/4stems"
    provenance = json.loads((model / "provenance.json").read_text())
    checked = []
    for item in provenance["files"]:
        name = item["file"]
        if Path(name).name != name:
            raise ValueError("Unexpected model manifest entry")
        path = model / name
        actual = digest(path)
        if path.is_symlink() or path.stat().st_size != item["bytes"] or actual != item["sha256"]:
            raise ValueError("Installed model integrity failure")
        checked.append({"file": name, "bytes": path.stat().st_size, "sha256": actual})
    archive = base / "4stems.tar.gz"
    archive_sha = digest(archive)
    if archive_sha != receipt["model_sha256"] or archive_sha != provenance["archive_sha256"]:
        raise ValueError("Archive/receipt/provenance mismatch")
    code = "import importlib.metadata,json,sys; print(json.dumps({'python':sys.version,'prefix':sys.prefix,'packages':{p.metadata['Name']:p.version for p in importlib.metadata.distributions()}}))"
    packages = subprocess.run([str(python), "-I", "-B", "-c", code], capture_output=True, text=True, timeout=30, check=True)
    paths = {"base": str(base), "python": str(python), "python_target": str(python.resolve()), "models": str(model), "receipt": str(base / "engine.json")}
    return {"paths": paths, "receipt": receipt, "model_archive_sha256": archive_sha, "model_files": checked,
            "python_executable_sha256": digest(python), "environment": json.loads(packages.stdout),
            "claim": "provenance and importlib package metadata; inference measured separately"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", type=Path, required=True)
    args = parser.parse_args()
    if not args.base.is_absolute():
        parser.error("--base must be absolute")
    print(json.dumps(audit(args.base), indent=2))


if __name__ == "__main__":
    main()
