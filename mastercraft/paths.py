"""Application paths. Everything lives inside the app folder (never the system drive)."""
import os
from pathlib import Path

APP_DIR = Path(__file__).resolve().parent.parent
PROFILES_DIR = APP_DIR / "profiles"
# MASTERCRAFT_SETTINGS lets tests use their own file instead of the user's settings.json
SETTINGS_FILE = Path(os.environ.get("MASTERCRAFT_SETTINGS") or (APP_DIR / "settings.json"))
TMP_DIR = APP_DIR / "tmp"
OUTPUT_DIR = APP_DIR / "output"
# per-user data (EQ presets): NOT inside the install folder, so updates/reinstalls never touch it.
# Default: a sibling folder next to the app on the same drive (F:\Apps\Master-Craft-data);
# override with MASTERCRAFT_DATA_DIR.
DATA_DIR = Path(os.environ.get("MASTERCRAFT_DATA_DIR") or (APP_DIR.parent / f"{APP_DIR.name}-data"))
EQ_PRESETS_DIR = DATA_DIR / "eq_presets"


def _writable(d):
    """True if d can be created and a file written in it (probe file removed again)."""
    try:
        d.mkdir(parents=True, exist_ok=True)
        t = d / ".mc_write_test"
        t.write_bytes(b"ok"); t.unlink()
        return True
    except OSError:
        return False


def preset_dir_candidates():
    """v0.2.2 fallback chain for EQ presets, in order: MASTERCRAFT_DATA_DIR, the sibling data folder
    (F:\\Apps\\Master-Craft-data), a folder inside the app dir, and only as a last resort
    %APPDATA%\\Master-Craft (system drive; the GUI asks before using it). (label, path, last_resort)."""
    c = []
    if os.environ.get("MASTERCRAFT_DATA_DIR"):
        c.append(("MASTERCRAFT_DATA_DIR", Path(os.environ["MASTERCRAFT_DATA_DIR"]) / "eq_presets", False))
    c.append(("app data folder", APP_DIR.parent / f"{APP_DIR.name}-data" / "eq_presets", False))
    c.append(("inside the app folder", APP_DIR / "user_data" / "eq_presets", False))
    if os.environ.get("APPDATA"):
        c.append(("%APPDATA%", Path(os.environ["APPDATA"]) / "Master-Craft" / "eq_presets", True))
    return c


def resolve_preset_dir(candidates=None, allow_last_resort=None):
    """First writable folder of the chain. Returns (path or None, notice). notice is "" when the first
    choice works; otherwise it says what failed and which fallback is used. A last-resort candidate is
    only probed when allow_last_resort(path) returns True (the GUI asks the user first)."""
    cands = preset_dir_candidates() if candidates is None else candidates
    failed = []
    for i, (label, d, last) in enumerate(cands):
        if last and not (allow_last_resort and allow_last_resort(d)):
            failed.append(f"{d} (not used: needs your OK)")
            continue
        if _writable(d):
            if not failed:
                return d, ""
            return d, ("Presets folder not writable: " + "; ".join(failed) + f". Using fallback ({label}): {d}")
        failed.append(str(d))
    return None, "No writable presets folder: " + "; ".join(failed)


# disk cache of the auto masters the Manual EQ tab edits (one .npy per row, loaded on demand)
EQ_CACHE_DIR = TMP_DIR / "eqcache"


def ensure_dirs():
    for d in (PROFILES_DIR, TMP_DIR):
        d.mkdir(parents=True, exist_ok=True)


def setup_env():
    """Keep temp files / library caches inside the app folder."""
    ensure_dirs()
    t = str(TMP_DIR)
    for k in ("TEMP", "TMP", "TMPDIR"):
        os.environ[k] = t
    os.environ.setdefault("MPLCONFIGDIR", str(TMP_DIR / "mpl"))
    os.environ.setdefault("NUMBA_CACHE_DIR", str(TMP_DIR / "numba"))
    import tempfile
    tempfile.tempdir = t
