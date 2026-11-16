"""Revision checks, immutable command submission and fast Studio history changes."""

from copy import deepcopy
import secrets

from .errors import ApiError
from .principals import OWNER
from .project_models import ProjectCommand
from .project_requests import command_request
from .project_limits import MAX_ARTIFACTS
from .project_references import reference_projection


def selected_audio(project, request):
    operation = request["operation"]
    default = project.input_id or project.original_id if operation == "master" else (
        project.master_id or project.preview_id if operation == "render" else project.preview_id)
    audio_id = request.get("audio_id", default)
    asset = project.assets.get(audio_id)
    if asset is None:
        raise ApiError(409, "audio_required")
    return asset


def selected_stems(project, request):
    selection = request.get("stems", [{"audio_id": stem["id"], "kind": stem["kind"]} for stem in project.stems])
    if not selection:
        raise ApiError(409, "stems_required")
    result = []
    for stem in selection:
        asset = project.assets.get(stem["audio_id"])
        if asset is None:
            raise ApiError(404, "audio_not_found")
        result.append((asset, stem["kind"]))
    return result


def validate_inputs(project, request, config):
    operation = request["operation"]
    if operation == "clone":
        return
    if "audio_id" in request and request["audio_id"] not in project.assets:
        raise ApiError(404, "audio_not_found")
    if operation in ("import", "reference", "import_stems"):
        uploads = [project.uploads.get(upload_id) for upload_id in request.get("upload_ids", [])]
        if not uploads or any(upload is None or upload.uploaded_bytes != upload.total_bytes for upload in uploads):
            raise ApiError(409, "complete_upload_required")
        if operation in ("import", "reference"):
            required = "original" if operation == "import" else "reference"
            if len(uploads) != 1 or uploads[0].role != required:
                raise ApiError(400, "upload_role_mismatch")
        elif not (all(upload.role == "stem" for upload in uploads)
                  or len(uploads) == 1 and uploads[0].role == "stem_bundle"):
            raise ApiError(400, "upload_role_mismatch")
    elif operation == "bundle":
        if "audio_id" in request:
            selected_audio(project, request)
        if "audio_id" not in request or request.get("include_stems", False):
            selected_stems(project, request)
    elif operation == "mix":
        selected_stems(project, request)
    elif operation not in ("history", "replace", "promote"):
        asset = selected_audio(project, request)
        if operation == "separate" and asset["frames"] * asset["channels"] * 4 > 512 * 1024**2:
            raise ApiError(413, "neural_pcm_limit")
    if operation in ("replace", "promote") and "audio_id" not in request:
        raise ApiError(400, "audio_required")
    if operation == "history" and not (project.redo if request.get("redo", False) else project.undo):
        raise ApiError(409, "history_unavailable")
    if operation == "replace" and (not project.preview_id or not any(stem["id"] == request["audio_id"] for stem in project.stems)):
        raise ApiError(409, "stem_required")
    if operation in ("export", "bundle", "render") and len(project.artifacts) >= MAX_ARTIFACTS:
        raise ApiError(413, "project_asset_limit")
    if operation == "master":
        reference_projection(project, request, config)


def metadata_command(project, command):
    request, operation = command.request, command.request["operation"]
    if operation == "history":
        source, destination = (project.redo, project.undo) if request.get("redo", False) else (project.undo, project.redo)
        destination.append(project.checkpoint())
        del destination[:-2]
        previous = source.pop()
        for key, value in previous.items():
            setattr(project, key, deepcopy(value))
    else:
        project.remember()
        project.input_id = request["audio_id"]
        project.preview_id = request["audio_id"]
        project.before_id = request["audio_id"]
        project.preview_is_mix = False
        project.edit_origin_kind = None
        project.master_id = None
        project.analysis = project.assets[request["audio_id"]].get("analysis")
    project.revision += 1
    command.state, command.progress, command.message = "completed", 1, "Ready"


def submit(store, project_id, value, principal=OWNER, client_key=None):
    request = command_request(value, store.config)
    with store.jobs.changed:
        project = store.get(project_id, principal)
        if store.jobs.stopping:
            raise ApiError(503, "service_stopping")
        if project.command and project.command.state in ("queued", "running"):
            raise ApiError(409, "command_in_progress")
        if project.uploads_active:
            raise ApiError(409, "upload_in_progress")
        if request["base_revision"] != project.revision:
            raise ApiError(409, "revision_conflict")
        metadata = request["operation"] in ("history", "promote")
        if not metadata and (store.config.studio_cli is None or store.config.ffmpeg is None):
            raise ApiError(503, "studio_unavailable")
        if request["operation"] == "separate" and store.config.stem_python is None:
            raise ApiError(503, "separation_unavailable")
        validate_inputs(project, request, store.config)
        if request["operation"] == "clone":
            source = store.get(request["source_project_id"], principal)
            if source.principal_id != project.principal_id:
                raise ApiError(404, "project_not_found")
            if request["audio_id"] not in source.assets:
                raise ApiError(404, "audio_not_found")
        if not principal.is_owner:
            if client_key is None:
                raise ApiError(400, "client_identity_required")
            store.jobs.sessions.start_command(principal, client_key)
        command_id = secrets.token_hex(16)
        command = ProjectCommand(command_id, project, request, project.directory / ("command-" + command_id))
        if metadata:
            metadata_command(project, command)
        project.command = command
        if not metadata:
            store.jobs.queue.append((project.id, command.id))
            store.jobs.changed.notify()
        return project.snapshot()
