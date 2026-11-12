"""Analysis-driven mastering chain with post-process safety check.

Chain order (user spec + research brief 3.2):
  headroom -> declip -> DC/HPF/polarity -> HF denoise (only if noisy) -> [HF rebuild opt]
  -> corrective EQ (tonal curve, mud, resonances, low end) -> dynamic EQ (mud/harsh/fizz/side swirl)
  -> de-esser -> multiband -> glue -> saturation -> stereo (mono-safe bass, M/S width)
  -> true-peak limiter iterated to target LUFS -> TP verification.
Then the output is re-analysed; if it got worse/more distorted the chain is re-run at lower
intensity (keeping the best attempt)."""
import copy
import time
from pathlib import Path

import numpy as np

from . import loudness as LN
from .analysis import analyze, _stft_power, _band_power, mud_cut_db
from .profiles import TONAL, BAND_CENTERS, DETECTORS as DET, get_profile, MODES
from .settings import resolve_targets
from .dsp import filters as F
from .dsp.dynamics import dynamic_band, glue_compress, compress, auto_threshold
from .dsp.limiter import limit_to_target
from .dsp import lowend as LE
from .dsp import repair as RP
from .dsp import tone as TN
from . import reference as RF

SAFETY_KEYS = ("harshness", "sibilance", "fizz", "clipping", "shimmer", "resonances", "mud")


class Cancelled(Exception):
    pass


def _mud_dyn_depth(rep, S, pp, I):
    """Max depth (dB) of the dynamic mud band; grows with severity, capped per profile (mud_scale)."""
    de = S["dynamic_eq"]
    sev = _sev(rep, "mud")
    sev_k = float(np.interp(sev, [30, 70, 100], [1.0, 1.2, 1.4]))
    return float(de["mud_max_db"] * min(I * de["amount"], 1.25) * pp.get("mud_scale", 1.0) * sev_k)


MUD_DYN_SHARE = 0.25     # share of the total mud cut left to the dynamic band (QA v0.2.0 M8)


def _mud_dyn_band_db(rep, S, pp, I, A_eq):
    """Dynamic mud band depth: the remainder of the total cut. A dynamic band works part of the
    time, so its depth is twice the remainder it should remove on average, never more than the
    old severity-based depth and never under 1 dB."""
    mud = _issue(rep, "mud")["details"]
    tot = mud_cut_db(mud.get("score_db", mud["excess_db"]), DET["mud"]["max_cut_db"]) * min(A_eq, 1.25)
    return float(min(_mud_dyn_depth(rep, S, pp, I), max(2.0 * MUD_DYN_SHARE * tot, 1.0)))


def _sev(rep, key):
    for i in rep["issues"]:
        if i["key"] == key:
            return i["severity"]
    return 0.0


def _issue(rep, key):
    for i in rep["issues"]:
        if i["key"] == key:
            return i
    return {"severity": 0.0, "details": {}}


# ------------------------------------------------------------------ low-mid guard
LOWMID_GUARDS = ((100.0, -1.8), (125.0, -2.0), (160.0, -2.6))
LOWMID_GRID = (63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0)


def _sos_of(f, sr):
    """(type, freq, gain, q) -> biquad SOS, same recipe the execution stage uses."""
    kind, fc, g, q = f[0], f[1], f[2], f[3]
    if kind == "bell":
        return F.peaking(fc, g, q, sr)
    return F.shelf(fc, g, sr, "low" if kind == "low_shelf" else "high", q)


def _lowmid_response(filters, sr, freqs=LOWMID_GRID):
    """Cascade magnitude response of the planned EQ at the low-mid grid (dB)."""
    freqs = np.asarray(freqs, float)
    if not filters:
        return np.zeros(freqs.size)
    return F.eq_response_db([_sos_of(f, sr) for f in filters], freqs, sr)


def _guard_low_mid(filters, sr):
    """Stop the corrective EQ from hollowing the kick body.

    Muddiness sits at 150-500 Hz and the kick/bass punch zone is 80-200 Hz, so a wide mud
    bell plus the low-end bell can scoop the punch out of the mix while the sub stays (or
    grows): the classic "muffled lows that do not slap". After the low cuts are planned, the
    cascade response is measured on a low-mid grid; where it takes more than the floor below
    out of 100-160 Hz, every low cut (< 300 Hz) is scaled back together (never to nothing,
    at least 50 % of the planned depth) and the amount is reported."""
    freqs = np.asarray(LOWMID_GRID, float)
    resp = _lowmid_response(filters, sr, freqs)
    actions = []
    for guard_hz, floor_db in LOWMID_GUARDS:
        i = int(np.where(np.isclose(freqs, guard_hz))[0][0])
        if resp[i] >= floor_db:
            continue
        short = float(floor_db - resp[i])
        cuts = [k for k, f in enumerate(filters) if f[2] < 0 and f[1] < 300.0]
        if not cuts:
            continue
        total = float(-sum(filters[k][2] for k in cuts))
        if total <= 1e-6:
            continue
        factor = float(max(0.5, min(1.0, 1.0 - short / total)))
        if factor >= 0.995:
            continue
        for k in cuts:
            t, fc, g, q = filters[k][:4]
            why = filters[k][4] if len(filters[k]) > 4 else ""
            filters[k] = (t, fc, round(float(g) * factor, 2), q, why + f" (halved for kick-body guard)" if factor < 0.9 else why)
        resp = _lowmid_response(filters, sr, freqs)
        actions.append({"freq_hz": guard_hz, "over_by_db": round(short, 2), "scale": round(factor, 2)})
    return filters, actions, {f"{int(f)}": round(float(v), 2) for f, v in zip(freqs, resp)}



# ------------------------------------------------------------------ planning
def plan(rep, profile, settings, intensity_scale=1.0, reference=None):
    """Turn analysis into concrete module parameters, each with a reason."""
    S = settings["modules"]
    I = float(np.clip(settings["general"]["intensity"], 0, 1.5)) * intensity_scale
    it = rep["_internal"]
    sr = rep["sr"]
    p = {"intensity": round(I, 3), "steps": []}
    art = rep.get("artifact_severity", 0)
    warm = art >= 50  # brief 5: high artifact severity -> no HF boosts, halve saturation
    pp = profile.get("punch", {}) or {}
    tr = settings.get("transients", {"protect": True, "post_ms": 40.0})
    prot = {"protect": bool(tr.get("protect", True)) and bool(it.get("hits")), "protect_ms": float(tr.get("post_ms", 40.0))}
    ref_rm = (S.get("reference_match") or {})
    rmatch = bool(reference) and bool(ref_rm.get("enabled", True)) and it.get("ref_src") is not None

    def step(name, reason, **kw):
        p["steps"].append({"module": name, "reason": reason, **kw})

    if S["headroom"]["enabled"]:
        step("headroom", "Restore float headroom before processing", gain_db=S["headroom"]["gain_db"])
    runs = it["clip_runs"]
    if S["declip"]["enabled"] and runs and _sev(rep, "clipping") >= 15:
        step("declip", f"{len(runs)} clipped runs", max_run=S["declip"]["max_run"])
    c = S["cleanup"]
    if c["enabled"]:
        flip = bool(c["polarity_fix"] and rep.get("stereo") and rep["stereo"]["corr"] < DET["stereo"]["polarity_flip_corr"])
        step("cleanup", "DC removal + 22 Hz rumble HPF" + (" + polarity fix" if flip else ""),
             hpf_hz=c["hpf_hz"], dc=c["dc_remove"], flip=flip)
    dn = S["denoise"]
    sn = _sev(rep, "noise")
    if dn["enabled"] and sn >= 30 and _issue(rep, "noise")["details"].get("triggered"):
        frac = np.clip((sn - 30) / 60, 0, 1)
        prop = (dn["prop_min"] + (dn["prop_max"] - dn["prop_min"]) * frac) * min(I, 1.0) * dn["amount"]
        step("denoise", f"Hiss severity {sn:.0f}", prop=round(float(np.clip(prop, 0.05, 0.6)), 3),
             n_std=dn["n_std"], split_hz=dn["split_hz"], max_hf_drop_db=dn["max_hf_drop_db"], **prot)
    hr = S["hf_rebuild"]
    if hr["enabled"] and it["cutoff_hz"] and it["cutoff_hz"] < 16500:
        step("hf_rebuild", f"Band-limited at {it['cutoff_hz']:.0f} Hz", fc=it["cutoff_hz"], mix_db=hr["mix_db"], drive_db=hr["drive_db"])

    # ---- static corrective EQ
    eq = S["static_eq"]
    filters = []
    cutoff = it["cutoff_hz"] or sr / 2
    if eq["enabled"]:
        A = I * eq["amount"]
        dev, valid = np.asarray(it["dev"], float), np.asarray(it["valid"], bool)
        mud_on = _sev(rep, "mud") >= 30
        le = _issue(rep, "low_end")["details"]
        # with a reference track the reference IS the tonal target: the profile correction and the
        # bass bell would fight the match, so only local problems (mud, resonances) remain here
        low_on = (S["low_end"]["enabled"] and not rmatch
                  and (le.get("deficit_db", 0) > DET["low_end"]["deficit_flag_db"] or le.get("excess_db", 0) > 3))
        # ---- tilt first (research2 1.4 / 3.1 step 5): one straight line explains most of the
        # deviation on real mixes, so fix the slope with two shelves and leave the bells only the
        # residual. Bells fighting a slope is what makes a 10-band corrector sound "pumped".
        tilt = {"enabled": bool(eq.get("tilt", True)) and not rmatch, "slope_db_per_oct": 0.0, "applied_db": 0.0}
        if tilt["enabled"]:
            lf = np.log2(np.asarray(BAND_CENTERS, float))
            core_m = valid & (np.asarray(BAND_CENTERS) >= 125) & (np.asarray(BAND_CENTERS) <= 8000)
            if int(core_m.sum()) >= 3:
                slope = float(np.polyfit(lf[core_m], dev[core_m], 1)[0])
                tilt["slope_db_per_oct"] = round(slope, 2)
                if abs(slope) > float(eq.get("tilt_min_slope_db_oct", 0.75)):
                    cap = float(eq.get("tilt_max_db", 3.5))
                    lo_hz, hi_hz = float(eq.get("tilt_low_hz", 200.0)), float(eq.get("tilt_high_hz", 3000.0))
                    gl = float(np.clip(-slope * np.log2(lo_hz / 1000.0), -cap, cap)) * eq["fraction"] * A
                    gh = float(np.clip(-slope * np.log2(hi_hz / 1000.0), -cap, cap)) * eq["fraction"] * A
                    if warm:
                        gh = min(gh, 0.0)                      # no top-end boost on artifact-heavy material
                    if abs(gl) >= 0.3:
                        filters.append(("low_shelf", lo_hz, gl, 0.7, f"tilt {slope:+.2f} dB/oct (bass vs 1 kHz)"))
                    if abs(gh) >= 0.3:
                        filters.append(("high_shelf", hi_hz, gh, 0.7, f"tilt {slope:+.2f} dB/oct (top vs 1 kHz)"))
                    if filters:
                        dev = dev - F.eq_response_db([_sos_of(f, sr) for f in filters],
                                                     np.asarray(BAND_CENTERS, float), sr)
                        tilt["applied_db"] = round(float(gl + gh), 2)
        for i, fc in enumerate(BAND_CENTERS):
            if rmatch:
                break                      # the reference match owns the broad tonal balance
            if not valid[i] or abs(dev[i]) <= eq["tolerance_db"]:
                continue
            if mud_on and fc in (125, 250, 500) and it["mud_fc"] / 1.7 <= fc <= it["mud_fc"] * 1.7:
                continue  # the mud cut below owns this region
            if low_on and fc in (31.5, 63, 125):
                continue
            if fc == 31.5 and dev[i] < 0:
                continue  # never boost the sub-bass band (brief 1.8)
            g = -np.sign(dev[i]) * min(eq["fraction"] * (abs(dev[i]) - 0.0), eq["max_db"]) * A
            g = float(np.clip(g, -eq["max_db"], min(eq["max_db"], eq.get("max_boost_db", eq["max_db"]))))
            if g > 0 and 160 <= fc <= 600 and "max_lowmid_boost_db" in pp:
                g = min(g, pp["max_lowmid_boost_db"])  # low-mid boosts sit under kick/snare body: boxy, slower hits
            if g > 0 and (not eq["allow_boosts"] or (warm and fc >= 4000) or fc >= cutoff - 500 or fc * np.sqrt(2) > cutoff):
                continue
            if fc <= 63:
                filters.append(("low_shelf", fc * 1.4, g, 0.7, f"{fc:g} Hz band {dev[i]:+.1f} dB vs target"))
            elif fc >= 8000:
                filters.append(("high_shelf", fc / 1.4, g, 0.7, f"{fc:g} Hz band {dev[i]:+.1f} dB vs target"))
            else:
                filters.append(("bell", fc, g, 0.9, f"{fc:g} Hz band {dev[i]:+.1f} dB vs target"))
        mud = _issue(rep, "mud")["details"]
        if mud_on:
            # QA v0.1.1 M8: the cut tracks the detected centre and excess. Total wanted cut =
            # 75 % of the excess above 1 dB (bounded); when the dynamic mud band also runs, the
            # static bell takes what that band is not expected to remove (about half its depth).
            # QA v0.2.0 M8: total cut from the mud score (target distance + local prominence); the
            # static bell at the detected centre takes most of it, the dynamic band only the rest.
            score = mud.get("score_db", mud["excess_db"])
            want = mud_cut_db(score, DET["mud"]["max_cut_db"]) * min(A, 1.25)
            dyn_on = S["dynamic_eq"]["enabled"] and S["dynamic_eq"]["mud"] and it["mud_var"] > 2
            if dyn_on:
                want *= 1.0 - MUD_DYN_SHARE
            # punch-preserving profiles also carry the mud cut, not just the dynamic band: a wide
            # bell at -4.5 dB at 267 Hz was the main reason the low-mids (and the slap with them)
            # disappeared on Wait for the Ground (see test/diag_lows.py)
            want *= float(pp.get("mud_static_scale", 1.0))
            if want > 0.3:
                q = float(np.interp(score, [2, 7], [0.8, 1.1]))
                # keep the bell off the kick body; the guard below checks the result anyway
                filters.append(("bell", max(it["mud_fc"], float(pp.get("mud_min_hz", 160.0))), -float(want), q,
                                f"mud score {score:.1f} dB at {it['mud_fc']:.0f} Hz "
                                f"(target distance {mud.get('excess_db', 0):+.1f}, local prominence {mud.get('local_prominence_db', 0):+.1f})"))
        rs = S["resonance"]
        if rs["enabled"] and it["resonances"]:
            for r in it["resonances"][: rs["max_notches"]]:
                g = -float(np.clip(0.6 * r["height_db"], 2.0, rs["max_cut_db"])) * min(A * rs["amount"], 1.25)
                q = float(np.interp(r["height_db"], [3, 8], [rs["q_min"], rs["q_max"]]))
                filters.append(("bell", r["freq_hz"], g, q,
                                f"resonance +{r['height_db']:.1f} dB, persists {100 * r['persistence']:.0f}% of the song"))
        if low_on:
            lo = S["low_end"]
            body_hz = float(lo.get("body_hz") or 105.0)
            dev63, dev125 = dev[list(BAND_CENTERS).index(63)], dev[list(BAND_CENTERS).index(125)]
            if le["deficit_db"] > DET["low_end"]["deficit_flag_db"]:
                g = min(0.6 * le["deficit_db"], lo["max_shelf_db"]) * A * lo["amount"]
                if mud_on:
                    g *= 0.75  # mud cut already restores perceived bass (brief 1.8)
                if g > 0 and "max_low_shelf_boost_db" in pp:
                    g = min(g, pp["max_low_shelf_boost_db"])  # a big boost lengthens the kick tail (TSR down)
                # a bell on the kick body instead of a shelf at 90 Hz: a shelf lifts the sub just
                # as much as the body, which is what made the lows woolly instead of tight
                filters.append(("bell", body_hz, float(g), 0.8,
                                f"bass {le['deficit_db']:+.1f} dB under target: kick body {body_hz:.0f} Hz"))
            else:
                cut_hz = 78.0 if dev63 > dev125 else 110.0
                g = -min(0.6 * le["excess_db"], 4.0) * A * lo["amount"]
                filters.append(("bell", cut_hz, float(g), 1.0,
                                f"bass {le['excess_db']:+.1f} dB over target at {cut_hz:.0f} Hz"))
        filters = [f for f in filters if abs(f[2]) >= 0.2]
        # ---- total-correction check (research2 3.1 step 7): if the curve has to swing more than
        # 6 dB peak-to-peak the mix is far from the target; halve it and say so instead of force-fitting
        span = 0.0
        if filters and valid.any():
            rb = F.eq_response_db([_sos_of(f, sr) for f in filters], np.asarray(BAND_CENTERS, float), sr)
            span = float(rb[valid].max() - rb[valid].min())
            if span > 6.0:
                k = 6.0 / span
                filters = [(t, fr, round(float(g) * k, 2), q, why + " (scaled down: mix is far from the target)")
                           for t, fr, g, q, why in filters]
                span = 6.0
        guard = []
        lowmid = {}
        if filters:
            filters, guard, lowmid = _guard_low_mid(filters, sr)
            filters = [f for f in filters if abs(f[2]) >= 0.3]
        if filters:
            step("static_eq", f"{len(filters)} corrective band(s)"
                 + (f", tilt {tilt['slope_db_per_oct']:+.2f} dB/oct corrected" if tilt["applied_db"] else ""),
                 filters=[{"type": tup[0], "freq_hz": round(float(tup[1]), 1), "gain_db": round(float(tup[2]), 2),
                           "q": round(float(tup[3]), 2), "why": (tup[4] if len(tup) > 4 else "")} for tup in filters],
                 lowmid_response_db=lowmid, lowmid_guard=guard, tilt=tilt, correction_span_db=round(span, 2))
    lo = S["low_end"]
    if lo["enabled"] and lo["psycho_bass"] and it["sub_missing"]:
        step("psycho_bass", "Sub-bass missing; add harmonics instead of boosting", mix_db=lo["psycho_bass_db"] + 3 * (1 - min(I, 1)))

    # ---- reference-track matching (tonal balance, optionally width and loudness)
    if rmatch:
        gain = it.get("ref_gain_db")
        side_db = 0.0
        if ref_rm.get("match_width", True) and rep.get("channels") == 2 and reference.get("sm_db") is not None:
            side_db = float(np.clip(float(reference["sm_db"]) - float(it.get("ref_src_sm_db", 0.0)), -3.0, 3.0))
            side_db *= min(I, 1.0)
        nm = Path(reference.get("name") or "reference").name if reference.get("name") else "reference"
        step("reference_match", f"Match '{nm}': tonal curve, max ±{float(ref_rm.get('max_db', 6.0)):.0f} dB, "
                                f"strength {float(ref_rm.get('strength', 1.0)):.2f}"
                                + (f", width {side_db:+.1f} dB" if abs(side_db) > 0.05 else ""),
             freqs=[float(v) for v in it["ref_src"][0]], gain_db=[float(v) for v in gain],
             taps=int(ref_rm.get("taps", 4097)), side_db=round(side_db, 2),
             realize=str(ref_rm.get("realize", "biquads")),
             ref_name=reference.get("name"), residual_before=it.get("ref_residual_before"))

    # ---- dynamic EQ
    de = S["dynamic_eq"]
    if de["enabled"]:
        A = I * de["amount"]
        bands = []
        mud = _issue(rep, "mud")
        if de["mud"] and mud["severity"] >= 30 and it["mud_var"] > 2:
            f0 = it["mud_fc"]
            # keep the mud band off the kick fundamental (<= 120 Hz) and the snare body's lowest part
            bands.append({"name": "mud", "lo": max(f0 / 1.5, 150.0), "hi": max(f0 * 1.5, 260.0),
                          "max_db": _mud_dyn_band_db(rep, S, pp, I, I * S["static_eq"]["amount"]),
                          "ratio": de["mud_ratio"], "att": de["mud_attack_ms"], "rel": de["mud_release_ms"], "over": 0.0,
                          "det_ms": 20.0})
        h = _issue(rep, "harshness")
        if de["harsh"] and h["severity"] >= 30:
            bands.append({"name": "harsh", "lo": de["harsh_lo_hz"], "hi": de["harsh_hi_hz"],
                          "max_db": min(de["harsh_max_db"] * min(A, 1.25) * (0.6 + 0.4 * h["severity"] / 100),
                                        pp.get("harsh_max_db", 99.0)),
                          "ratio": de["harsh_ratio"], "att": max(de["harsh_attack_ms"], pp.get("dyn_attack_min_ms", 0.0)),
                          "rel": de["harsh_release_ms"], "over": de["harsh_thresh_over_median_db"], "det_ms": 10.0})
        fz = _issue(rep, "fizz")
        if de["fizz"] and fz["severity"] >= 30 and cutoff > de["fizz_lo_hz"] * 1.2:
            bands.append({"name": "fizz", "lo": de["fizz_lo_hz"], "hi": min(de["fizz_hi_hz"], cutoff),
                          "max_db": min(de["fizz_max_db"] * min(A, 1.25), pp.get("fizz_max_db", 99.0)), "ratio": 3.0, "att": max(3.0, pp.get("dyn_attack_min_ms", 0.0)),
                          "rel": 80.0, "over": 2.0, "det_ms": 10.0})
        if bands:
            step("dynamic_eq", ", ".join(b["name"] for b in bands) + (" (drum hits protected)" if prot["protect"] else ""),
                 bands=bands, **prot)
    ds = S["deesser"]
    if ds["enabled"] and _sev(rep, "sibilance") >= 30:
        step("deesser", f"Sibilance severity {_sev(rep, 'sibilance'):.0f}" + (" (drum hits protected)" if prot["protect"] else ""),
             lo=ds["lo_hz"], hi=min(ds["hi_hz"], sr * 0.45),
             max_db=min(ds["max_db"] * min(I * ds["amount"], 1.25), pp.get("deesser_max_db", 99.0)), ratio=ds["ratio"],
             att=ds["attack_ms"], rel=ds["release_ms"], over=ds["thresh_over_median_db"], det_ms=5.0, **prot)

    # ---- dynamics
    mb = S["multiband"]
    prof_mb = profile.get("multiband")
    if mb["enabled"] and prof_mb:
        xo = mb["xover_hz"] or prof_mb["xover_hz"]
        cfg = {"xover_hz": xo, "bands": {}}
        names = ["low", "mid", "high"] if len(xo) == 2 else ["low", "high"]
        for nme in names:
            if nme in prof_mb and prof_mb[nme]:
                b = prof_mb[nme]
                att = float(b.get("attack_ms", mb["attack_ms"]))
                rel = float(b.get("release_ms", mb["release_ms"]))
                if mb.get("smart_timing", True):
                    # research2 2.4: attack/release from the crest factor, and never shorter than the
                    # profile asks for (a punchy source needs a long attack so the hit passes through)
                    cf = float(rep["metrics"]["crest_factor_db"])
                    att_r = float(np.clip(10.0 + 4.0 * (cf - 10.0), 10.0, 50.0)) * (0.5 if nme == "high" else 1.0)
                    rel_r = float(np.clip(80.0 + 20.0 * (12.0 - cf), 60.0, 300.0))
                    att = max(att, att_r)
                    rel = max(rel, rel_r)
                cfg["bands"][nme] = {"ratio": b["ratio"], "max_gr_db": b["max_gr_db"] * min(I * mb["amount"], 1.25),
                                     "attack_ms": att, "release_ms": rel,
                                     "auto_release": bool(b.get("auto_release", False)), "det_ms": b.get("det_ms", 10.0)}
        if cfg["bands"]:
            step("multiband", f"Profile multiband ({'/'.join(str(int(v)) for v in xo)} Hz)", **cfg, names=names, knee=mb["knee_db"])
    gl = S["glue"]
    pg = profile["glue"]
    if gl["enabled"]:
        tgr = (gl["target_gr_db"] if gl["target_gr_db"] is not None else pg["target_gr_db"]) * min(I * gl["amount"], 1.5)
        plr = rep["metrics"]["plr_db"]
        if plr < 8:
            tgr *= float(np.clip((plr - 4) / 4, 0.2, 1.0))  # already dense: compress less
        step("glue", f"Glue to ~{tgr:.1f} dB GR on loud parts", ratio=gl["ratio"] or pg["ratio"],
             attack_ms=gl["attack_ms"] or pg["attack_ms"], release_ms=gl["release_ms"] or pg["release_ms"],
             target_gr_db=round(tgr, 2), knee=gl["knee_db"], auto_release=bool(pg.get("auto_release", False)))
    sa = S["saturation"]
    ps = profile.get("saturation", {})
    if sa["enabled"]:
        drive = sa["drive_db"] if sa["drive_db"] is not None else ps.get("drive_db", 0)
        mix = (sa["mix"] if sa["mix"] is not None else ps.get("mix", 0)) * min(I * sa["amount"], 1.5)
        if warm:
            mix *= 0.5
        if _sev(rep, "clipping") >= 40 or rep["metrics"]["plr_db"] < 6:
            mix *= 0.5
        if drive > 0 and mix > 0.005:
            step("saturation", "Profile colour" + (" (halved: artifact-heavy/dense source)" if warm else ""),
                 drive_db=drive, mix=round(mix, 3), asym=sa["asymmetry"], oversample=sa["oversample"],
                 band_hz=200.0 if ps.get("band") == "low" else None, **prot)
        if sa["style_lpf"] and ps.get("style_lpf_hz"):
            step("style_lpf", "Lo-fi style low-pass (by design)", fc=ps["style_lpf_hz"])

    # ---- low-end punch (the only stages that *add* attack; everything above can only take it away)
    lpn = S.get("low_punch") or {}
    if lpn.get("enabled"):
        A = min(I * float(lpn.get("amount", 1.0)), 1.5)
        plr = float(rep["metrics"]["plr_db"])
        kick_hits = it.get("hits", {}).get("kick") if it.get("hits") else None
        n_kick = int(len(kick_hits)) if kick_hits is not None else 0
        s_mud, s_le = _sev(rep, "mud"), _sev(rep, "low_end")
        dense = float(np.clip((plr - 5.0) / 4.0, 0.0, 1.0))     # PLR 5 -> 0 (no headroom), 9+ -> 1
        need = float(np.clip(0.55 + 0.45 * dense + 0.25 * min(s_mud, 70.0) / 70.0 + 0.15 * min(s_le, 70.0) / 70.0,
                             0.5, 1.25))
        att_max = float(pp.get("low_punch_attack_max_db", lpn.get("max_attack_db", 3.0) or 3.0))
        attack = min(float(lpn["attack_db"]) * A * need, att_max)
        sustain = min(float(lpn["sustain_db"]) * A * need, att_max * 0.6)
        duck = float(lpn["sub_duck_db"]) * A
        duck_max = float(pp.get("low_punch_duck_max_db", lpn.get("max_duck_db", 2.5) or 2.5))
        duck = min(duck, duck_max)
        if n_kick < 8 or plr < 4.0:
            duck = 0.0          # too few hits to duck reliably, or no headroom at all
        if attack < 0.05 and sustain < 0.05 and duck < 0.05:
            pass
        else:
            why = []
            if attack >= 0.05 or sustain >= 0.05:
                why.append(f"kick band {lpn['band_lo_hz']:.0f}-{lpn['band_hi_hz']:.0f} Hz +{attack:.1f}/-{sustain:.1f} dB")
            if duck >= 0.05:
                why.append(f"sub {lpn['sub_lo_hz']:.0f}-{lpn['sub_hi_hz']:.0f} Hz ducked {duck:.1f} dB on {n_kick} hits")
            step("low_punch", "Low-end attack: " + ", ".join(why) + (" (dense mix: reduced)" if dense < 0.35 else ""),
                 attack_db=round(attack, 2), sustain_db=round(sustain, 2),
                 band_lo_hz=float(lpn["band_lo_hz"]), band_hi_hz=float(lpn["band_hi_hz"]),
                 attack_ms=float(lpn.get("attack_ms", 4.0)), release_ms=float(lpn.get("release_ms", 70.0)),
                 sub_duck_db=round(duck, 2), sub_lo_hz=float(lpn["sub_lo_hz"]), sub_hi_hz=float(lpn["sub_hi_hz"]),
                 duck_release_ms=float(lpn.get("duck_release_ms", 90.0)), kick_hits=n_kick)
    st = S["stereo"]
    if st["enabled"] and rep["channels"] == 2 and rep.get("stereo"):
        sd = rep["stereo"]
        pst = profile["stereo"]
        cw = pst["corr_window"]
        mono_hz = st["mono_bass_hz"] or pst["side_hpf_hz"]
        maxg = st["max_side_gain_db"] if st["max_side_gain_db"] is not None else pst["max_side_gain_db"]
        # solve r(g) = (PM - g^2 PS)/(PM + g^2 PS) for the window's upper edge (widen) or lower (narrow)
        r0 = sd["corr"]
        sm = 10 ** (sd["sm_db"] / 10)
        side_db = 0.0
        swirl = bool(it["swirl"])
        if r0 > cw[1] and not swirl:
            rt = cw[1]
            g2 = (1 - rt) / max(sm * (1 + rt), 1e-9)
            side_db = float(np.clip(10 * np.log10(max(g2, 1e-9)), 0, maxg)) * min(I * st["amount"], 1.0)
        elif r0 < cw[0] and st["allow_narrowing"]:
            rt = cw[0]
            g2 = (1 - rt) / max(sm * (1 + rt), 1e-9)
            side_db = float(np.clip(10 * np.log10(max(g2, 1e-9)), -3, 0)) * min(I * st["amount"], 1.0)
        swirl_db = S["dynamic_eq"]["swirl_max_db"] * min(I, 1.0) if (swirl and S["dynamic_eq"]["side_swirl"]) else 0.0
        mono_amt = float(pst.get("mono_amount", 1.0))
        step("stereo", f"Mono bass < {mono_hz:.0f} Hz" + (f" ({100 * mono_amt:.0f}%)" if mono_amt < 1 else "")
             + (f", side {side_db:+.1f} dB" if abs(side_db) > 0.05 else "")
             + (", HF side swirl control" if swirl_db else ""), mono_hz=mono_hz, mono_amount=mono_amt, side_db=round(side_db, 2),
             widen_above_hz=st["widen_above_hz"], swirl_db=swirl_db)
    lm = S["limiter"]
    tl, tp = resolve_targets(settings, profile)
    mode = settings["general"].get("mode", "streaming_safe")
    cl = S.get("clipper", {"enabled": False})
    clip_db = 0.0
    if cl.get("enabled"):
        clip_db = cl["drive_db"] if cl.get("drive_db") is not None else profile.get("clipper_db", {}).get(mode, 0.0)
        clip_db = float(np.clip(clip_db * min(I, 1.0), 0.0, 3.0))
    abort = lm["max_gr_abort_db"]
    if mode == "loud" and pp.get("loud_max_limiter_gr_db") is not None:
        abort = min(abort, float(pp["loud_max_limiter_gr_db"]))  # punch over loudness
    lk = {k: lm[k] for k in ("lookahead_ms", "release_ms", "oversample", "safety_margin_db", "iterations", "tolerance_lu")}
    step("limiter", f"Target {tl:.1f} LUFS, ceiling {tp:.1f} dBTP" + (f", soft clip {clip_db:.1f} dB first" if clip_db > 0.05 else "")
         + (f", GR cap {abort:.0f} dB (punch first)" if abort < lm["max_gr_abort_db"] else ""),
         target_lufs=tl, ceiling_dbtp=tp, **lk, release_slow_ms=lm.get("release_slow_ms"), max_gr_abort_db=abort,
         clip_db=round(clip_db, 2), enabled=lm["enabled"])
    return p


# ------------------------------------------------------------------ execution
def run_chain(x, sr, pl, rep, progress=None, cancel=None, probe=None):
    y = np.array(x, np.float64, copy=True)
    log = []
    n_steps = len(pl["steps"])
    for k, s in enumerate(pl["steps"]):
        if cancel and cancel():
            raise Cancelled()
        if progress:
            progress(int(100 * k / n_steps), s["module"])
        t0 = time.time()
        m = s["module"]
        info = {}
        if m == "headroom":
            y *= 10 ** (s["gain_db"] / 20)
        elif m == "declip":
            # runs were found on the original; re-detect on the (scaled) signal for exact positions
            y, info = RP.declip(y, rep["_internal"]["clip_runs"], s["max_run"])
        elif m == "cleanup":
            y = RP.dc_hpf_polarity(y, sr, s["hpf_hz"], s["dc"], s["flip"])
        elif m == "denoise":
            noise = RP._quiet_segments(y, sr, rep["_internal"]["mom_lufs"])
            before = _hf_active_level(y, sr)
            prop = s["prop"]
            for attempt in range(3):
                z = RP.denoise_hf(y, sr, prop, s["n_std"], s["split_hz"], noise)
                drop = before - _hf_active_level(z, sr)
                if drop <= s["max_hf_drop_db"] or attempt == 2:
                    break
                prop *= 0.5  # brief 1.4 safety gate
            pm = _protect(y.shape[0], sr, rep, s)
            if pm is not None:  # never gate/soften drum hits: dry signal on the hits
                z = z + pm[:, None] * (y - z)
            y = z
            info = {"prop_decrease": round(prop, 3), "hf_drop_db": round(float(drop), 2)}
        elif m == "hf_rebuild":
            y = RP.hf_rebuild(y, sr, s["fc"], s["mix_db"], s["drive_db"])
        elif m == "static_eq":
            for f in s["filters"]:
                if f["type"] == "bell":
                    sos = F.peaking(f["freq_hz"], f["gain_db"], f["q"], sr)
                else:
                    sos = F.shelf(f["freq_hz"], f["gain_db"], sr, "low" if f["type"] == "low_shelf" else "high", f["q"])
                y = F.apply_sos(y, sos)
        elif m == "psycho_bass":
            y = RP.psycho_bass(y, sr, s["mix_db"])
        elif m == "reference_match":
            y, info = RF.apply(y, sr, np.asarray(s["freqs"], float), np.asarray(s["gain_db"], float),
                               taps=s["taps"], side_db=s.get("side_db", 0.0),
                               realize=s.get("realize", "biquads"))
        elif m == "dynamic_eq":
            info = {}
            for b in s["bands"]:
                y, bi = dynamic_band(y, sr, b["lo"], b["hi"], b["max_db"], b["ratio"], b["att"], b["rel"], b["over"],
                                     detector_ms=b.get("det_ms", 5.0), protect=_protect(y.shape[0], sr, rep, s))
                info[b["name"]] = bi
        elif m == "deesser":
            y, info = dynamic_band(y, sr, s["lo"], s["hi"], s["max_db"], s["ratio"], s["att"], s["rel"], s["over"],
                                   relative_to_broadband=True, detector_ms=s.get("det_ms", 2.0),
                                   protect=_protect(y.shape[0], sr, rep, s))
        elif m == "multiband":
            bands = F.split_bands(y, s["xover_hz"], sr)
            info = {}
            for nme, b in zip(s["names"], range(len(bands))):
                cfg = s["bands"].get(nme)
                if not cfg:
                    continue
                band = bands[b]
                thr, _ = auto_threshold(band, sr, cfg["ratio"], cfg["max_gr_db"] * 0.6,
                                        active_db=10 * np.log10(np.mean(band ** 2) + 1e-20) - 20)
                bands[b], gr = compress(band, sr, thr, cfg["ratio"], cfg["attack_ms"], cfg["release_ms"], s["knee"],
                                        max_gr=cfg["max_gr_db"], detector_ms=cfg.get("det_ms", 10.0),
                                        auto_release=cfg.get("auto_release", False))
                # level-preserving: make up the band's average reduction so the multiband only
                # evens out dynamics and never acts as a static EQ (it used to pull 30-80 Hz down
                # ~0.9 dB on RuneCore while low-mids rose: "tamed, mushy low end")
                mk = float(10 * np.log10((np.mean(band ** 2) + 1e-20) / (np.mean(bands[b] ** 2) + 1e-20)))  # energy-preserving
                mk = float(np.clip(mk, 0.0, cfg["max_gr_db"]))
                bands[b] = bands[b] * 10 ** (mk / 20)
                info[nme] = {"threshold_db": round(thr, 2), "gr_max_db": round(float(gr.max()), 2), "makeup_db": round(mk, 2)}
            y = np.sum(bands, axis=0)
        elif m == "glue":
            y, info = glue_compress(y, sr, s["ratio"], s["attack_ms"], s["release_ms"], s["target_gr_db"], s["knee"],
                                    auto_release=s.get("auto_release", False))
        elif m == "saturation":
            y = TN.saturate(y, sr, s["drive_db"], s["mix"], s["asym"], s["oversample"], s["band_hz"],
                            protect=_protect(y.shape[0], sr, rep, s))
        elif m == "style_lpf":
            y = TN.style_lpf(y, sr, s["fc"])
        elif m == "low_punch":
            info = {}
            if s["attack_db"] > 0.01 or s["sustain_db"] > 0.01:
                y, i1 = LE.transient_shape(y, sr, s["band_lo_hz"], s["band_hi_hz"], s["attack_db"], s["sustain_db"],
                                           attack_ms=s["attack_ms"], release_ms=s["release_ms"])
                info.update(i1)
            if s["sub_duck_db"] > 0.01:
                y, i2 = LE.duck_sub_on_hits(y, sr, rep["_internal"].get("hits", {}).get("kick"),
                                            s["sub_lo_hz"], s["sub_hi_hz"], s["sub_duck_db"],
                                            release_ms=s["duck_release_ms"])
                info.update(i2)
            info["kick_hits"] = s.get("kick_hits", 0)
        elif m == "stereo":
            y, info = TN.stereo_process(y, sr, s["mono_hz"], s.get("mono_amount", 1.0), s["side_db"], s["widen_above_hz"],
                                        s["swirl_db"])
        elif m == "limiter":
            if s["enabled"]:
                y, info = limit_to_target(y, sr, s["target_lufs"], s["ceiling_dbtp"], s["lookahead_ms"], s["release_ms"],
                                          s["oversample"], s["safety_margin_db"], s["iterations"], s["tolerance_lu"],
                                          s["max_gr_abort_db"], release_slow_ms=s.get("release_slow_ms"),
                                          clip_db=s.get("clip_db", 0.0))
            else:  # loudness-normalise and hard-guard the ceiling only
                g = float(np.clip(s["target_lufs"] - LN.integrated_lufs(y, sr), -40.0, 30.0))
                y = y * 10 ** (g / 20)
                tpk = LN.true_peak(y, sr, 4)
                c = 10 ** ((s["ceiling_dbtp"] - 0.05) / 20)
                cut = 0.0
                if tpk > c:
                    cut = 20 * np.log10(c / tpk)
                    y *= c / tpk
                info = {"disabled": True, "pre_gain_db": round(g, 2), "ceiling_cut_db": round(cut, 2),
                        "out_lufs": LN.integrated_lufs(y, sr), "reachable": cut > -0.3}
        y = np.nan_to_num(y, nan=0.0, posinf=0.0, neginf=0.0)
        log.append({"module": m, "seconds": round(time.time() - t0, 2), **({"info": info} if info else {})})
        if probe is not None:
            probe(m, y)
    # final hard guard: never exceed ceiling, never NaN
    ceil = next((s["ceiling_dbtp"] for s in pl["steps"] if s["module"] == "limiter"), None)
    if ceil is None:  # partial chain (diagnostics): no loudness stage, no ceiling guard
        return y, log
    tpk = LN.true_peak(y, sr, 4)
    c = 10 ** ((ceil - 0.02) / 20)
    if tpk > c:
        y *= c / tpk
    return np.clip(y, -1.0, 1.0), log


def _protect(n, sr, rep, step):
    """Drum-hit protect mask (0..1) for this step, or None."""
    if not step.get("protect"):
        return None
    hits = rep.get("_internal", {}).get("hits")
    if not hits:
        return None
    from .punch import hit_mask
    return hit_mask(n, sr, hits, post_ms=step.get("protect_ms", 40.0))


def _hf_active_level(x, sr):
    if sr / 2 < 9000:
        return 0.0
    f, t, P = _stft_power(x, sr, 2048, 1024, max_frames=1500)
    tot = P.sum(axis=0)
    act = tot > np.percentile(tot, 30)
    return float(10 * np.log10(np.mean(_band_power(P[:, act], f, 8000, min(16000, sr / 2)))))


def _score(rep_in, rep_out):
    """Positive = output worse than input on distortion/artifact measures."""
    worse = {}
    for k in SAFETY_KEYS:
        d = _sev(rep_out, k) - _sev(rep_in, k)
        # an increase that stays below the "act on it" level (30) is not a regression: on RuneCore
        # the tonal EQ lifted sibilance 0.3 -> 18 ("none/minor") and the safety loop threw away
        # 40 % of the correction for it
        if d > 0 and _sev(rep_out, k) >= 30:
            worse[k] = round(d, 1)
    return worse


def master(x, sr, settings, path=None, progress=None, cancel=None, report_in=None, reference=None):
    """Full pipeline. Returns (y, result dict).

    reference: dict from mastercraft.reference.target()/load_target(). When given, the reference
    replaces the profile's tonal target (and optionally its loudness and width); the profile still
    supplies dynamics, punch caps, stereo and clipper."""
    prof = get_profile(settings["general"]["profile"])
    if reference and reference.get("freqs") is not None:
        # a reference IS a tonal target: analyse against it (mud, low end, harshness all become
        # reference-relative), while the profile keeps supplying dynamics, punch caps and stereo
        prof = copy.deepcopy(prof)
        rf_g = np.asarray(reference["freqs"], float)
        rf_d = np.asarray(reference["db"], float)
        prof["tonal_db"] = [round(float(np.interp(np.log2(fc), np.log2(rf_g), rf_d)), 2) for fc in BAND_CENTERS]
        nm = Path(str(reference.get("name") or "reference")).name
        prof["display_name"] = f"{prof['display_name']} + ref '{nm}'"
        prof["source"] = (prof.get("source", "") + " | tonal target from reference "
                          f"'{reference.get('name')}'").strip()
    cb = (lambda a, b, msg: progress(int(a + (b - a) * msg[0] / 100), msg[1])) if progress else None

    def sub(a, b):
        return (lambda pct, msg: progress(int(a + (b - a) * pct / 100), msg)) if progress else None

    rep_in = report_in or analyze(x, sr, prof, path, progress=sub(0, 20))
    too_short = LN.as2d(x).shape[0] < max(1024, int(0.02 * sr))
    if too_short or not np.isfinite(rep_in["metrics"]["integrated_lufs"]) or rep_in["metrics"]["integrated_lufs"] <= -69.5:  # meter floor is -70
        # digital silence / near silence: any gain would only raise noise (QA L5: +30 dB pre-gain on silence)
        tl, tp = resolve_targets(settings, prof)
        if progress:
            progress(100, "done")
        rin = strip_internal(rep_in)
        return np.array(x, copy=True), {
            "input": rin, "output": rin, "plan": {"intensity": 0.0, "steps": []}, "log": [],
            "targets": {"lufs": tl, "ceiling_dbtp": tp, "mode": settings["general"]["mode"]},
            "profile": prof["display_name"], "safety": {"attempts": [], "chosen_intensity_scale": 1.0},
            "limiter": {}, "punch": {"input": {}, "vs_input": {}, "score_db": 0.0, "threshold_db": settings["safety"].get("punch_max_loss_db", 2.0), "guard": []},
            "notes": [("Input is too short to master (under 1024 samples / 20 ms)" if too_short else "Input is silent (below -70 LUFS)")
                      + ": nothing was processed and the output is a copy of the input."]
                     + rep_in.get("warnings", [])}
    # QA N2: cap the make-up gain so near-silent material (room tone, dither, hiss) is not blown
    # up by 50 dB. The capped target replaces the requested one for this file and a note says so.
    gain_cap = None
    cap = settings["modules"]["limiter"].get("max_makeup_db", 18.0)
    tl_req, _ = resolve_targets(settings, prof)
    lin = rep_in["metrics"]["integrated_lufs"]
    if cap is not None and tl_req - lin > float(cap):
        settings = copy.deepcopy(settings)
        settings["general"]["target_lufs"] = round(lin + float(cap), 2)
        gain_cap = {"requested_lufs": tl_req, "input_lufs": lin, "cap_db": float(cap), "capped_lufs": round(lin + float(cap), 2)}
    sf = settings["safety"]
    from . import punch as P
    xin = LN.as2d(np.asarray(x, np.float64))
    hits = P.detect_hits(xin, sr)
    rep_in.setdefault("_internal", {})["hits"] = hits
    # ---- reference match: one curve for the whole file (measured on the input, before the chain)
    ref_info = None
    rm = (settings["modules"].get("reference_match") or {})
    if reference and rm.get("enabled", True) and reference.get("freqs") is not None:
        src_g, src_db = RF._ltas_curve(xin, sr)
        rep_in["_internal"]["ref_src"] = (src_g, src_db)
        cap_db = float(rm.get("max_db", 6.0))
        gain = RF.match_gain_db((src_g, src_db), (reference["freqs"], reference["db"]),
                               strength=float(rm.get("strength", 1.0)), max_db=cap_db,
                               cutoff_hz=rep_in.get("cutoff_hz"))
        rep_in["_internal"]["ref_gain_db"] = gain
        rep_in["_internal"]["ref_src_sm_db"] = float(RF._stereo_sm_db(xin, sr))
        bands = list(BAND_CENTERS)
        ref_info = {"name": reference.get("name"), "strength": round(float(rm.get("strength", 1.0)), 2),
                    "max_db": cap_db, "ref_lufs": round(float(reference.get("lufs", 0.0)), 2),
                    "ref_plr_db": round(float(reference.get("plr_db", 0.0)), 2),
                    "match_loudness": bool(rm.get("match_loudness")),
                    "gain_max_db": round(float(np.max(gain)), 2), "gain_min_db": round(float(np.min(gain)), 2),
                    "residual_before": RF.residual_bands(xin, sr, reference, bands)}
        rep_in["_internal"]["ref_residual_before"] = ref_info["residual_before"]
        if rm.get("match_loudness") and np.isfinite(reference.get("lufs", np.nan)):
            settings = copy.deepcopy(settings)
            settings["general"]["target_lufs"] = float(np.clip(float(reference["lufs"]), -20.0, -6.0))
    m_in = P.hit_metrics(xin, sr, hits)
    punch_in = P.summary(m_in)
    attempts = []
    scale = 1.0
    tries = 1 + (sf["max_retries"] if sf["enabled"] else 0)
    for a in range(tries):
        if cancel and cancel():
            raise Cancelled()
        pl = plan(rep_in, prof, settings, scale, reference=reference)
        lo, hi = 20 + 60 * a / tries, 20 + 60 * (a + 1) / tries
        y, log = run_chain(x, sr, pl, rep_in, progress=sub(lo, hi - 5), cancel=cancel)
        punch_log = []
        if sf.get("punch_check", True):
            y, pl, log, punch_log = _punch_guard(x, y, sr, pl, log, rep_in, hits, m_in, sf, cancel)
        rep_out = analyze(y, sr, prof, None, progress=sub(hi - 5, hi), fast=True)
        worse = _score(rep_in, rep_out)
        if reference is not None:
            worse.pop("mud", None)   # the mud score is target-relative; the match is judged separately
        bad = {k: v for k, v in worse.items() if v >= sf["worse_by"]}
        attempts.append({"intensity_scale": round(scale, 3), "plan": pl, "log": log, "report": rep_out,
                         "worse": worse, "rejected": bool(bad), "y": y, "punch_log": punch_log})
        if not bad or not sf["enabled"]:
            break
        if a > 0 and sum(worse.values()) >= sum(attempts[-2]["worse"].values()) - 1.0:
            break  # backing off did not help: the regression is not caused by the corrective moves
        scale *= sf["backoff"]
    best = min(attempts, key=lambda t: (t["rejected"], sum(t["worse"].values())))
    if progress:
        progress(100, "done")
    tl, tp = resolve_targets(settings, prof)
    lim = next((l.get("info", {}) for l in best["log"] if l["module"] == "limiter"), {})
    res = {"input": strip_internal(rep_in), "output": strip_internal(best["report"]), "plan": best["plan"],
           "log": best["log"], "targets": {"lufs": tl, "ceiling_dbtp": tp, "mode": settings["general"]["mode"]},
           "profile": prof["display_name"],
           "safety": {"attempts": [{"intensity_scale": t["intensity_scale"], "worse": t["worse"], "rejected": t["rejected"]}
                                   for t in attempts], "chosen_intensity_scale": best["intensity_scale"]},
           "limiter": lim}
    cmp_out = P.compare(m_in, P.hit_metrics(LN.as2d(best["y"]), sr, hits), hits, sr)
    res["punch"] = {"input": punch_in, "vs_input": cmp_out, "score_db": round(P.punch_score(cmp_out), 2),
                    "threshold_db": sf.get("punch_max_loss_db", 2.0), "guard": best["punch_log"]}
    if ref_info is not None:
        try:
            ref_info["residual_after"] = RF.residual_bands(LN.as2d(best["y"]), sr, reference, list(BAND_CENTERS))
        except Exception:
            ref_info["residual_after"] = None
        res["reference"] = ref_info
    notes = []
    if ref_info is not None:
        before = ref_info.get("residual_before") or {}
        after = ref_info.get("residual_after") or {}
        if before and after:
            b = float(np.sqrt(np.mean([v ** 2 for v in before.values()])))
            a = float(np.sqrt(np.mean([v ** 2 for v in after.values()])))
            notes.append(f"Reference match '{Path(str(ref_info.get('name'))).name}': band distance to the reference "
                         f"{b:.1f} -> {a:.1f} dB RMS" + (f" (capped at +/-{ref_info['max_db']:.0f} dB)" if
                         ref_info["gain_max_db"] >= ref_info["max_db"] - 0.01 or ref_info["gain_min_db"] <= -ref_info["max_db"] + 0.01 else "")
                         + ("; loudness target taken from the reference" if ref_info.get("match_loudness") else "")
                         + ". The reference sets the tone; the profile still sets dynamics and punch limits.")
        if ref_info["max_db"] >= 6.0 and (ref_info["gain_max_db"] >= 5.99 or ref_info["gain_min_db"] <= -5.99):
            notes.append("The reference needs more than the +/-6 dB match limit somewhere: raise "
                         "modules.reference_match.max_db if you trust the reference, or pick a closer one.")
    if gain_cap:
        res["targets"]["requested_lufs"] = gain_cap["requested_lufs"]
        res["targets"]["gain_cap"] = gain_cap
        notes.append(f"Make-up gain capped at +{gain_cap['cap_db']:.0f} dB: the input is very quiet ({gain_cap['input_lufs']:.1f} LUFS), "
                     f"so the target was {gain_cap['capped_lufs']:.1f} LUFS instead of {gain_cap['requested_lufs']:.1f}. "
                     "If this is real music, raise modules.limiter.max_makeup_db.")
    for g in best["punch_log"]:
        if g.get("action"):
            notes.append(f"Punch check: {g['reason']} -> {g['action']}.")
        if g.get("largest_increase"):
            li = g["largest_increase"]
            notes.append(f"Punch check: the largest single punch loss (+{li['db']:.1f} dB) came from the {li['stage']} stage, "
                         "which the guard does not back off; consider lowering that module's amount.")
        if g.get("still_over"):
            tops = g.get("top_stages") or []
            blame = ", ".join(f"{k} +{v:.1f} dB" for k, v in tops)
            adv = "; ".join(dict.fromkeys(_STAGE_ADVICE.get(k, f"lower the {k} amount") for k, _ in tops)) or \
                "try a lower loudness target or streaming-safe mode"
            notes.append(f"WARNING: punch check could not get back under the limit: {g['reason']}."
                         + (f" Largest contributors: {blame}." if blame else "") + f" Suggestion: {adv}.")
    lim_step = next((st for st in best["plan"]["steps"] if st["module"] == "limiter"), {})
    out_lufs = res["output"]["metrics"]["integrated_lufs"]
    if lim.get("disabled"):
        notes.append(f"Limiter is disabled: the output was only gain-matched and peak-scaled to the ceiling, so the {tl:.1f} LUFS "
                     f"target may not be reached (output {out_lufs:.1f} LUFS"
                     + (f"; {-lim['ceiling_cut_db']:.1f} dB pulled back to respect the {tp:.1f} dBTP ceiling" if lim.get("ceiling_cut_db", 0) < -0.05 else "")
                     + "). Enable the limiter to hit the target.")
    elif settings["general"].get("mode") == "loud" and lim and not lim.get("reachable", True) and prof.get("punch", {}).get("loud_max_limiter_gr_db"):
        notes.append(f"Loud mode, {prof['display_name']} profile: punch is preferred over loudness, so limiting was capped at "
                     f"{lim_step.get('max_gr_abort_db', prof['punch']['loud_max_limiter_gr_db']):.1f} dB GR; output is {lim['out_lufs']:.1f} LUFS instead of {tl:.1f}.")
    elif lim and not lim.get("reachable", True):
        cap = lim_step.get("max_gr_abort_db", settings["modules"]["limiter"]["max_gr_abort_db"])
        notes.append(f"Target {tl:.1f} LUFS not reachable cleanly (needs > {cap:.1f} dB "
                     f"of limiting). Stopped at {lim['out_lufs']:.1f} LUFS.")
    elif lim and abs(out_lufs - tl) > 0.5:
        # reachable by the limiter's own estimate but the final true-peak guard / trims moved it (QA H1)
        notes.append(f"WARNING: output is {out_lufs:.1f} LUFS, {out_lufs - tl:+.1f} LU from the {tl:.1f} LUFS target"
                     + (f" (a {lim.get('trim_db', 0):.1f} dB true-peak safety trim was needed)" if lim.get("trim_db", 0) < -0.1 else "") + ".")
    pmax = prof.get("limiter_max_gr_db", {}).get(settings["general"]["mode"])
    if lim and pmax is not None and lim.get("max_gr_db", 0) > pmax + 0.5:
        notes.append(f"Limiter peaked at {lim['max_gr_db']:.1f} dB GR (profile guideline {pmax} dB); consider a lower target "
                     "or the streaming-safe mode for more punch.")
    if len(attempts) > 1:
        first = ", ".join(f"{k} +{v:.0f}" for k, v in attempts[0]["worse"].items())
        if best["intensity_scale"] < 1.0:
            notes.append(f"Safety check backed off processing to {100 * best['intensity_scale']:.0f}% intensity "
                         f"(first pass measured worse on: {first}).")
        else:
            notes.append(f"Safety check: output measured worse on {first}; backing off intensity "
                         f"({', '.join(f'{100 * t['intensity_scale']:.0f}%' for t in attempts[1:])}) did not improve it, so the "
                         "full pass was kept. This usually comes from heavy limiting changing the tonal balance; "
                         "try a lower loudness target or streaming-safe mode.")
    # a mud regression that comes from *choosing* a smaller low-mid cut is not a bug: say which
    # trade-off was made instead of leaving the generic "backing off did not help" note
    se_step = next((st for st in best["plan"]["steps"] if st["module"] == "static_eq"), {})
    d_mud = (best.get("worse") or {}).get("mud")
    if d_mud and se_step:
        guarded = bool(se_step.get("lowmid_guard"))
        notes.append(f"Low-mid cut was kept smaller than the measurement asked for"
                     + (" (kick-body guard)" if guarded else "") +
                     f", so the output reads {d_mud:+.0f} muddier on this profile: that is the punch/clarity "
                     "trade-off, not a regression. Raise modules.static_eq.amount, the profile's mud scale "
                     "(mud_static_scale / mud_scale) or the dynamic EQ mud amount to trade some slap for clarity.")
    n_in = next((i for i in rep_in.get("issues", []) if i["key"] == "noise"), None)
    n_out = next((i for i in best["report"].get("issues", []) if i["key"] == "noise"), None)
    if n_in and n_out and n_out["severity"] >= 30 and n_out["severity"] > n_in["severity"] + 5:
        # QA L10: the hiss score rises with the loudness gain; say why instead of leaving it unexplained
        dg = out_lufs - rep_in["metrics"]["integrated_lufs"]
        notes.append(f"Noise/hiss score rose {n_in['severity']:.0f} -> {n_out['severity']:.0f}: mastering raised the level by "
                     f"{dg:+.1f} dB and denoise is gentle by design. Raise the denoise amount if the hiss is audible.")
    res["notes"] = notes + rep_in.get("warnings", [])
    return best["y"], res


_STAGE_ADVICE = {
    "limiter": "lower the loudness target or use streaming-safe mode",
    "clipper": "lower or disable the clipper drive",
    "glue": "lower the glue amount", "multiband": "lower the multiband amount",
    "saturation": "lower the saturation amount", "static_eq": "lower the static EQ amount",
    "dynamic_eq": "lower the dynamic EQ amount", "deesser": "lower the de-esser amount",
    "stereo": "lower the stereo amount or the mono-bass frequency", "psycho_bass": "lower the low-end amount",
    "denoise": "lower the denoise amount", "cleanup": "raise less HPF (cleanup hpf_hz)",
    "low_punch": "lower the low_punch attack/sustain or sub duck",
}
PUNCH_MIN_HITS = 16   # below this many hits the smeared percentage is not meaningful (QA N8)

_BACKOFF_ORDER = ("saturation", "multiband", "glue", "dynamic_eq", "deesser", "limiter", "static_eq",
                  "reference_match", "denoise")


def _backoff_step(st):
    """Halve the strength of one stage in place. Returns a short description or None."""
    m = st["module"]
    if m == "saturation" and st["mix"] > 0.01:
        st["mix"] = round(st["mix"] * 0.5, 3); return f"saturation mix halved to {st['mix']:.2f}"
    if m == "multiband":
        for b in st["bands"].values():
            b["max_gr_db"] = round(b["max_gr_db"] * 0.5, 2); b["attack_ms"] = max(b["attack_ms"], 40.0)
        return "multiband gain reduction halved, attack >= 40 ms"
    if m == "glue" and st["target_gr_db"] > 0.2:
        st["target_gr_db"] = round(st["target_gr_db"] * 0.5, 2); st["attack_ms"] = max(st["attack_ms"], 30); return f"glue target {st['target_gr_db']:.1f} dB GR"
    if m == "dynamic_eq":
        for b in st["bands"]:
            b["max_db"] = round(b["max_db"] * 0.5, 2)
        return "dynamic EQ depth halved"
    if m == "deesser":
        st["max_db"] = round(st["max_db"] * 0.5, 2); return f"de-esser depth {st['max_db']:.1f} dB"
    if m == "limiter":
        if st.get("clip_db", 0) > 0.05:
            st["clip_db"] = round(st["clip_db"] * 0.5, 2); return f"soft clip {st['clip_db']:.1f} dB"
        st["max_gr_abort_db"] = max(2.0, st["max_gr_abort_db"] - 1.5)
        return f"limiter GR cap {st['max_gr_abort_db']:.1f} dB (gives up loudness)"
    if m == "static_eq":
        for f in st["filters"]:
            if f["gain_db"] > 0:
                f["gain_db"] = round(f["gain_db"] * 0.5, 2)
        return "EQ boosts halved"
    if m == "denoise":
        st["prop"] = round(st["prop"] * 0.5, 3); return "denoise halved"
    if m == "reference_match":
        st["gain_db"] = [round(float(g) * 0.5, 2) for g in st["gain_db"]]
        st["side_db"] = round(float(st.get("side_db", 0.0)) * 0.5, 2)
        return "reference match strength halved"
    return None


def _punch_guard(x, y, sr, pl, log, rep, hits, m_in, sf, cancel=None, max_rounds=2):
    """Post-check: compare kick/snare/crack/full transient-to-sustain ratio before vs after.
    If punch degraded past the threshold, find the responsible stage (punch measured after
    each stage of a probe run) and back it off; re-render. Returns (y, plan, log, record)."""
    from . import punch as P
    thr = float(sf.get("punch_max_loss_db", 2.0))
    thr_pct = float(sf.get("punch_max_smeared_pct", 15.0)) / 100.0
    record = []

    def measure(z):
        c = P.compare(m_in, P.hit_metrics(LN.as2d(z), sr, hits), hits, sr)
        sm = max((v.get("frac_hits_smeared", 0.0) for v in c.values() if v.get("hits", 0) >= PUNCH_MIN_HITS), default=0.0)
        return P.punch_score(c), sm, c

    score, sm, _ = measure(y)
    record.append({"round": 0, "punch_loss_db": round(score, 2), "max_smeared_pct": round(100 * sm, 1)})
    done = set()
    for rnd in range(1, max_rounds + 1):
        if score <= thr and sm <= thr_pct:
            break
        if cancel and cancel():
            raise Cancelled()
        stage_scores = {}
        prev = [0.0]

        def probe(mod, z):
            # measure after EVERY stage so a stage is charged only with its own change (QA M7: the
            # stereo/mono-bass stage used to be charged to the limiter that followed it)
            sc, s2, _ = measure(z)
            stage_scores[mod] = round(stage_scores.get(mod, 0.0) + sc - prev[0], 2)
            prev[0] = sc
        run_chain(x, sr, pl, rep, probe=probe)
        cand = sorted(((k, v) for k, v in stage_scores.items() if k in _BACKOFF_ORDER), key=lambda kv: -kv[1])
        pl_prev = pl
        pl = copy.deepcopy(pl)
        action = None
        for mod, inc in cand:
            if inc <= 0.15:
                break
            if mod in done:
                continue
            st = next((q for q in pl["steps"] if q["module"] == mod), None)
            action = _backoff_step(st) if st else None
            if action:
                culprit = mod
                done.add(mod)
                break
        rec = {"round": rnd, "per_stage_increase_db": stage_scores,
               "reason": f"punch loss {score:.1f} dB p90 / {100 * sm:.0f}% hits smeared (limit {thr:.1f} dB / {100 * thr_pct:.0f}%)"}
        top = max(stage_scores.items(), key=lambda kv: kv[1], default=(None, 0.0))
        if top[0] and top[0] not in _BACKOFF_ORDER and top[1] > 0.15:
            rec["largest_increase"] = {"stage": top[0], "db": top[1], "backoff_supported": False}
        if not action:
            rec["action"] = None
            record.append(rec)
            break
        y2, log2 = run_chain(x, sr, pl, rep)
        s2, sm2, _ = measure(y2)
        rec.update(stage=culprit, action=f"backed off {culprit}: {action}", punch_loss_db_after=round(s2, 2),
                   max_smeared_pct_after=round(100 * sm2, 1))
        record.append(rec)
        if s2 + 0.5 * 100 * sm2 / 10 >= score + 0.5 * 100 * sm / 10:
            rec["action"] += " (no improvement, reverted)"
            pl = pl_prev
            continue  # the stage stays in `done`; the next round tries the next culprit
        y, log, score, sm = y2, log2, s2, sm2
    n_full = len(P.hits_for("full", hits, sr))
    last = next((r for r in reversed(record) if r.get("per_stage_increase_db")), None)
    tops = sorted(((k, v) for k, v in (last or {}).get("per_stage_increase_db", {}).items() if v > 0.15), key=lambda kv: -kv[1])[:2]
    if (score > thr or sm > thr_pct) and n_full >= PUNCH_MIN_HITS:
        record.append({"round": "final", "still_over": True, "punch_loss_db": round(score, 2), "top_stages": tops,
                       "max_smeared_pct": round(100 * sm, 1),
                       "reason": f"punch loss {score:.1f} dB p90 / {100 * sm:.0f}% hits smeared is still over the limit "
                                 f"({thr:.1f} dB / {100 * thr_pct:.0f}%)"})
    return y, pl, log, record


def strip_internal(rep):
    r = dict(rep)
    r.pop("_internal", None)
    return r
