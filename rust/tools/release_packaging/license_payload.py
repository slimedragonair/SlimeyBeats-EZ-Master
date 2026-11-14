"""Copy original Cargo notices and reviewed, hash-pinned upstream supplements offline."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import re


OVERRIDES = Path(__file__).with_name("license_overrides")
NOTICE_NAME = re.compile(
    r"(?:^|[-_. ])(?:licen[cs]es?|copying|notices?|unlicense)(?=$|[-_. ])", re.I
)
SOURCE_SUFFIXES = {".rs", ".c", ".h", ".cpp", ".py", ".js", ".toml", ".json"}
# These upstream font notices have descriptive names rather than LICENSE prefixes.
EXTRA_NOTICES = {"epaint_default_fonts": ("fonts/*.txt",)}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def relative_path(value):
    path = PurePosixPath(value)
    if (not value or not path.parts or path.is_absolute() or ".." in path.parts
            or "\\" in value or ":" in value or path.as_posix() != value):
        raise ValueError(f"Unsafe notice path: {value!r}")
    return path


def contained_file(directory, relative):
    path = directory / relative_path(relative)
    if not path.resolve().is_relative_to(directory.resolve()):
        raise ValueError(f"Notice escapes package directory: {relative}")
    return path


def json_file(path):
    return json.loads(path.read_text(encoding="utf-8")) if path.is_file() else None


def upstream_url(manifest, entry):
    """Only the crate's official public GitHub repository at a full commit is supported."""
    repository = manifest["repository"].split("/tree/", 1)[0].rstrip("/").removesuffix(".git")
    if (not re.fullmatch(r"https://github\.com/[\w.-]+/[\w.-]+", repository)
            or manifest["upstream_repository"] != repository
            or not re.fullmatch(r"[0-9a-f]{40}", manifest["upstream_commit"])):
        raise ValueError("Expected official HTTPS GitHub repository and immutable commit")
    path = relative_path(entry["upstream_path"]).as_posix()
    url = (repository.replace("https://github.com/", "https://raw.githubusercontent.com/", 1)
           + "/" + manifest["upstream_commit"] + "/" + path)
    if entry["url"] != url or not re.fullmatch(r"[0-9a-f]{64}", entry["sha256"]):
        raise ValueError("Pinned notice URL or SHA-256 is invalid")
    return url


def original_notices(package, directory):
    selected = set()
    for path in directory.rglob("*"):
        if (path.is_file() and path.suffix.lower() not in SOURCE_SUFFIXES
                and (NOTICE_NAME.search(path.name)
                     or path.name.upper() in {"OFL.TXT", "UFL.TXT", "FTL.TXT"})):
            selected.add(path.relative_to(directory).as_posix())
    for pattern in EXTRA_NOTICES.get(package["name"], ()):
        selected.update(path.relative_to(directory).as_posix()
                        for path in directory.glob(pattern) if path.is_file())
    if package.get("license_file"):
        path = (directory / package["license_file"]).resolve()
        if path.is_file() and path.is_relative_to(directory.resolve()):
            selected.add(path.relative_to(directory.resolve()).as_posix())
    return sorted(selected)


def pinned_manifest(package, directory, override_directory):
    manifest = json_file(override_directory / "manifest.json")
    if manifest is None:
        return None
    for field in ("name", "version", "source", "license", "repository"):
        if manifest[field] != package.get(field):
            raise ValueError(f"License override {field} mismatch: {override_directory.name}")
    checksum = json_file(directory / ".cargo-checksum.json")
    if (manifest["format_version"] != 1
            or manifest["crate_checksum"] != (package.get("checksum")
                                               or (checksum or {}).get("package"))
            or manifest["crate_vcs"] != json_file(directory / ".cargo_vcs_info.json")):
        raise ValueError(f"License override crate provenance mismatch: {override_directory.name}")
    return manifest


def copy_dependency_licenses(packages, destination, overrides=OVERRIDES):
    """Keep Cargo declarations intact; each copied byte has a package-local provenance record."""
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=True)
    records = []
    for package in sorted(packages, key=lambda item: (item["name"], item["version"])):
        if package["source"] is None:
            continue
        directory = Path(package["manifest_path"]).parent.resolve()
        key = f"{package['name']}-{package['version']}"
        relative_path(key)
        override_directory = Path(overrides) / key
        manifest = pinned_manifest(package, directory, override_directory)
        files = {}
        collision_keys = {}

        def add(relative, data, provenance):
            relative_path(relative)
            folded = relative.casefold()
            if folded == "license-provenance.json":
                raise ValueError(f"Reserved notice path in {key}: {relative}")
            if folded in collision_keys and collision_keys[folded] != relative:
                raise ValueError(f"Case-insensitive notice collision in {key}: {relative}")
            collision_keys[folded] = relative
            if relative in files and files[relative][0] != data:
                raise ValueError(f"Conflicting original notice in {key}: {relative}")
            if relative not in files:
                files[relative] = (data, {"path": relative, "sha256": sha256(data),
                                          "sources": []})
            files[relative][1]["sources"].append(provenance)

        for relative in original_notices(package, directory):
            data = contained_file(directory, relative).read_bytes()
            add(relative, data, {"kind": "cargo_package", "path": relative})
        if manifest:
            for entry in manifest["files"]:
                upstream_url(manifest, entry)
                data = contained_file(override_directory, entry["path"]).read_bytes()
                if sha256(data) != entry["sha256"]:
                    raise ValueError(f"Pinned license hash mismatch: {key}/{entry['path']}")
                add(entry["path"], data, {"kind": "pinned_upstream", **entry})
        output = destination / "licenses" / key
        for relative, (data, _) in sorted(files.items()):
            target = contained_file(output, relative)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
        provenance_path = None
        if files:
            checksum = json_file(directory / ".cargo-checksum.json")
            provenance = {
                "format_version": 1, "name": package["name"], "version": package["version"],
                "source": package["source"],
                "crate_checksum": package.get("checksum") or (checksum or {}).get("package"),
                "crate_vcs": json_file(directory / ".cargo_vcs_info.json"),
                "upstream_override": manifest,
                "files": [item[1] for _, item in sorted(files.items())],
            }
            provenance_path = f"licenses/{key}/LICENSE-PROVENANCE.json"
            (output / "LICENSE-PROVENANCE.json").write_text(
                json.dumps(provenance, indent=2) + "\n", encoding="utf-8")
        records.append({
            "name": package["name"], "version": package["version"],
            "license": package["license"], "repository": package["repository"],
            "license_files": [f"licenses/{key}/{relative}" for relative in sorted(files)],
            "notice_provenance": provenance_path,
            "notice_status": "files_copied" if files else "no_separate_notice_files",
        })
    return records
