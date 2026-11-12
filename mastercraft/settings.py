"""All user-tunable settings with sensible defaults, persisted to <app>/settings.json."""
import copy
import json
import os
from pathlib import Path

from .paths import SETTINGS_FILE

# None = "use the genre profile / analysis value".
DEFAULTS = {
    "general": {
        "profile": "neutral",
        "mode": "streaming_safe",        # streaming_safe | loud
        "spotify_strict": False,          # loud mode: -2 dBTP
        "target_lufs": None,              # override profile/mode LUFS
        "ceiling_dbtp": None,             # override profile ceiling
        "intensity": 1.0,                 # 0..1.5 global scale for all corrective moves
        "format": "wav24",                # legacy single format (kept in sync with `formats`)
        "formats": [],                    # one or more of EXPORT_FORMATS; [] = use `format`
        "output_sr": 0,                   # 0 = keep source rate
        "out_dir": "",                    # "" = <app>/output
        "reference": "",                  # optional reference track to match tone/width/loudness to
        "suffix": "_master",
        "dither": True,
        "write_report": True,
    },
    "modules": {
        "headroom": {"enabled": True, "gain_db": -3.0},
        "declip": {"enabled": True, "max_run": 32},
        "cleanup": {"enabled": True, "hpf_hz": 22.0, "dc_remove": True, "polarity_fix": True},
        "denoise": {"enabled": True, "amount": 1.0, "split_hz": 4000.0, "prop_min": 0.3, "prop_max": 0.5,
                    "n_std": 1.75, "max_hf_drop_db": 2.0},
        "static_eq": {"enabled": True, "amount": 1.0, "tolerance_db": 2.0, "fraction": 0.6,
                      "max_db": 4.0, "max_boost_db": 3.0, "allow_boosts": True,
                      "tilt": True, "tilt_min_slope_db_oct": 0.75, "tilt_low_hz": 200.0,
                      "tilt_high_hz": 3000.0, "tilt_max_db": 3.5},
        "resonance": {"enabled": True, "amount": 1.0, "max_notches": 4, "q_min": 4.0, "q_max": 8.0,
                      "max_cut_db": 4.0},
        "low_end": {"enabled": True, "amount": 1.0, "shelf_hz": 90.0, "body_hz": 105.0, "max_shelf_db": 3.0,
                    "psycho_bass": True, "psycho_bass_db": -17.0},
        # v0.3: the only stages that ADD attack, so the low end still slaps after the mud cut,
        # the multiband, the glue and the limiter have all taken punch away
        "low_punch": {"enabled": True, "amount": 1.0,
                      "attack_db": 2.0, "sustain_db": 1.5, "max_attack_db": 3.0,
                      "band_lo_hz": 50.0, "band_hi_hz": 160.0, "attack_ms": 4.0, "release_ms": 70.0,
                      "sub_duck_db": 2.0, "max_duck_db": 2.5, "sub_lo_hz": 22.0, "sub_hi_hz": 60.0,
                      "duck_release_ms": 90.0},
        "dynamic_eq": {"enabled": True, "amount": 1.0,
                       "mud": True, "mud_max_db": 4.0, "mud_ratio": 2.0, "mud_attack_ms": 10.0, "mud_release_ms": 120.0,
                       "harsh": True, "harsh_lo_hz": 2000.0, "harsh_hi_hz": 5000.0, "harsh_max_db": 3.0,
                       "harsh_ratio": 3.0, "harsh_attack_ms": 4.0, "harsh_release_ms": 80.0, "harsh_thresh_over_median_db": 2.0,
                       "fizz": True, "fizz_lo_hz": 8000.0, "fizz_hi_hz": 14000.0, "fizz_max_db": 3.0,
                       "side_swirl": True, "swirl_max_db": 3.0},
        "deesser": {"enabled": True, "amount": 1.0, "lo_hz": 5000.0, "hi_hz": 10000.0, "max_db": 6.0,
                    "ratio": 4.0, "attack_ms": 1.0, "release_ms": 50.0, "thresh_over_median_db": 6.0},
        "hf_rebuild": {"enabled": False, "mix_db": -23.0, "drive_db": 9.0},
        "multiband": {"enabled": True, "amount": 1.0, "xover_hz": None, "attack_ms": 20.0,
                      "release_ms": 150.0, "knee_db": 6.0, "smart_timing": True},
        "glue": {"enabled": True, "amount": 1.0, "ratio": None, "attack_ms": None, "release_ms": None,
                 "target_gr_db": None, "knee_db": 6.0},
        "saturation": {"enabled": True, "amount": 1.0, "drive_db": None, "mix": None, "oversample": 2,
                       "asymmetry": 0.08, "style_lpf": True},
        "stereo": {"enabled": True, "amount": 1.0, "mono_bass_hz": None, "max_side_gain_db": None,
                   "widen_above_hz": 300.0, "allow_narrowing": True},
        "clipper": {"enabled": True, "drive_db": None},   # None = profile value per mode (0 = off)
        # v0.3: match the tone (and optionally loudness/width) of a reference track instead of the
        # profile curve. The profile still supplies dynamics, punch caps and clipper.
        "reference_match": {"enabled": True, "strength": 1.0, "max_db": 6.0, "taps": 4097,
                            "realize": "biquads", "match_width": True, "match_loudness": False},
        "limiter": {"enabled": True, "lookahead_ms": 2.0, "release_ms": 30.0, "release_slow_ms": 250.0, "oversample": 4,
                    "safety_margin_db": 0.2, "max_gr_abort_db": 6.0, "iterations": 6, "tolerance_lu": 0.2,
                    "max_makeup_db": 18.0},   # QA N2: never raise a file by more than this
    },
    # transient protection: drum hits detected on the input; dynamic EQ / de-esser / denoise /
    # saturation do not act on them
    "transients": {"protect": True, "post_ms": 40.0},
    "safety": {"enabled": True, "max_retries": 2, "backoff": 0.6, "worse_by": 15.0,
               "punch_check": True, "punch_max_loss_db": 2.0, "punch_max_smeared_pct": 15.0},
}


def _merge(dst, src):
    for k, v in (src or {}).items():
        if isinstance(v, dict) and isinstance(dst.get(k), dict):
            _merge(dst[k], v)
        elif k in dst or not isinstance(dst, dict) or True:
            dst[k] = v
    return dst


def defaults():
    return copy.deepcopy(DEFAULTS)


# Settings schema version. The GUI saves the whole dict, so every default becomes a stored value;
# when a default is retuned, a migration moves stored values that still equal the OLD default to
# the new one (user-changed values are left alone). The version is written on the next save.
SETTINGS_VERSION = 2
MIGRATIONS = {
    # 519b6b1 transient fix: limiter retune (QA M1)
    2: [("modules.limiter.lookahead_ms", 3.0, 2.0),
        ("modules.limiter.release_ms", 100.0, 30.0)],
}
LAST_MIGRATION = []   # [(key, old, new)] applied by the last load()/load_file()


def _same(a, b):
    try:
        return abs(float(a) - float(b)) < 1e-9
    except (TypeError, ValueError):
        return a == b


def migrate(raw):
    """Upgrade a raw settings dict (as read from JSON) in place to SETTINGS_VERSION.
    Returns (raw, changes) where changes = [(dotted_key, old, new)]. 'settings_version' is removed
    from the dict (it is file metadata, written back by save())."""
    changes = []
    if not isinstance(raw, dict):
        return raw, changes
    try:
        ver = int(raw.get("settings_version", 1))
    except (TypeError, ValueError):
        ver = 1
    for target in sorted(MIGRATIONS):
        if ver >= target:
            continue
        for key, old, new in MIGRATIONS[target]:
            parts = key.split(".")
            d = raw
            for p in parts[:-1]:
                d = d.get(p) if isinstance(d, dict) else None
            if isinstance(d, dict) and parts[-1] in d and _same(d[parts[-1]], old):
                d[parts[-1]] = new
                changes.append((key, old, new))
    raw.pop("settings_version", None)
    return raw, changes


def preview_migration(path):
    """Read-only: which values migrate() would change in the settings file at `path`."""
    raw = json.loads(Path(path).read_text(encoding="utf-8-sig"))
    return migrate(raw)[1]


def merged(overrides=None):
    s = defaults()
    return _merge(s, overrides or {})


def load_file(path):
    """Defaults + the (migrated) settings JSON at `path`. Raises on unreadable / invalid JSON."""
    global LAST_MIGRATION
    raw = json.loads(Path(path).read_text(encoding="utf-8-sig"))
    if not isinstance(raw, dict):
        raise ValueError("settings file must contain a JSON object")
    raw, LAST_MIGRATION = migrate(raw)
    return _merge(defaults(), raw)


# QA N9: a corrupt / unreadable settings file is never overwritten silently. load() backs it up,
# falls back to in-memory defaults and blocks autosave until the user explicitly saves or resets.
LOAD_ERROR = None        # readable message when the settings file could not be used
BACKUP_PATH = None       # where the unusable file was copied
AUTOSAVE_BLOCKED = False


def _backup_path(p):
    p = Path(p)
    cand = p.with_name(p.name + ".bad")
    if not cand.exists():
        return cand
    import time
    stamp = time.strftime("%Y%m%d-%H%M%S")
    cand = p.with_name(f"{p.name}.{stamp}.bad")
    k = 2
    while cand.exists():
        cand = p.with_name(f"{p.name}.{stamp}-{k}.bad")
        k += 1
    return cand


def load():
    global LAST_MIGRATION, LOAD_ERROR, BACKUP_PATH, AUTOSAVE_BLOCKED
    LAST_MIGRATION = []
    LOAD_ERROR, BACKUP_PATH, AUTOSAVE_BLOCKED = None, None, False
    if not SETTINGS_FILE.exists():
        return defaults()
    try:
        return load_file(SETTINGS_FILE)
    except Exception as e:  # noqa: corrupt JSON, not an object, unreadable, bad encoding ..
        AUTOSAVE_BLOCKED = True
        LOAD_ERROR = f"{SETTINGS_FILE.name} could not be read ({type(e).__name__}: {e})."
        try:
            import shutil
            bp = _backup_path(SETTINGS_FILE)
            shutil.copy2(SETTINGS_FILE, bp)   # copy, never move: the original stays untouched too
            BACKUP_PATH = bp
            LOAD_ERROR += f" A copy was saved as {bp.name}."
        except Exception as e2:  # noqa
            LOAD_ERROR += f" It could not be backed up ({e2})."
        LOAD_ERROR += (" Running on built-in defaults; the file will not be overwritten until you save "
                       "or reset the settings yourself.")
        return defaults()


def save(s, explicit=False):
    """Write the settings file. Autosaves (explicit=False) are skipped while the file on disk could
    not be read (AUTOSAVE_BLOCKED); an explicit save/reset writes it and lifts the block.
    Returns True when the file was written."""
    global AUTOSAVE_BLOCKED
    if AUTOSAVE_BLOCKED and not explicit:
        return False
    AUTOSAVE_BLOCKED = False
    out = copy.deepcopy(s)
    out.pop("settings_version", None)
    out = {"settings_version": SETTINGS_VERSION, **out}
    tmp = SETTINGS_FILE.with_name(SETTINGS_FILE.name + f".{os.getpid()}.tmp")
    tmp.write_text(json.dumps(out, indent=2), encoding="utf-8")
    os.replace(tmp, SETTINGS_FILE)  # atomic: a crash mid-write never leaves a truncated settings file
    return True


def resolve_targets(settings, profile):
    """Returns (target_lufs, ceiling_dbtp) after mode/profile/overrides."""
    g = settings["general"]
    mode = g.get("mode", "streaming_safe")
    lufs = profile["lufs"].get(mode, profile["lufs"]["streaming_safe"])
    tp = float(profile.get("tp_dbtp", -1.0))
    if mode == "loud" and g.get("spotify_strict"):
        tp = -2.0
    if g.get("target_lufs") is not None:
        lufs = float(g["target_lufs"])
    if g.get("ceiling_dbtp") is not None:
        tp = float(g["ceiling_dbtp"])
    return float(lufs), float(min(tp, 0.0))
