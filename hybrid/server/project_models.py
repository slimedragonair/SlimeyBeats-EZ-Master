"""Private project data and the exact public manifest projection."""

from copy import deepcopy
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
import threading

from .project_limits import MAX_STORAGE_FILES

MANIFEST_FIELDS = ("original_id", "master_id", "preview_id", "reference_id", "input_id",
                   "before_id", "preview_is_mix", "edit_origin_kind",
                   "stems", "controls", "analysis", "report", "issues", "spectrogram")
READ_ONLY_OPERATIONS = frozenset(("analyze", "issues", "spectrogram", "suggest", "export", "bundle"))
ASSET_FIELDS = ("id", "role", "name", "frames", "sample_rate", "channels", "duration_seconds",
                "bytes", "content_type", "has_preview", "has_waveform", "kind", "analysis")


def public_asset(asset):
    value = {key: deepcopy(asset[key]) for key in ASSET_FIELDS if key in asset and key != "analysis"}
    if isinstance(asset.get("analysis"), dict):
        # The native reference/mastering paths retain the full private Analysis.
        # Public bank entries need meters; spectrum vectors belong to explicit
        # analyze results and report.output, not every asset in a large bank.
        from .project_reports import numeric
        value["analysis"] = {"metrics": numeric(asset["analysis"].get("metrics", {}))}
    return value


def public_artifact(artifact):
    return {key: artifact[key] for key in ("id", "name", "bytes", "content_type")}


@dataclass
class Upload:
    id: str
    extension: str
    total_bytes: int
    role: str
    path: Path
    source_name: str = "source.wav"
    kind: str | None = None
    uploaded_bytes: int = 0
    lock: threading.Lock = field(default_factory=threading.Lock)

    def snapshot(self):
        return {"id": self.id, "total_bytes": self.total_bytes,
                "uploaded_bytes": self.uploaded_bytes, "role": self.role}


@dataclass
class ProjectCommand:
    id: str
    project: object
    request: dict
    stage: Path
    state: str = "queued"
    progress: float = 0
    message: str = "Queued"
    error_code: str | None = None
    cancel: threading.Event = field(default_factory=threading.Event)
    done: threading.Event = field(default_factory=threading.Event)
    unreaped: bool = False
    result: dict | None = None
    artifact_id: str | None = None

    def __post_init__(self):
        self.done.set()

    @property
    def directory(self):
        return self.project.directory

    @property
    def deadline(self):
        return self.project.deadline

    def snapshot(self):
        value = {"id": self.id, "operation": self.request["operation"], "state": self.state,
                "base_revision": self.request["base_revision"], "progress": self.progress,
                "message": self.message, "error_code": self.error_code}
        if self.result is not None:
            value["result"] = deepcopy(self.result)
        if self.artifact_id:
            value["artifact_id"] = self.artifact_id
        return value


@dataclass
class Project:
    id: str
    principal_id: str
    directory: Path
    expires: float
    deadline: float
    max_disk_bytes: int
    revision: int = 0
    deleting: bool = False
    uploads: dict = field(default_factory=dict)
    assets: dict = field(default_factory=dict)
    artifacts: dict = field(default_factory=dict)
    command: ProjectCommand | None = None
    original_id: str | None = None
    input_id: str | None = None
    master_id: str | None = None
    preview_id: str | None = None
    reference_id: str | None = None
    reference_assets: set = field(default_factory=set)
    before_id: str | None = None
    preview_is_mix: bool = False
    edit_origin_kind: str | None = None
    stems: list = field(default_factory=list)
    controls: dict = field(default_factory=lambda: {"tracks": [], "output_gain_db": 0,
                                                   "protect_peaks": True, "ceiling_dbtp": -1})
    analysis: dict | None = None
    report: dict | None = None
    issues: dict | None = None
    spectrogram: dict | None = None
    undo: list = field(default_factory=list)
    redo: list = field(default_factory=list)
    downloads: int = 0
    uploads_active: int = 0
    downloads_done: threading.Event = field(default_factory=threading.Event)
    uploads_done: threading.Event = field(default_factory=threading.Event)
    max_storage_files: int = MAX_STORAGE_FILES

    def __post_init__(self):
        self.downloads_done.set()
        self.uploads_done.set()

    def checkpoint(self):
        return {key: deepcopy(getattr(self, key)) for key in MANIFEST_FIELDS}

    def remember(self):
        self.undo = (self.undo + [self.checkpoint()])[-2:]
        self.redo.clear()

    def snapshot(self):
        state = "deleting" if self.deleting else self.command.state if self.command and self.command.state in ("queued", "running") else "ready"
        return {"api_version": 2, "id": self.id, "revision": self.revision, "state": state,
                "expires_at": datetime.fromtimestamp(self.expires, timezone.utc).isoformat().replace("+00:00", "Z"),
                **self.checkpoint(), "assets": [public_asset(asset) for asset in self.assets.values()],
                "artifacts": [public_artifact(artifact) for artifact in self.artifacts.values()],
                "uploads": [upload.snapshot() for upload in self.uploads.values()],
                "command": self.command.snapshot() if self.command else None,
                "can_undo": bool(self.undo), "can_redo": bool(self.redo)}
