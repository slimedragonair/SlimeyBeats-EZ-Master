"""Compose immutable bank edits and the full routed mix within one transaction."""

from copy import deepcopy

from .errors import ProcessingError
from .project_native_request import private_request
from .project_outputs import collect_outputs


def target_stem(command):
    audio_id = command.request.get("audio_id")
    return next((stem for stem in command.project.stems if stem["id"] == audio_id), None)


def remix_bank(command, config, runner, driver, edited_result=None):
    project, operation = command.project, command.request["operation"]
    stem = target_stem(command)
    if stem is None or command.cancel.is_set():
        raise ProcessingError("Bank edit cancelled or missing target")
    if operation == "replace":
        replacement_id = project.preview_id
        replacement = project.assets[replacement_id]
        edited_assets = {}
    else:
        edited_assets, artifacts, replacement_id, _ = edited_result
        if artifacts or replacement_id not in edited_assets:
            raise ProcessingError("Missing immutable stem edit")
        replacement = edited_assets[replacement_id]
        replacement.update(role="stem", kind=stem["kind"], name=stem["name"])
    request = private_request(command, config, "mix", {})
    request["stems"] = [{"kind": entry["kind"],
                         "path": str(replacement["path"] if entry["kind"] == stem["kind"] else project.assets[entry["id"]]["path"]),
                         "name": entry["name"]} for entry in project.stems]
    request["mix"] = deepcopy(project.controls)
    # Driver request/response files are removed after each reaped invocation;
    # native output paths are unique. Both passes share this command's cancel
    # event and project budget, and remain private until the final atomic commit.
    raw = driver.invoke(command, request)
    mixed_assets, artifacts, primary, updates = collect_outputs(command, raw, config, runner)
    if primary is None or artifacts:
        raise ProcessingError("Missing complete bank mix")
    updates["_bank_replacements"] = {stem["kind"]: replacement_id}
    return {**edited_assets, **mixed_assets}, {}, primary, updates
