"""Execute v2 commands on the sole v1/v2 worker and commit complete revisions."""

from copy import copy, deepcopy
import json
import logging
import shutil
import time

from .errors import ProcessingError
from .processes import ProcessRunner
from .project_bank_edits import remix_bank, target_stem
from .project_commands import selected_audio
from .project_limits import processing_config
from .project_models import MANIFEST_FIELDS, READ_ONLY_OPERATIONS, public_asset
from .project_native_request import build_request, remove_inputs
from .project_outputs import bundle, collect_outputs
from .project_limits import MAX_ASSETS
from .project_retention import reclaim_commands, retain_history_assets
from .project_references import selected_reference_id
from .storage import directory_bytes, is_link
from .studio_driver import StudioDriver


class ProjectWorker:
    def __init__(self, config, store):
        self.config = processing_config(config)
        self.store = store
        self.runner = ProcessRunner(self.config)
        self.driver = StudioDriver(self.config, self.runner)

    def process(self, command):
        command.stage.mkdir(mode=0o700)
        if command.request["operation"] == "replace":
            result = remix_bank(command, self.config, self.runner, self.driver)
            directory_bytes(command.directory, self.config.max_job_disk_bytes, self.config.max_storage_files)
            return result
        if command.request["operation"] == "clone":
            from .project_media import lease
            with self.store.lock:
                source = self.store.get(command.request["source_project_id"])
                if source.principal_id != command.project.principal_id:
                    raise ProcessingError("Foreign clone")
            with lease(self.store, source.id, command.request["audio_id"], "audio") as (_, stream):
                with (command.stage / "cloned-input.wav").open("xb") as output:
                    while block := stream.read(65536):
                        if command.cancel.is_set() or source.deleting or source.deadline <= time.monotonic():
                            raise ProcessingError("Cancelled clone")
                        output.write(block)
        if command.request["operation"] == "bundle" and "audio_id" not in command.request:
            return bundle(command, self.config)
        request, temporary_inputs = build_request(command, self.config, self.runner)
        raw = self.driver.invoke(command, request)
        result = collect_outputs(command, raw, self.config, self.runner)
        if command.request["operation"] == "region_edit" and target_stem(command) is not None:
            result = remix_bank(command, self.config, self.runner, self.driver, result)
        if command.request["operation"] == "bundle":
            result = bundle(command, self.config, result[1])
        remove_inputs(temporary_inputs)
        (command.stage / "cloned-input.wav").unlink(missing_ok=True)
        directory_bytes(command.directory, self.config.max_job_disk_bytes, self.config.max_storage_files)
        return result

    def execute(self, command):
        state, code = "failed", "processing_failed"
        try:
            result = self.process(command)
            with self.store.lock:
                project = command.project
                if (command.cancel.is_set() or project.deleting or project.deadline <= time.monotonic()
                        or project.command is not command or project.revision != command.request["base_revision"]):
                    raise ProcessingError("Stale/cancelled command")
                self.commit(command, result)
                state, code = "completed", None
                command.state, command.progress, command.message = "completed", 1, "Ready"
                if not project.downloads:
                    try:
                        reclaim_commands(project)
                    except (OSError, ProcessingError):
                        logging.getLogger("slimeybeats.hybrid").error("Project retired-file cleanup requires retry")
        except Exception:
            if command.cancel.is_set() or command.deadline <= time.monotonic():
                state, code = "cancelled", "cancelled"
        finally:
            if state != "completed" and not command.unreaped:
                try:
                    if command.stage.exists():
                        if is_link(command.stage) or command.stage.parent != command.project.directory:
                            raise ProcessingError("Unsafe staging cleanup")
                        shutil.rmtree(command.stage)
                except (OSError, ProcessingError):
                    code = "cleanup_failed"
            if command.unreaped:
                self.store.jobs.stop()
            with self.store.lock:
                command.state, command.error_code = state, code
                command.message = "Ready" if state == "completed" else "Cancelled" if state == "cancelled" else "Processing failed"
                command.progress = 1 if state == "completed" else 0
                command.done.set()

    def commit(self, command, result):
        assets, artifacts, primary, updates = result
        project, request = command.project, command.request
        # Build and size-check the whole candidate before touching committed
        # state; the browser deliberately bounds JSON reads to one MiB.
        candidate = copy(project)
        candidate.assets, candidate.artifacts = dict(project.assets), dict(project.artifacts)
        candidate.undo, candidate.redo = deepcopy(project.undo), deepcopy(project.redo)
        candidate.reference_assets = set(project.reference_assets)
        candidate.stems, candidate.controls = deepcopy(project.stems), deepcopy(project.controls)
        command_result = updates.pop("_command_result", None)
        self.apply(candidate, request, assets, artifacts, primary, updates)
        retain_history_assets(candidate)
        if len(candidate.assets) > MAX_ASSETS:
            raise ProcessingError("Asset count limit")
        if len(json.dumps(candidate.snapshot(), allow_nan=False, separators=(",", ":")).encode()) > 1024 * 1024:
            raise ProcessingError("Manifest byte limit")
        for name in ("assets", "artifacts", "undo", "redo", "reference_assets", "revision", *MANIFEST_FIELDS):
            setattr(project, name, getattr(candidate, name))
        command.result = command_result
        command.artifact_id = next(reversed(artifacts), None)

    def apply(self, project, request, assets, artifacts, primary, updates):
        operation = request["operation"]
        target = selected_audio(project, request)["id"] if operation in (
            "voice_cleanup", "repair", "spectral_repair", "region_edit", "master", "render") else None
        origin = next((stem["kind"] for stem in project.stems if stem["id"] == target), None)
        if operation not in READ_ONLY_OPERATIONS:
            project.remember()
        project.assets.update(assets)
        project.artifacts.update(artifacts)
        if operation == "import" and request.get("as_preview", False):
            project.preview_id = primary
            project.before_id = project.original_id or primary
            project.preview_is_mix, project.edit_origin_kind = False, None
        elif operation in ("import", "clone"):
            project.original_id = project.input_id = primary
            project.preview_id = primary
            project.before_id = primary
            project.preview_is_mix, project.edit_origin_kind = False, None
            project.master_id = None
            project.report = project.issues = project.spectrogram = None
            project.stems = []
            project.controls = {"tracks": [], "output_gain_db": 0, "protect_peaks": True, "ceiling_dbtp": -1}
        elif operation == "reference":
            project.reference_id = primary
            project.reference_assets.add(primary)
            # Reference analysis belongs to its asset, not the main track display.
            updates.pop("analysis", None)
        elif operation == "master":
            project.master_id = project.preview_id = primary
            reference_id = selected_reference_id(project, request)
            if reference_id is not None:
                project.reference_assets.add(reference_id)
            project.before_id = target
            project.preview_is_mix, project.edit_origin_kind = False, None
        elif operation in ("separate", "import_stems"):
            bank_original_id = updates.pop("_bank_original_id", None)
            if bank_original_id and (operation == "import_stems" or project.original_id is None):
                project.original_id = project.input_id = bank_original_id
            if operation == "import_stems":
                project.master_id = None
                project.report = project.issues = project.spectrogram = None
            project.stems = [public_asset(asset) for asset in assets.values() if asset["role"] == "stem"]
            project.controls = {"tracks": [{"kind": stem["kind"], "gain_db": 0, "balance": 0, "mute": False, "solo": False}
                                            for stem in project.stems],
                                "output_gain_db": 0, "protect_peaks": True, "ceiling_dbtp": -1}
            project.preview_id = primary
            project.before_id = project.original_id
            project.preview_is_mix, project.edit_origin_kind = True, None
        elif primary is not None:
            project.preview_id = primary
            if operation == "mix":
                project.controls.update(request.get("mix", {}))
            replacements = updates.pop("_bank_replacements", None)
            if replacements:
                for index, stem in enumerate(project.stems):
                    if stem["kind"] in replacements:
                        updated = public_asset(project.assets[replacements[stem["kind"]]])
                        updated.update(role="stem", kind=stem["kind"], name=stem["name"])
                        project.stems[index] = updated
            if operation == "mix" or replacements:
                project.before_id = project.original_id
                project.preview_is_mix, project.edit_origin_kind = True, None
            else:
                project.before_id = target
                project.preview_is_mix, project.edit_origin_kind = False, origin
        for key, value in updates.items():
            setattr(project, key, value)
        project.revision += 1
