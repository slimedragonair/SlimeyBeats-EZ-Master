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


def verify(target):
    """Full cache validation is explicit work, never a per-frame status probe."""
    provenance = target / "provenance.json"
    if not provenance.is_file() or provenance.is_symlink() or provenance.stat().st_size >= 65536:
        raise ValueError("Model provenance is missing or invalid; run Repair stem engine")
    info = json.loads(provenance.read_text(encoding="utf-8"))
    if info.get("archive_sha256") != SHA256 or not isinstance(info.get("files"), list):
        raise ValueError("Unexpected model provenance; run Repair stem engine")
    checked = set()
    for item in info["files"]:
        name = item["file"]
        if Path(name).name != name or name in checked:
            raise ValueError("Invalid model manifest filename")
        checked.add(name)
        path = target / name
        if path.is_symlink() or not path.is_file() or path.stat().st_size != item["bytes"] or digest(path) != item["sha256"]:
            raise ValueError("Model integrity check failed; run Repair stem engine")
    if not {"checkpoint", "model.index", "model.data-00000-of-00001"}.issubset(checked):
        raise ValueError("Model checkpoint is incomplete; run Repair stem engine")
    return True


def discard_known_backup(backup, target):
    # Cleanup is optional. Damaged/unknown provenance must not turn an already
    # successful replacement into an installation failure or delete user files.
    try:
        previous = backup / "provenance.json"
        if not backup.is_dir() or backup.is_symlink() or not previous.is_file() or previous.is_symlink() or previous.stat().st_size >= 65536:
            return
        info = json.loads(previous.read_text(encoding="utf-8"))
        expected = {item["file"] for item in info.get("files", [])} | {"provenance.json"}
        if (info.get("archive_sha256") == SHA256 and
                all(p.name in expected and p.is_file() and not p.is_symlink() for p in backup.iterdir()) and
                backup.resolve().is_relative_to(target.parent.resolve())):
            shutil.rmtree(backup)
    except (OSError, ValueError, KeyError, TypeError, AttributeError):
        return


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
        if backup is not None:
            discard_known_backup(backup, target)
        print(json.dumps({"installed": True, "archive_sha256": SHA256}), flush=True)
    finally:
        if stage.exists():
            shutil.rmtree(stage)


if __name__ == "__main__":
    if sys.argv[1] == "--verify":
        verify(Path(sys.argv[2]))
        print(json.dumps({"verified": True}), flush=True)
    else:
        install(Path(sys.argv[1]), Path(sys.argv[2]))
