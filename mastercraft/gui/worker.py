"""Background processing (QThread) so the UI never blocks."""
import atexit
import copy
import os
import time
import traceback
import uuid
from pathlib import Path

import numpy as np
import soundfile as sf
from PySide6.QtCore import QObject, Signal, Slot

from ..paths import TMP_DIR, OUTPUT_DIR
from .. import loudness as LN


# A/B preview files: unique per process + run so two app instances (or two runs) never write or
# delete each other's files; removed on exit, and stale ones (> 24 h, e.g. after a crash) at startup.
_SESSION = f"{os.getpid()}_{uuid.uuid4().hex[:8]}"
_PREVIEWS = {}          # (row, tag) -> current path
_OLD_PREVIEWS = []      # superseded files that were still open by the player


def _try_unlink(p):
    try:
        Path(p).unlink()
        return True
    except FileNotFoundError:
        return True
    except OSError:
        return False


def cleanup_previews():
    """Delete this process's preview files (call after the player released them)."""
    for p in list(_PREVIEWS.values()) + list(_OLD_PREVIEWS):
        _try_unlink(p)
    _PREVIEWS.clear()
    _OLD_PREVIEWS.clear()


_MASTERS = {}          # row -> cached master .npy of this process
_OLD_MASTERS = []


def cache_master(y, row):
    """QA v0.2.0 #4: the auto master for the EQ tab goes to disk (float32 .npy), not into the payload."""
    from ..paths import EQ_CACHE_DIR
    EQ_CACHE_DIR.mkdir(parents=True, exist_ok=True)
    p = EQ_CACHE_DIR / f"master_{_SESSION}_{row}_{uuid.uuid4().hex[:6]}.npy"
    np.save(str(p), np.ascontiguousarray(y, dtype=np.float32))
    old = _MASTERS.get(row)
    if old and not _try_unlink(old):
        _OLD_MASTERS.append(old)
    _MASTERS[row] = str(p)
    return str(p)


def load_cached_master(path):
    return np.load(str(path), mmap_mode=None, allow_pickle=False)


def cleanup_masters():
    for p in list(_MASTERS.values()) + list(_OLD_MASTERS):
        _try_unlink(p)
    _MASTERS.clear()
    _OLD_MASTERS.clear()


def purge_stale_previews(max_age_h=24.0):
    """Remove preview / cached-master files left behind by crashed runs (older than max_age_h)."""
    from ..paths import EQ_CACHE_DIR
    cut = time.time() - max_age_h * 3600
    try:
        for p in list(TMP_DIR.glob("preview_*.wav")) + list(EQ_CACHE_DIR.glob("master_*.npy")):
            try:
                if p.stat().st_mtime < cut:
                    p.unlink()
            except OSError:
                pass
    except OSError:
        pass


atexit.register(cleanup_previews)
atexit.register(cleanup_masters)


class Job:
    def __init__(self, row, path, settings, analyze_only=False):
        self.row, self.path, self.settings, self.analyze_only = row, path, settings, analyze_only


class Worker(QObject):
    progress = Signal(int, int, str)        # row, pct, message
    done = Signal(int, dict)                # row, payload
    failed = Signal(int, str)
    idle = Signal()

    def __init__(self):
        super().__init__()
        self._cancel = False

    def cancel(self):
        self._cancel = True

    @Slot(list)
    def run_jobs(self, jobs):
        from ..audio_io import load_audio_ex, EXPORT_FORMATS, AudioLoadError, unique_output_path
        from ..analysis import analyze
        from ..engine import master, strip_internal, Cancelled
        from ..postprocess import finalize_exports
        from ..profiles import get_profile
        from .widgets import envelope
        self._cancel = False
        used = set()  # output names of this batch (QA M2: same stem from two folders)
        ref_cache = {}  # reference path -> analysed target (one analysis per batch)
        for job in jobs:
            if self._cancel:
                self.failed.emit(job.row, "Cancelled")
                continue
            try:
                s = job.settings
                prof = get_profile(s["general"]["profile"])
                self.progress.emit(job.row, 1, "loading")
                x, sr, load_warn = load_audio_ex(job.path)
                pr = lambda p, m: self.progress.emit(job.row, int(p), m)
                payload = {"path": job.path, "sr": sr, "duration": x.shape[0] / sr, "env_before": envelope(x)}
                if job.analyze_only:
                    rep = analyze(x, sr, prof, job.path, progress=pr)
                    rep["warnings"] = load_warn + rep.get("warnings", [])
                    payload["input"] = strip_internal(rep)
                    payload["preview_before"] = self._preview(x, sr, job.row, "before", 0.0)
                else:
                    ref = None
                    rp = (s["general"].get("reference") or "").strip()
                    if rp:
                        from .. import reference as REF
                        if rp not in ref_cache:
                            try:
                                ref_cache[rp] = REF.load_target(rp)
                            except Exception as e:      # noqa: a bad reference must not stop the batch
                                ref_cache[rp] = None
                                load_warn.append(f"reference not used ({rp}): {e}")
                        ref = ref_cache[rp]
                    y, res = master(x, sr, s, job.path, progress=lambda p, m: pr(min(p, 97), m),
                                    cancel=lambda: self._cancel, reference=ref)
                    res["notes"] = load_warn + res["notes"]
                    g = s["general"]
                    out_dir = Path(g.get("out_dir") or OUTPUT_DIR)
                    suffix = (g.get("suffix") if g.get("suffix") is not None else "_master").strip()
                    self.progress.emit(job.row, 98, "exporting")
                    exports = finalize_exports(y, sr, out_dir, Path(job.path).stem, suffix, g,
                                               res["targets"]["ceiling_dbtp"], used=used)
                    res["exports"] = [e for _p, e in exports]
                    res["export"] = exports[0][1]          # the primary file (first ticked format)
                    from ..postprocess import export_notes
                    from ..audio_io import report_path_for
                    for _p, e in exports:
                        res["notes"] += export_notes(e, res["targets"])
                    if g.get("write_report", True):
                        from ..cli import dump_json
                        for op_, e in exports:            # one report per written file
                            dump_json({**res, "export": e}, report_path_for(op_))
                    payload.update(res)
                    payload["out_path"] = str(exports[0][0])
                    payload["out_paths"] = [str(p) for p, _e in exports]
                    # for the Manual EQ tab: the auto master as it was before export (float32 to save memory)
                    payload["y_master_path"] = cache_master(y, job.row)
                    payload["sr"] = int(sr)
                    payload["general"] = dict(g)
                    payload["env_after"] = envelope(y)
                    # loudness-matched previews: both played at the quieter of the two loudnesses
                    li, lo = res["input"]["metrics"]["integrated_lufs"], res["output"]["metrics"]["integrated_lufs"]
                    li = li if np.isfinite(li) else -70.0
                    lo = lo if np.isfinite(lo) else -70.0
                    ref = min(li, lo)
                    payload["preview_before"] = self._preview(x, sr, job.row, "before", ref - li)
                    payload["preview_after"] = self._preview(y, sr, job.row, "after", ref - lo)
                    payload["match_db"] = {"before": round(ref - li, 2), "after": round(ref - lo, 2)}
                self.progress.emit(job.row, 100, "done")
                self.done.emit(job.row, payload)
            except Cancelled:
                self.failed.emit(job.row, "Cancelled")
            except AudioLoadError as e:  # readable, no traceback (QA M3)
                self.failed.emit(job.row, str(e))
            except Exception as e:  # noqa
                self.failed.emit(job.row, f"{type(e).__name__}: {e}\n\n{traceback.format_exc()}")
        self.idle.emit()

    @staticmethod
    def _preview(x, sr, row, tag, gain_db):
        TMP_DIR.mkdir(parents=True, exist_ok=True)
        # new name every time: the player may still hold the previous file open
        p = TMP_DIR / f"preview_{_SESSION}_{row}_{tag}_{uuid.uuid4().hex[:6]}.wav"
        y = np.clip(x * 10 ** (gain_db / 20), -1, 1)
        sf.write(str(p), y, sr, subtype="PCM_16")
        old = _PREVIEWS.get((row, tag))
        if old and not _try_unlink(old):
            _OLD_PREVIEWS.append(old)
        _PREVIEWS[(row, tag)] = str(p)
        return str(p)
