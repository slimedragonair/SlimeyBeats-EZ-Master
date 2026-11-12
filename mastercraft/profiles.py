"""Genre profiles: built-ins from the research brief (data/builtin_profiles.json) plus
user custom profiles stored as JSON in <app>/profiles/."""
import copy
import json
import re
from pathlib import Path

import numpy as np

from .paths import PROFILES_DIR

_DATA = Path(__file__).resolve().parent / "data" / "builtin_profiles.json"
_DB = json.loads(_DATA.read_text(encoding="utf-8-sig"))

BAND_CENTERS = list(_DB["band_centers_hz"])
TONAL = dict(_DB["tonal_correction"])
MODES = dict(_DB["modes"])
PLATFORMS = dict(_DB["platforms_lufs"])
DETECTORS = dict(_DB["detectors"])
BUILTIN = dict(_DB["profiles"])
BUILTIN_ORDER = ["neutral", "metal", "rock", "alt_rock", "metalcore_djent", "pop", "pop_punk", "hiphop_trap",
                 "edm", "synthwave_retro", "acoustic_folk", "country", "rnb_soul", "lofi", "game_ost",
                 "orchestral_cinematic"]
COMBO_PREFIX = "combo:"
# short names people actually type; also used when building blends
ALIASES = {
    "game": "game_ost", "ost": "game_ost", "soundtrack": "game_ost", "instrumental": "game_ost",
    "score": "game_ost", "cinematic": "orchestral_cinematic", "orchestral": "orchestral_cinematic",
    "synthwave": "synthwave_retro", "retro": "synthwave_retro", "synth": "synthwave_retro",
    "metalcore": "metalcore_djent", "djent": "metalcore_djent", "deathcore": "metalcore_djent",
    "alt": "alt_rock", "alternativerock": "alt_rock", "indie": "alt_rock", "punk": "pop_punk",
    "hiphop": "hiphop_trap", "trap": "hiphop_trap", "rap": "hiphop_trap", "rnb": "rnb_soul",
    "soul": "rnb_soul", "folk": "acoustic_folk", "acoustic": "acoustic_folk", "lofi": "lofi",
    "electronic": "edm", "dance": "edm", "house": "edm", "techno": "edm",
}
# why a blend exists: most real songs are a mix of two curves, and the user works in combos
# ("synth metal", "alt rock", "instrumental retro", "game tracks")
COMBO_EXAMPLES = ("metal:0.6+edm:0.4  (synth metal)", "rock:0.6+pop:0.4  (alt rock)",
                  "lofi:0.5+game_ost:0.5  (retro instrumental)", "game_ost:0.7+edm:0.3  (game tracks)")


def _slug(name):
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_") or "custom"


def _clean_disp(disp):
    return re.sub(r"\s*\((custom|blend)\)\s*$", "", disp.lower())


def list_profiles():
    """Returns ordered list of (key, display_name, is_custom)."""
    out = [(k, BUILTIN[k]["display_name"], False) for k in BUILTIN_ORDER if k in BUILTIN]
    if PROFILES_DIR.exists():
        for p in sorted(PROFILES_DIR.glob("*.json")):
            try:
                d = json.loads(p.read_text(encoding="utf-8-sig"))
                tag = "(blend)" if d.get("blend") else "(custom)"
                out.append(("custom:" + p.stem, d.get("display_name", p.stem) + " " + tag, True))
            except Exception:
                continue
    return out


def resolve_key(name):
    """Accept key, display name, custom name, or a blend spec (case-insensitive)."""
    if name is None:
        return "neutral"
    n = str(name).strip()
    if "+" in n or n.lower().startswith(COMBO_PREFIX):
        items = parse_combo(n)
        return COMBO_PREFIX + "+".join(f"{k}:{w:.3f}".rstrip("0").rstrip(".") for k, w in items)
    for k, disp, _ in list_profiles():
        if n.lower() in (k.lower(), disp.lower(), _clean_disp(disp), k.lower().replace("custom:", "")):
            return k
    s = _slug(n)
    for k, disp, _ in list_profiles():
        if s in (_slug(k.replace("custom:", "")), _slug(disp), _slug(_clean_disp(disp))):
            return k
    alias = ALIASES.get(s) or ALIASES.get(n.lower().replace(" ", "").replace("-", ""))
    if alias and alias in BUILTIN:
        return alias
    raise KeyError(f"Unknown profile '{name}'. Use --list-profiles. Blends: -p \"{COMBO_EXAMPLES[0]}\".")


def parse_combo(spec):
    """'metal:0.6+edm:0.4' or 'metal+edm' -> [(key, weight)] normalised to sum 1."""
    s = str(spec).strip()
    if s.lower().startswith(COMBO_PREFIX):
        s = s[len(COMBO_PREFIX):]
    parts = [p.strip() for p in s.split("+") if p.strip()]
    if len(parts) < 2:
        raise KeyError(f"A blend needs at least two profiles joined by '+', got '{spec}'. "
                       f"Examples: {', '.join(COMBO_EXAMPLES)}")
    items = []
    for part in parts:
        nm, _, w = part.partition(":")
        key = resolve_key(nm.strip())
        if key.startswith(COMBO_PREFIX):
            raise KeyError(f"A blend cannot contain another blend ('{nm}'). Use the base profiles instead.")
        try:
            weight = float(w) if w.strip() else 1.0
        except ValueError:
            raise KeyError(f"Weight '{w}' is not a number in '{part}' (use name:weight, e.g. metal:0.6).")
        if weight <= 0:
            raise KeyError(f"Weight for '{nm}' must be > 0.")
        items.append((key, weight))
    tot = sum(w for _, w in items)
    return [(k, w / tot) for k, w in items]


def _wavg(pairs, key, default=None):
    vals = [(p.get(key), w) for p, w in pairs if isinstance(p.get(key), (int, float)) and not isinstance(p.get(key), bool)]
    if not vals:
        return default
    tot = sum(w for _, w in vals)
    return round(sum(v * w for v, w in vals) / tot, 3) if tot else default


def _wavg_pair(pairs, key):
    """Weighted mean of a 2-element list value such as a correlation window."""
    vals = [(p[key], w) for p, w in pairs if isinstance(p.get(key), (list, tuple)) and len(p[key]) == 2]
    if not vals:
        return None
    tot = sum(w for _, w in vals) or 1.0
    return [round(sum(float(v[0]) * w for v, w in vals) / tot, 2),
            round(sum(float(v[1]) * w for v, w in vals) / tot, 2)]


def blend(items, spec=None):
    """Weighted blend of two or more profiles (tonal curve, loudness, dynamics, stereo, colour).

    Caps that protect punch are taken from the *most conservative* profile, so a blend never
    becomes looser than either half (blending Metal with EDM must not allow more harsh cuts
    than Metal allows)."""
    items = [(k, float(w)) for k, w in items]
    tot = sum(w for _, w in items) or 1.0
    pairs = [(get_profile(k), w / tot) for k, w in items]
    heavy = max(pairs, key=lambda pw: pw[1])[0]
    p = copy.deepcopy(heavy)
    p["tonal_db"] = [round(sum(float(pr["tonal_db"][i]) * w for pr, w in pairs), 2) for i in range(len(BAND_CENTERS))]
    p["lufs"] = {m: round(sum(float(pr["lufs"].get(m, pr["lufs"]["streaming_safe"])) * w for pr, w in pairs), 1)
                 for m in ("streaming_safe", "loud")}
    p["tp_dbtp"] = min(float(pr.get("tp_dbtp", -1.0)) for pr, _ in pairs)
    gl = {k: _wavg([(pr.get("glue", {}), w) for pr, w in pairs], k) for k in
          ("ratio", "attack_ms", "release_ms", "target_gr_db")}
    gl = {k: v for k, v in gl.items() if v is not None}
    gl["auto_release"] = float(sum((1.0 if pr.get("glue", {}).get("auto_release") else 0.0) * w for pr, w in pairs)) >= 0.5
    p["glue"] = gl
    mb = copy.deepcopy(heavy.get("multiband") or next((pr.get("multiband") for pr, _ in sorted(pairs, key=lambda pw: -pw[1])
                                                      if pr.get("multiband")), None))
    if mb:
        for b in ("low", "mid", "high"):
            if b in mb and mb[b]:
                for k in ("ratio", "max_gr_db", "attack_ms", "release_ms", "det_ms"):
                    v = _wavg([((pr.get("multiband") or {}).get(b) or {}, w) for pr, w in pairs], k)
                    if v is not None:
                        mb[b][k] = v
    p["multiband"] = mb
    p["limiter_max_gr_db"] = {m: round(float(np_mean([(pr.get("limiter_max_gr_db", {}).get(m), w) for pr, w in pairs], 4.0)), 1)
                              for m in ("streaming_safe", "loud")}
    st = {k: _wavg([(pr.get("stereo", {}), w) for pr, w in pairs], k)
          for k in ("max_side_gain_db", "side_hpf_hz", "mono_amount")}
    st = {k: v for k, v in st.items() if v is not None}
    cw = _wavg_pair([(pr.get("stereo", {}), w) for pr, w in pairs], "corr_window")
    if cw is not None:
        st["corr_window"] = cw
    p["stereo"] = st
    p["saturation"] = {k: _wavg([(pr.get("saturation", {}), w) for pr, w in pairs], k)
                       for k in ("drive_db", "mix") if _wavg([(pr.get("saturation", {}), w) for pr, w in pairs], k) is not None}
    for k in ("band", "style_lpf_hz"):
        if heavy.get("saturation", {}).get(k) is not None:
            p["saturation"][k] = heavy["saturation"][k]
    p["clipper_db"] = {m: round(float(np_mean([(pr.get("clipper_db", {}).get(m), w) for pr, w in pairs], 0.0)), 2)
                       for m in ("streaming_safe", "loud")}
    punc = {}
    keys = {k for pr, _ in pairs for k in (pr.get("punch") or {})}
    for k in keys:
        vals = [(pr["punch"][k], w) for pr, w in pairs if isinstance((pr.get("punch") or {}).get(k), (int, float))]
        if not vals:
            continue
        punc[k] = round(min(v for v, _ in vals), 3) if k in ("max_lowmid_boost_db", "max_low_shelf_boost_db",
                                                             "loud_max_limiter_gr_db") else _wavg([({k: v}, w) for v, w in vals], k)
    if punc:
        p["punch"] = punc
    names = " + ".join(f"{get_profile(k)['display_name']} {100 * w:.0f}%" for k, w in items)
    p["display_name"] = "Blend: " + names
    p["source"] = "blend of " + ", ".join(f"{k} ({w:.2f})" for k, w in items)
    p["blend"] = "+".join(f"{k}:{w:.3f}" for k, w in items)
    p.pop("key", None)
    p["key"] = COMBO_PREFIX + "+".join(f"{k}:{round(w, 3)}" for k, w in items) if spec is None else COMBO_PREFIX + spec
    return p


def np_mean(pairs, default):
    vals = [(v, w) for v, w in pairs if isinstance(v, (int, float)) and not isinstance(v, bool)]
    if not vals:
        return default
    tot = sum(w for _, w in vals)
    return sum(v * w for v, w in vals) / tot if tot else default


def get_profile(key):
    key = resolve_key(key)
    if key.startswith(COMBO_PREFIX):
        return blend(parse_combo(key))
    if key.startswith("custom:"):
        d = json.loads((PROFILES_DIR / (key[7:] + ".json")).read_text(encoding="utf-8-sig"))
        if d.get("blend"):
            base = blend(parse_combo(d["blend"]))
        else:
            base = copy.deepcopy(BUILTIN.get(d.get("base", "neutral"), BUILTIN["neutral"]))
        _deep_update(base, d)
        base["key"] = key
        base["display_name"] = d.get("display_name", base.get("display_name", key))
        return base
    p = copy.deepcopy(BUILTIN[key])
    p["key"] = key
    return p


def save_blend_profile(name, spec):
    """Save a blend as a custom profile (so it shows up in the GUI list and works everywhere)."""
    items = parse_combo(spec)
    pr = blend(items)
    d = {"display_name": name, "blend": "+".join(f"{k}:{round(w, 3)}" for k, w in items),
         "base": items[0][0], "schema": "master-craft.profile.v1"}
    PROFILES_DIR.mkdir(parents=True, exist_ok=True)
    path = PROFILES_DIR / (_slug(name) + ".json")
    path.write_text(json.dumps(d, indent=2), encoding="utf-8")
    return path, pr


def save_custom_profile(name, profile, settings=None, base=None):
    PROFILES_DIR.mkdir(parents=True, exist_ok=True)
    d = copy.deepcopy(profile)
    d.pop("key", None)
    d["display_name"] = name
    d["base"] = base or d.get("base") or "neutral"
    d["schema"] = "master-craft.profile.v1"
    if settings is not None:
        d["settings"] = settings
    path = PROFILES_DIR / (_slug(name) + ".json")
    path.write_text(json.dumps(d, indent=2), encoding="utf-8")
    return path


def delete_custom_profile(key):
    if key.startswith("custom:"):
        p = PROFILES_DIR / (key[7:] + ".json")
        if p.exists():
            p.unlink()


def _deep_update(dst, src):
    for k, v in src.items():
        if isinstance(v, dict) and isinstance(dst.get(k), dict):
            _deep_update(dst[k], v)
        else:
            dst[k] = v
    return dst


def target_curve_at(profile, freqs):
    """Interpolate octave target curve (dB re 1k octave band) at arbitrary freqs (log-f)."""
    lf = np.log2(np.asarray(BAND_CENTERS, float))
    return np.interp(np.log2(np.maximum(np.asarray(freqs, float), 1.0)), lf, np.asarray(profile["tonal_db"], float))


def suggest_genre(band_rel_db, valid_mask=None):
    """Nearest built-in tonal curve (RMS distance, after removing mean offset)."""
    m = np.asarray(band_rel_db, float)
    v = np.ones_like(m, bool) if valid_mask is None else np.asarray(valid_mask, bool)
    best = []
    for k in BUILTIN_ORDER:
        t = np.asarray(BUILTIN[k]["tonal_db"], float)
        d = (m - t)[v]
        d = d - d.mean()
        best.append((float(np.sqrt(np.mean(d ** 2))), k))
    best.sort()
    return [(BUILTIN[k]["display_name"], round(s, 2)) for s, k in best[:3]]


def suggest_blend(band_rel_db, valid_mask=None, max_terms=2, min_weight=0.15):
    """Best weighted mix of TWO built-in curves for a measured curve.

    Returns ([(key, weight)], residual_db) with the weights summing to 1, or ([], None) when no pair
    is clearly better than the best single curve. Many real tracks sit between two genres, so this
    matches how people actually work ("synth metal" = Metal + EDM). All 120 pairs are tried with a
    non-negative 2-variable fit: an unconstrained many-term fit invents pairs like
    "Acoustic/Folk + Metal" for a dark synth-metal track."""
    from scipy.optimize import nnls
    m = np.asarray(band_rel_db, float)
    v = np.ones_like(m, bool) if valid_mask is None else np.asarray(valid_mask, bool)
    keys = [k for k in BUILTIN_ORDER if k in BUILTIN]
    A = np.stack([np.asarray(BUILTIN[k]["tonal_db"], float)[v] for k in keys], axis=1)
    y = m[v]
    y = y - y.mean()
    A = A - A.mean(axis=0, keepdims=True)
    singles = np.sqrt(np.mean((A - y[:, None]) ** 2, axis=0))
    best = None
    for i in range(len(keys)):
        for j in range(i + 1, len(keys)):
            Aj = A[:, [i, j]]
            try:
                w2, _ = nnls(Aj, y)
            except Exception:
                continue
            s = float(w2.sum())
            if s <= 1e-9:
                continue
            w2 = w2 / s
            if float(w2.min()) < min_weight:
                continue
            r = float(np.sqrt(np.mean((Aj @ w2 - y) ** 2)))
            if best is None or r < best[0]:
                best = (r, keys[i], keys[j], w2)
    if best is None:
        return [], None
    res, k1, k2, w2 = best
    if res > 0.8 * float(singles.min()):
        return [], None
    return [(k1, round(float(w2[0]), 3)), (k2, round(float(w2[1]), 3))], round(res, 2)
