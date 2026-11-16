"""Keep current and two undo/redo states, reclaiming only owned stale command files."""

import re

from .errors import ProcessingError
from .storage import is_link

COMMAND_DIRECTORY = re.compile(r"command-[a-f0-9]{32}\Z")


def retain_history_assets(project):
    referenced = set(project.reference_assets)
    referenced.update(key for key, asset in project.assets.items() if asset["role"] == "reference")
    for state in [project.checkpoint(), *project.undo, *project.redo]:
        for field in ("original_id", "master_id", "preview_id", "reference_id", "input_id", "before_id"):
            if state.get(field):
                referenced.add(state[field])
        referenced.update(stem["id"] for stem in state["stems"])
    project.assets = {key: asset for key, asset in project.assets.items() if key in referenced}


def reclaim_commands(project):
    keep = {entry["path"] for entry in project.artifacts.values()}
    for asset in project.assets.values():
        keep.update(asset[key] for key in ("path", "preview_path", "waveform_path") if key in asset)
    for folder in project.directory.iterdir():
        if not COMMAND_DIRECTORY.fullmatch(folder.name):
            continue
        if is_link(folder) or not folder.is_dir():
            raise ProcessingError("Unsafe committed storage")
        entries = list(folder.rglob("*"))
        if len(entries) > project.max_storage_files or any(is_link(entry) for entry in entries):
            raise ProcessingError("Unsafe committed storage")
        for entry in sorted(entries, key=lambda item: len(item.parts), reverse=True):
            if entry.is_file() and entry not in keep:
                entry.unlink()
            elif entry.is_dir() and not any(entry.iterdir()):
                entry.rmdir()
        if not any(folder.iterdir()):
            folder.rmdir()
