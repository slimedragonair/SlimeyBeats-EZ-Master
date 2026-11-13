"""Extract only the verified upstream model's regular files into an owned cache."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import uuid

SHA256 = "3adb4a50ad4eb18c7c4d65fcf4cf2367a07d48408a5eb7d03cd20067429dfaa8"


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1048576), b""):
            value.update(chunk)
    return value.hexdigest()


def install(archive, target):
    if digest(archive) != SHA256:
        raise ValueError("The model archive did not pass its SHA-256 check")
    target.parent.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(prefix=".model-stage-", dir=target.parent))
    try:
        with tarfile.open(archive) as bundle:
            members = bundle.getmembers()
            # Upstream's verified archive includes an irrelevant macOS resource
            # fork for checkpoint. It is not part of the TensorFlow model.
            members = [entry for entry in members if not Path(entry.name).name.startswith("._")]
            total = 0
            for entry in members:
                relative = Path(entry.name)
                if relative.is_absolute() or ".." in relative.parts or len(relative.parts) != 1 or not entry.isfile():
                    raise ValueError("Unexpected model archive member")
                if not (entry.name == "checkpoint" or entry.name.startswith("model.")):
                    raise ValueError("Unexpected model archive filename")
                total += entry.size
            if total > 1024 * 1024 * 1024 or len(members) > 16:
                raise ValueError("Model archive exceeds its extraction budget")
            for entry in members:
                with bundle.extractfile(entry) as source, (stage / entry.name).open("xb") as output:
                    shutil.copyfileobj(source, output)
        if not (stage / "model.index").is_file():
            raise ValueError("Model checkpoint is incomplete")
        (stage / ".probe").write_text("Verified by SlimeyBeats\n", encoding="utf-8")
        files = [{"file": p.name, "bytes": p.stat().st_size, "sha256": digest(p)}
                 for p in stage.iterdir() if p.is_file()]
        (stage / "provenance.json").write_text(json.dumps({
            "source": "https://github.com/deezer/spleeter/releases/download/v1.4.0/4stems.tar.gz",
            "archive_sha256": SHA256, "files": files,
            "notice": "Third-party model; not relicensed by SlimeyBeats. See the upstream release and project notices."
        }, indent=2), encoding="utf-8")
        backup = None
        if target.exists():
            backup = target.with_name(target.name + ".previous-" + uuid.uuid4().hex[:12])
            target.rename(backup)
        stage.rename(target)
        # Delete only an identifiable previous cache containing no extra files.
        # Unknown/user-added files remain in the uniquely named backup.
        if backup is not None and backup.is_dir() and not backup.is_symlink():
            previous = backup / "provenance.json"
            if previous.is_file() and previous.stat().st_size < 65536:
                info = json.loads(previous.read_text(encoding="utf-8"))
                expected = {item["file"] for item in info.get("files", [])} | {"provenance.json"}
                actual = list(backup.iterdir())
                if (info.get("archive_sha256") == SHA256 and
                        all(p.name in expected and p.is_file() and not p.is_symlink() for p in actual) and
                        backup.resolve().is_relative_to(target.parent.resolve())):
                    shutil.rmtree(backup)
        print(json.dumps({"installed": True, "archive_sha256": SHA256}), flush=True)
    finally:
        if stage.exists():
            shutil.rmtree(stage)


if __name__ == "__main__":
    install(Path(sys.argv[1]), Path(sys.argv[2]))
