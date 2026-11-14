"""Validate and snapshot the original dependency notices used by recovery bundles."""
from dataclasses import dataclass
import json
from pathlib import Path
import re

from .license_payload import contained_file, relative_path, sha256


@dataclass(frozen=True)
class NoticePayload:
    inventory: bytes
    summary: bytes
    licenses: dict[str, bytes]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_file(directory, relative):
    path = contained_file(directory, relative)
    require(path.is_file(), f"Missing dependency notice file: {path}")
    return path.read_bytes()


def read_json(data, label):
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, f"Duplicate JSON key in {label}: {key}")
            result[key] = value
        return result

    try:
        return json.loads(data.decode("utf-8"), object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ValueError(f"Invalid dependency notice JSON: {label}") from error


def package_path(value, key):
    require(isinstance(value, str), f"Invalid notice path for {key}")
    path = relative_path(value)
    require(len(path.parts) >= 3 and path.parts[:2] == ("licenses", key),
            f"Notice path is outside its package {key}: {value}")
    return path


def original_files(directory, record, key):
    declared = record.get("license_files")
    require(isinstance(declared, list), f"Invalid license_files for {key}")
    require(all(isinstance(value, str) for value in declared), f"Invalid license_files for {key}")
    paths = [package_path(value, key).as_posix() for value in declared]
    require(len({value.casefold() for value in paths}) == len(paths),
            f"Duplicate or case-colliding notice paths for {key}")
    status = record.get("notice_status")
    provenance_path = record.get("notice_provenance")
    if status == "no_separate_notice_files":
        require(not paths and provenance_path is None,
                f"Contradictory no_separate_notice_files record for {key}")
        return {}
    require(status == "files_copied" and paths, f"Missing explicit notice status/files for {key}")
    package_path(provenance_path, key)
    expected_path = f"licenses/{key}/LICENSE-PROVENANCE.json"
    require(provenance_path == expected_path, f"Invalid notice provenance path for {key}")
    package = contained_file(directory, f"licenses/{key}")
    provenance_bytes = read_file(package, "LICENSE-PROVENANCE.json")
    provenance = read_json(provenance_bytes, provenance_path)
    require(isinstance(provenance, dict), f"Invalid notice provenance for {key}")
    require(type(provenance.get("format_version")) is int and provenance["format_version"] == 1
            and provenance.get("name") == record["name"]
            and provenance.get("version") == record["version"],
            f"Notice provenance identity/schema mismatch for {key}")
    entries = provenance.get("files")
    require(isinstance(entries, list) and entries, f"Empty notice provenance for {key}")
    files = {}
    seen = set()
    for entry in entries:
        require(isinstance(entry, dict) and isinstance(entry.get("path"), str),
                f"Invalid notice provenance file for {key}")
        local = relative_path(entry["path"])
        full = f"licenses/{key}/{local.as_posix()}"
        require(full.casefold() not in seen and full.casefold() != provenance_path.casefold(),
                f"Duplicate or reserved provenance file for {key}: {local}")
        seen.add(full.casefold())
        require(full in paths, f"Unlisted provenance notice for {key}: {local}")
        expected = entry.get("sha256")
        require(isinstance(expected, str) and re.fullmatch(r"[0-9a-fA-F]{64}", expected),
                f"Invalid original notice SHA-256 for {key}: {local}")
        data = read_file(package, local.as_posix())
        require(sha256(data) == expected.lower(), f"Original notice hash mismatch: {full}")
        sources = entry.get("sources")
        require(isinstance(sources, list) and sources, f"Missing original notice sources: {full}")
        for source in sources:
            require(isinstance(source, dict) and source.get("path") == entry["path"]
                    and isinstance(source.get("kind"), str)
                    and source.get("kind") in {"cargo_package", "pinned_upstream"},
                    f"Invalid original notice source: {full}")
            if source["kind"] == "pinned_upstream":
                require(isinstance(source.get("sha256"), str)
                        and source["sha256"].lower() == expected.lower(),
                        f"Pinned upstream notice hash mismatch: {full}")
        files[full] = data
    require(set(files) == set(paths), f"License inventory/provenance file mismatch for {key}")
    files[provenance_path] = provenance_bytes
    return files


def collect(directory):
    """Return exact verified bytes, or fail before the caller writes any helper ZIP."""
    directory = Path(directory)
    inventory_bytes = read_file(directory, "THIRD-PARTY.json")
    summary = read_file(directory, "THIRD-PARTY-NOTICES.txt")
    records = read_json(inventory_bytes, "THIRD-PARTY.json")
    require(isinstance(records, list) and records, "Dependency notice inventory must be a nonempty list")
    files = {}
    packages = set()
    for record in records:
        require(isinstance(record, dict) and isinstance(record.get("name"), str)
                and bool(record["name"]) and isinstance(record.get("version"), str)
                and bool(record["version"]), "Invalid dependency notice package record")
        key = f"{record['name']}-{record['version']}"
        require(len(relative_path(key).parts) == 1, f"Invalid dependency notice package: {key}")
        require(key.casefold() not in packages, f"Duplicate dependency notice package: {key}")
        packages.add(key.casefold())
        files.update(original_files(directory, record, key))
    root = contained_file(directory, "licenses")
    actual = set()
    if root.exists():
        require(root.is_dir(), "Dependency licenses path is not a directory")
        for path in root.rglob("*"):
            relative = path.relative_to(directory).as_posix()
            checked = contained_file(directory, relative)
            if checked.is_file():
                require(relative in files, f"Unlisted dependency notice file: {relative}")
                actual.add(relative)
            else:
                require(checked.is_dir(), f"Invalid dependency notice tree entry: {relative}")
    require(actual == set(files), "Dependency notice tree differs from the declared file inventory")
    # Archive the snapshot, not paths that could change after hash validation.
    return NoticePayload(inventory_bytes, summary,
                         {name.removeprefix("licenses/"): data for name, data in files.items()})
