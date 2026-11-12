"""Analysis engine. Detector features and thresholds follow docs/research.md (sections 1.1-1.8,
detector block of 2.4). Every issue gets a 0-100 severity, a plain-language explanation and the
fix the mastering chain will apply. Thresholds tagged HEURISTIC in the brief are starting points
(see `mastercraft.cli --calibrate` for the tuning workflow, brief section 6)."""
import numpy as np
from scipy import signal
from scipy.ndimage import uniform_filter1d

from . import loudness as LN
from .profiles import BAND_CENTERS, DETECTORS as DET, TONAL, target_curve_at, suggest_genre, get_profile
from .dsp.repair import find_clip_runs
from .dsp.filters import bandpass_sos, apply_sos, lowpass_sos, highpass_sos

NFFT = 4096
HOP = 1024
MAX_FRAMES = 6000          # spectral stats use at most this many frames (evenly spread)


# ------------------------------------------------------------------ helpers
def sev_map(v, flag, severe, extreme):
    """Piecewise severity: 0 at 0, 30 at flag, 70 at severe, 100 at extreme (monotone)."""
    if v is None or not np.isfinite(v) or v <= 0:
        return 0.0
    if v < flag:
        return 30.0 * v / flag
    if v < severe:
        return 30.0 + 40.0 * (v - flag) / max(severe - flag, 1e-9)
    if v < extreme:
        return 70.0 + 30.0 * (v - severe) / max(extreme - severe, 1e-9)
    return 100.0


def _stft_power(mono_or_multi, sr, nfft, hop, max_frames=None):
    """Mean-over-channels one-sided power spectrum, Parseval-scaled so that sum over bins ~
    mean-square of the frame. Returns (freqs, times, P[float32 bins x frames])."""
    x = LN.as2d(mono_or_multi)
    n = x.shape[0]
    if n < nfft:
        x = np.pad(x, ((0, nfft - n), (0, 0)))
        n = nfft
    starts = np.arange(0, n - nfft + 1, hop)
    if max_frames and starts.size > max_frames:
        starts = starts[np.linspace(0, starts.size - 1, max_frames).astype(int)]
    w = signal.windows.hann(nfft, sym=False)
    scale = 2.0 / (nfft * np.sum(w ** 2))
    P = np.zeros((nfft // 2 + 1, starts.size), np.float32)
    blk = 512
    for b in range(0, starts.size, blk):
        st = starts[b:b + blk]
        acc = None
        for ch in range(x.shape[1]):
            fr = np.lib.stride_tricks.sliding_window_view(x[:, ch], nfft)[st] * w
            pw = np.abs(np.fft.rfft(fr, axis=1)) ** 2
            acc = pw if acc is None else acc + pw
        P[:, b:b + st.size] = (acc / x.shape[1] * scale).T.astype(np.float32)
    freqs = np.fft.rfftfreq(nfft, 1.0 / sr)
    times = (starts + nfft / 2) / sr
    return freqs, times, P


def _band_power(P, freqs, lo, hi):
    m = (freqs >= lo) & (freqs < hi)
    if not np.any(m):
        return np.full(P.shape[1] if P.ndim == 2 else 1, 1e-20)
    return P[m].sum(axis=0) + 1e-20


def frac_oct_smooth(spec, freqs, frac):
    """Fractional-octave smoothing (power average over f*2^(+-1/(2*frac)))."""
    cs = np.concatenate([[0.0], np.cumsum(spec)])
    lo = np.searchsorted(freqs, freqs * 2 ** (-0.5 / frac))
    hi = np.searchsorted(freqs, freqs * 2 ** (0.5 / frac), side="right")
    hi = np.maximum(hi, lo + 1)
    return (cs[hi] - cs[lo]) / (hi - lo)


def octave_band_levels(ltas, freqs, sr):
    """Brief 1.1: band level = 10log10(sum power in band), relative to 1 kHz band."""
    lv, valid = [], []
    for fc in BAND_CENTERS:
        lo, hi = fc / np.sqrt(2), fc * np.sqrt(2)
        ok = hi < sr / 2 * 0.98
        m = (freqs >= lo) & (freqs < hi)
        p = ltas[m].sum() if np.any(m) else 1e-20
        lv.append(10 * np.log10(p + 1e-20))
        valid.append(bool(ok and np.any(m)))
    lv = np.array(lv)
    i1k = BAND_CENTERS.index(1000)
    return lv - lv[i1k], np.array(valid), lv


def third_oct_levels(ltas, freqs, lo=20, hi=20000):
    centers = 1000 * 2 ** (np.arange(-17, 14) / 3.0)
    centers = centers[(centers >= lo) & (centers <= min(hi, freqs[-1] / 1.12))]
    out = []
    for fc in centers:
        m = (freqs >= fc * 2 ** (-1 / 6)) & (freqs < fc * 2 ** (1 / 6))
        out.append(10 * np.log10(ltas[m].sum() + 1e-20) if np.any(m) else -200)
    return centers, np.array(out)


MUD_SCAN_HZ = (150.0, 500.0)
# the maximum of a smoothed curve reads higher than a band average: on unprocessed and mastered
# music the 150-500 Hz maximum sits about 0.75 dB above the old octave-band measure, so the
# severity mapping takes that off before applying the usual 2 / 4 / 7 dB flag points
MUD_MAX_BIAS_DB = 0.75
MUD_PROM_WEIGHT = 0.5


def mud_cut_db(score_db, max_cut_db):
    """Total low-mid cut wanted for a mud score (dB, before the intensity scale)."""
    return float(np.clip(0.8 * (score_db - 1.0), 0.0, max_cut_db + 2.0))


def _mud_scan(ltas, freqs, profile, sr):
    """Low-mid build-up scan at fine resolution (QA v0.1.1 M8).

    The long-term spectrum is 1/6-octave smoothed and read on a 1/24-octave grid as an
    octave-equivalent level, compared with the genre target, and the overall offset is removed
    (median over 125 Hz-8 kHz, like the octave tonal check). That residual is averaged over 2/3
    octave in dB, so narrow tonal peaks (single notes, resonances: the resonance detector's job)
    don't count, while a real build-up (a bell of Q <~ 2, about an octave or more wide) keeps
    most of its height. Excess = the highest averaged residual between 150 and 520 Hz. Centre =
    among the points within 1.5 dB of that maximum, the one that also stands out most against
    its neighbours 1.25 octaves either side (so a build-up sitting on a falling spectrum is
    located at its bump, not at the edge of the search range).
    Returns (excess_db, centre_hz, local_prominence_db) or (None, None, None)."""
    top = min(10000.0, 0.45 * sr)
    if top < 2000:
        return None, None, None
    fg = 2 ** np.arange(np.log2(50.0), np.log2(top), 1 / 24)
    sm = frac_oct_smooth(ltas, freqs, 6)
    r = 10 * np.log10(np.interp(fg, freqs, sm) * fg * 0.7071 + 1e-30) - target_curve_at(profile, fg)
    core = (fg >= 125) & (fg <= min(8000.0, top))
    r = r - np.median(r[core])
    rb = uniform_filter1d(r, 17, mode="nearest")          # 17 points of 1/24 oct = 2/3 octave
    lf = np.log2(fg)
    nb = 0.5 * (np.interp(lf - 1.25, lf, rb) + np.interp(lf + 1.25, lf, rb))
    prom = rb - nb
    mm = (fg >= MUD_SCAN_HZ[0]) & (fg <= MUD_SCAN_HZ[1])
    ex = float(rb[mm].max())
    near = mm & (rb >= ex - 1.5)
    i0 = int(np.argmax(np.where(near, rb + 0.5 * prom, -np.inf)))
    return ex, float(fg[i0]), float(prom[i0])


def _flatness(P, m):
    sub = P[m].astype(np.float64) + 1e-20
    return np.exp(np.mean(np.log(sub), axis=0)) / np.mean(sub, axis=0)


def _issue(key, title, severity, explanation, fix, details=None, category="tone"):
    s = float(np.clip(severity, 0, 100))
    level = "none" if s < 15 else "minor" if s < 40 else "moderate" if s < 70 else "severe"
    return {"key": key, "title": title, "severity": round(s, 1), "level": level,
            "explanation": explanation, "fix": fix, "details": details or {}, "category": category}


# ------------------------------------------------------------------ main
def _level_metrics(x, sr):
    ls = LN.loudness_stats(x, sr)
    integ = ls["integrated"]
    tp = LN.true_peak(x, sr, 4)
    tp_db = float(LN.db(tp))
    spk_db = float(LN.db(np.max(np.abs(x)) if x.size else 0))
    rms_db = float(10 * np.log10(np.mean(x ** 2) + 1e-20))
    kx = LN.kweight(x, sr)
    m = {
        "integrated_lufs": round(integ, 2), "lra_lu": round(ls["lra"], 2),
        "true_peak_dbtp": round(tp_db, 2), "sample_peak_dbfs": round(spk_db, 2),
        "plr_db": round(tp_db - integ, 2), "psr_db": round(tp_db - ls["short_term_max"], 2),
        "short_term_max_lufs": round(ls["short_term_max"], 2), "momentary_max_lufs": round(ls["momentary_max"], 2),
        "rms_dbfs": round(rms_db, 2), "crest_factor_db": round(spk_db - rms_db, 2),
    }
    return m, ls, kx, tp_db, spk_db


def level_metrics(x, sr):
    """Loudness / peak / dynamics block of the report (same keys as report['metrics'])."""
    x = np.nan_to_num(LN.as2d(np.asarray(x, np.float64)))
    return _level_metrics(x, sr)[0]


def analyze(x, sr, profile, path=None, progress=None, fast=False):
    """Analyze audio x (n, ch) float. Returns report dict."""
    x = np.nan_to_num(LN.as2d(np.asarray(x, np.float64)))
    n, nch = x.shape
    dur = n / sr
    P_ = (lambda p, msg: progress(p, msg)) if progress else (lambda p, msg: None)
    rep = {"file": str(path) if path else None, "sr": sr, "channels": nch, "duration_s": round(dur, 2),
           "profile": profile.get("display_name"), "warnings": [], "issues": []}
    if n < sr * 0.5:
        rep["warnings"].append("Very short file (< 0.5 s); measurements are unreliable.")

    # ---------------- loudness / dynamics
    P_(5, "loudness")
    m_metrics, ls, kx, tp_db, spk_db = _level_metrics(x, sr)
    integ = ls["integrated"]
    mom_p = LN._window_power(kx, sr, 0.4, 0.1)
    mom = LN._lufs(mom_p)
    rep["metrics"] = m_metrics
    rep["short_term_lufs"] = [round(float(v), 2) for v in ls["short_term"][::10]]  # 1 s resolution

    # ---------------- spectral front-end (brief 1.1)
    P_(15, "spectrum")
    freqs, times, P = _stft_power(x, sr, NFFT * (1 if sr <= 50000 else 2), HOP * (1 if sr <= 50000 else 2),
                                  max_frames=MAX_FRAMES // (2 if fast else 1))
    mi = np.clip(np.round((times - 0.2) / 0.1).astype(int), 0, max(mom.size - 1, 0))
    fmom = mom[mi] if mom.size else np.full(times.size, -70.0)
    active = (fmom > -50) & (fmom > integ - 20)
    if active.sum() < 3:
        active = np.ones_like(active)
    Pa = P[:, active]
    ltas = Pa.astype(np.float64).mean(axis=1) + 1e-20
    rel, valid, _abs = octave_band_levels(ltas, freqs, sr)

    # cutoff (1.5)
    ltas_db = 10 * np.log10(ltas)
    cutoff_hz, wall_db = None, None
    dc = DET["cutoff"]
    if sr / 2 > 14500:
        ref_m = (freqs >= dc["ref_band_hz"][0]) & (freqs <= dc["ref_band_hz"][1])
        lref = np.median(ltas_db[ref_m])
        below = ltas_db < lref - dc["drop_db"]
        df = freqs[1] - freqs[0]
        need = int(np.ceil(dc["min_run_hz"] / df))
        idx = np.flatnonzero(freqs > 12000)
        run = 0
        for i in idx:
            run = run + 1 if below[i] else 0
            if run >= need:
                cutoff_hz = float(freqs[i - need + 1])
                break
        if cutoff_hz:
            a = (freqs >= cutoff_hz - 500) & (freqs < cutoff_hz)
            b = (freqs >= cutoff_hz) & (freqs < cutoff_hz + 500)
            if np.any(a) and np.any(b):
                wall_db = float(np.mean(ltas_db[a]) - np.mean(ltas_db[b]))
    rep["cutoff_hz"] = cutoff_hz
    eff_top = cutoff_hz or sr / 2
    if cutoff_hz and cutoff_hz < 16000 * np.sqrt(2):
        valid[BAND_CENTERS.index(16000)] = valid[BAND_CENTERS.index(16000)] and cutoff_hz >= 16000

    target = np.asarray(profile["tonal_db"], float)
    # Deviation vs target after removing the overall offset (median over 125 Hz-8 kHz bands), so a
    # single odd 1 kHz band can't skew every band (levels are still reported re 1 kHz, brief 1.1).
    core = valid & (np.asarray(BAND_CENTERS) >= 125) & (np.asarray(BAND_CENTERS) <= 8000)
    offset = float(np.median((rel - target)[core])) if core.any() else 0.0
    dev = np.where(valid, rel - target - offset, 0.0)
    rep["tonal"] = {"bands_hz": BAND_CENTERS, "measured_db": [round(float(v), 2) for v in rel],
                    "target_db": target.tolist(), "offset_db": round(offset, 2), "deviation_db": [round(float(v), 2) for v in dev],
                    "valid": valid.tolist()}
    # spectrum curve for display (1/6-oct smoothed, ~240 log points)
    sm6 = frac_oct_smooth(ltas, freqs, 6)
    fd = np.geomspace(20, min(20000, sr / 2 * 0.99), 240)
    disp = 10 * np.log10(np.interp(fd, freqs, sm6) + 1e-20)
    rep["spectrum"] = {"freqs": [round(float(f), 1) for f in fd], "db": [round(float(v), 2) for v in disp]}
    rep["genre_suggestion"] = suggest_genre(rel, valid)
    try:
        from .profiles import suggest_blend
        _bl, _res = suggest_blend(rel, valid)
        if _bl and len(_bl) >= 2:
            rep["blend_suggestion"] = {"items": [{"key": k, "weight": w} for k, w in _bl],
                                       "spec": "+".join(f"{k}:{w:g}" for k, w in _bl),
                                       "display_name": " + ".join(f"{get_profile(k)['display_name']} {100 * w:.0f}%"
                                                                  for k, w in _bl),
                                       "residual_db": _res}
    except Exception:      # never let a suggestion break the analysis
        pass

    bi = {fc: i for i, fc in enumerate(BAND_CENTERS)}
    issues = rep["issues"]

    # ---------------- mud (1.2)
    P_(30, "tone")
    d = DET["mud"]
    mud_oct_ex = float(0.5 * (dev[bi[250]] + dev[bi[500]]))
    c3, l3 = third_oct_levels(ltas, freqs)
    l3_oct = l3 + 10 * np.log10(3) - _abs[bi[1000]]  # octave-equivalent, re 1k band
    t3 = target_curve_at(profile, c3)
    r3 = l3_oct - t3 - offset
    mm = (c3 >= 180) & (c3 <= 550)
    mud_ex, mud_fc, mud_prom = _mud_scan(ltas, freqs, profile, sr)
    if mud_ex is None:  # sample rate too low for the scan: fall back to the octave measure
        mud_ex, mud_fc, mud_prom = mud_oct_ex, 300.0, 0.0
    fb_mud = _band_power(Pa, freqs, 200, 500)
    fb_mid = _band_power(Pa, freqs, 500, 2000)
    mud_var = float(np.std(10 * np.log10(fb_mud / fb_mid)))
    # QA v0.2.0 M8: severity = distance to the target PLUS half of how far the bump stands out
    # from the song's own neighbouring bands (+-1.25 oct). A build-up that fills a natural dip
    # still reads high when it is a bump; a broad tilt toward the target alone does not.
    mud_score = float(mud_ex + MUD_PROM_WEIGHT * max(mud_prom, 0.0))
    s_mud = sev_map(mud_score - MUD_MAX_BIAS_DB, d["flag_excess_db"], d["severe_db"], 7.0)
    issues.append(_issue("mud", "Muddiness (150-500 Hz build-up)", s_mud,
        f"The low-mids around {mud_fc:.0f} Hz sit {mud_ex:+.1f} dB versus the {profile['display_name']} target and stand "
        f"{mud_prom:+.1f} dB out from the neighbouring bands. "
        + ("That makes the mix sound boxy and blurs kick, bass and vocals." if s_mud >= 30 else "This is within tolerance."),
        (f"Cut near {mud_fc:.0f} Hz, about {mud_cut_db(mud_score, d['max_cut_db']):.1f} dB"
         + (" (mostly static, the rest dynamic)." if mud_var > 2 else ".")) if s_mud >= 30 else "None.",
        {"excess_db": round(mud_ex, 2), "score_db": round(mud_score, 2), "center_hz": round(mud_fc, 1), "frame_std_db": round(mud_var, 2),
         "octave_excess_db": round(mud_oct_ex, 2), "local_prominence_db": round(mud_prom, 2),
         "third_oct_vs_target_db": {f"{c:.0f}": round(float(v), 2) for c, v in zip(c3[mm], r3[mm])}}))

    # ---------------- harshness (1.3), dynamic, 2048/512 frames
    d = DET["harsh"]
    f2, t2, P2 = _stft_power(x, sr, 2048, 512, max_frames=MAX_FRAMES * 2)
    mi2 = np.clip(np.round((t2 - 0.2) / 0.1).astype(int), 0, max(mom.size - 1, 0))
    act2 = (mom[mi2] > -50) & (mom[mi2] > integ - 20) if mom.size else np.ones(t2.size, bool)
    if act2.sum() < 3:
        act2 = np.ones(t2.size, bool)
    P2a = P2[:, act2]
    hr = 10 * np.log10(_band_power(P2a, f2, 2000, 5000) / _band_power(P2a, f2, 500, 2000))
    harsh_frac = float(np.mean(hr > np.median(hr) + d["frame_over_median_db"]))
    harsh_static = float(max(dev[bi[2000]], dev[bi[4000]]))
    s_h = max(sev_map(harsh_static, 2.0, 4.0, 7.0), sev_map(harsh_frac, d["flag_frame_fraction"], 0.25, 0.45))
    issues.append(_issue("harshness", "Harshness (2-5 kHz)", s_h,
        f"Upper mids measure {harsh_static:+.1f} dB vs target, and {100 * harsh_frac:.0f}% of loud moments spike more than "
        f"{d['frame_over_median_db']:.0f} dB above the song's typical 2-5 kHz level. "
        + ("Expect ear fatigue on vocals, guitars and snares." if s_h >= 30 else "Not a significant problem."),
        "Dynamic EQ 2-5 kHz (acts only on the spikes, max -3 dB)" + (" plus a gentle static correction." if harsh_static > 2 else "."),
        {"static_excess_db": round(harsh_static, 2), "spiky_frame_pct": round(100 * harsh_frac, 1),
         "dynamic_eq_needed": harsh_frac > d["flag_frame_fraction"]}))

    # sibilance (Master-Craft heuristic): 5-10 kHz bursts relative to 0.5-4 kHz that are also
    #  - narrowband: concentrated below ~10 kHz (5-10k vs 10-16k above its median; cymbals and snare
    #    crack are broadband up to 16k), and
    #  - transient: bursts of 30-300 ms (an 's'), not sustained distortion/grit, and
    #  - not a drum onset (frames where the low band or the whole spectrum jumps are excluded).
    sr_ratio = 10 * np.log10(_band_power(P2a, f2, 5000, min(10000, sr / 2)) / _band_power(P2a, f2, 500, 4000))
    burst = sr_ratio > np.median(sr_ratio) + 6.0
    sib_frac_raw = float(np.mean(burst))
    t2a = t2[act2]
    if sr / 2 > 12500:
        nb_r = 10 * np.log10(_band_power(P2a, f2, 5000, 10000) / (_band_power(P2a, f2, 10000, min(16000, sr / 2 * 0.98)) + 1e-20))
        narrow = nb_r > np.median(nb_r) + 3.0
    else:
        narrow = np.ones_like(burst)
    lowb = 10 * np.log10(_band_power(P2a, f2, 40, 150) + 1e-20)
    allb = 10 * np.log10(P2a.sum(axis=0) + 1e-20)
    jump = np.zeros_like(burst)
    jump[1:] = ((lowb[1:] - lowb[:-1]) > 4.0) | ((allb[1:] - allb[:-1]) > 4.0)
    jump[:-1] |= jump[1:]  # the frame before the onset frame overlaps the hit too
    cand = burst & narrow & ~jump
    sib = np.zeros_like(cand)
    i = 0
    nfr = cand.size
    while i < nfr:
        if cand[i]:
            j = i
            while j + 1 < nfr and cand[j + 1] and (t2a[j + 1] - t2a[j]) < 0.05:
                j += 1
            dur = t2a[j] - t2a[i] + 2048 / sr
            if 0.03 <= dur <= 0.3:
                sib[i:j + 1] = True
            i = j + 1
        else:
            i += 1
    sib_frac = float(np.mean(sib))
    sib_peak = float(np.percentile(sr_ratio[sib], 90) - np.median(sr_ratio)) if sib.sum() >= 5 else 0.0
    s_sib = max(sev_map(sib_frac, 0.03, 0.08, 0.15), sev_map(sib_peak - 6, 3, 7, 12) if sib.sum() >= 5 else 0.0)
    issues.append(_issue("sibilance", "Sibilance (5-10 kHz 's'/'t' bursts)", s_sib,
        f"{100 * sib_frac:.1f}% of active frames have narrowband 5-10 kHz bursts (30-300 ms, not on drum hits) more than "
        f"6 dB above normal (all 5-10 kHz bursts incl. cymbals/snare: {100 * sib_frac_raw:.1f}%). "
        + ("Esses will sound spitty, worse after limiting." if s_sib >= 30 else "Esses look controlled."),
        "Split-band de-esser 5-10 kHz, only on the bursts, drum hits protected." if s_sib >= 30 else "None.",
        {"burst_frame_pct": round(100 * sib_frac, 2), "raw_burst_frame_pct": round(100 * sib_frac_raw, 2),
         "excluded_drum_frames_pct": round(100 * float(np.mean(burst & jump)), 2),
         "excluded_broadband_frames_pct": round(100 * float(np.mean(burst & ~narrow)), 2),
         "p90_over_median_db": round(sib_peak, 2)}))

    # fizz (1.3)
    d = DET["fizz"]
    if sr / 2 > 12000:
        top = min(16000, eff_top, sr / 2)
        fr = 10 * np.log10(_band_power(P2a, f2, 8000, top) / _band_power(P2a, f2, 2000, 8000))
        flat = _flatness(P2a, (f2 >= 8000) & (f2 < top))
        fizz_frac = float(np.mean((flat > d["flatness_flag"]) & (fr > np.median(fr) + d["frame_over_median_db"])))
        fizz_static = float(dev[bi[8000]] if valid[bi[8000]] else 0.0)
        s_fz = max(sev_map(fizz_static, 2.0, 4.0, 7.0), sev_map(fizz_frac, 0.05, 0.15, 0.3))
        issues.append(_issue("fizz", "Fizz / sizzle (8-16 kHz)", s_fz,
            f"Top end is {fizz_static:+.1f} dB vs target; {100 * fizz_frac:.1f}% of frames show noisy, flat-spectrum sizzle bursts. "
            + ("This is the grainy 'AI sparkle' that limiting makes more obvious." if s_fz >= 30 else "The air band is clean enough."),
            "Dynamic band 8-14 kHz (max -3 dB). No top-end boosts.",
            {"static_excess_db": round(fizz_static, 2), "sizzle_frame_pct": round(100 * fizz_frac, 2),
             "hf_flatness_median": round(float(np.median(flat)), 3)}))

    # resonances (1.3) - persistent narrow peaks 2-8 kHz
    d = DET["resonance"]
    res = _resonances(P, freqs, times, active, sr, d)
    s_res = sev_map(len(res) and max(r["height_db"] for r in res), 3.0, 5.0, 8.0) if res else 0.0
    s_res = min(100.0, s_res + 8 * max(0, len(res) - 1)) if res else 0.0
    issues.append(_issue("resonances", "Metallic resonances (2-8 kHz)", s_res,
        (f"Found {len(res)} narrow peak(s) that repeat through the song at "
         + ", ".join(f"{r['freq_hz']:.0f} Hz" for r in res) + ". This is the 'metallic' fingerprint typical of Suno renders.")
        if res else "No persistent narrow resonances found.",
        f"Narrow notch per resonance (Q 4-8, -2 to -4 dB, at most {d['max_notches']})." if res else "None.",
        {"resonances": res}))

    # ---------------- noise / hiss (1.4)
    P_(50, "noise")
    d = DET["hiss"]
    # the noise floor is measured only in true gaps: momentary loudness < -35 LUFS for >= 0.9 s
    # (breaths inside phrases are short, so they never count as "noise"). Without such gaps the
    # floor is the 10th percentile of active frames and denoise only triggers on a high floor.
    gap_m = np.zeros(mom.size, bool)
    q = mom < -35 if mom.size else np.zeros(0, bool)
    i = 0
    while i < q.size:
        if q[i]:
            j = i
            while j + 1 < q.size and q[j + 1]:
                j += 1
            if j - i + 1 >= 6:
                gap_m[i:j + 1] = True
            i = j + 1
        else:
            i += 1
    quiet = gap_m[mi] if mom.size else np.zeros(times.size, bool)
    floor = (np.median(P[:, quiet], axis=1).astype(np.float64) + 1e-20 if quiet.sum() >= 10
             else np.percentile(Pa, 10, axis=1).astype(np.float64) + 1e-20)
    hiss = {"quiet_frames": int(quiet.sum()), "floor_source": "true gaps" if quiet.sum() >= 10 else "10th pct of active frames",
            "short_quiet_frames_ignored": int((fmom < -35).sum() - quiet.sum())}
    if sr / 2 > 8000:
        hi_top = min(16000, eff_top, sr / 2 * 0.98)
        hm = (freqs >= 4000) & (freqs < hi_top)
        c3f, l3f = third_oct_levels(floor, freqs, 4000, hi_top)
        floor_db = float(np.median(l3f)) if l3f.size else -200
        src = P[:, quiet] if quiet.sum() >= 10 else floor[:, None]
        flat_q = float(np.median(_flatness(src, hm)))
        act_med = float(np.median(10 * np.log10(_band_power(Pa, freqs, 4000, hi_top))))
        floor_tot = float(10 * np.log10(floor[hm].sum()))
        snr = act_med - floor_tot
        trig1 = floor_db > d["floor_flag_dbfs_per_third_oct"] and flat_q > d["flatness_flag"]
        trig2 = snr < 45 and flat_q > d["flatness_flag"] and quiet.sum() >= 10
        base = sev_map(floor_db - (d["floor_flag_dbfs_per_third_oct"] - 15), 15, 25, 35)  # -75 -> 30, -65 -> 70
        s_hs = base if (trig1 or trig2) else min(base, 20.0)
        hiss.update(floor_dbfs_per_third_oct=round(floor_db, 1), hf_flatness=round(flat_q, 3),
                    floor_to_active_db=round(snr, 1), triggered=bool(trig1 or trig2))
        issues.append(_issue("noise", "Broadband noise / hiss", s_hs,
            f"The steady high-frequency floor is {floor_db:.0f} dBFS per 1/3-octave with flatness {flat_q:.2f} "
            f"(1.0 = white noise). " + ("That is a constant hiss bed that limiting will raise." if (trig1 or trig2) and s_hs >= 30
                                       else "No significant hiss detected."),
            "Gentle spectral-gate denoise on the band above 4 kHz only (prop_decrease 0.3-0.5)." if s_hs >= 30 else "None.",
            hiss, category="repair"))

    # ---------------- codec / shimmer (1.5, 1.7 swirl)
    P_(60, "codec")
    d = DET["spectral_holes"]
    holes = _spectral_holes(Pa, freqs, eff_top, d["depth_db"])
    pre = _pre_echo(x, sr)
    codec_parts = []
    s_cut = 0.0
    if cutoff_hz:
        s_cut = sev_map(17500 - cutoff_hz, 1, 1500, 4000) if cutoff_hz < 17500 else 10
        codec_parts.append(f"energy stops at {cutoff_hz / 1000:.1f} kHz"
                           + (" with a codec-style brick wall" if (wall_db or 0) > 40 else ""))
    s_hole = sev_map(holes, DET["spectral_holes"]["flag_ratio"], 0.08, 0.15)
    if holes > DET["spectral_holes"]["flag_ratio"]:
        codec_parts.append(f"{100 * holes:.1f}% of high bins are 'holes'")
    s_pre = sev_map(pre["fraction"] if pre else 0, DET["pre_echo"]["flag_onset_fraction"], 0.4, 0.6)
    if pre and pre["fraction"] > DET["pre_echo"]["flag_onset_fraction"]:
        codec_parts.append(f"pre-echo before {100 * pre['fraction']:.0f}% of hits")
    s_codec = max(s_cut * 0.8, s_hole, s_pre)
    is_mp3 = bool(path and str(path).lower().endswith(".mp3"))
    if is_mp3:
        rep["warnings"].append("Input is MP3 (lossy). If you have Suno Pro/Premier, download the WAV instead for a cleaner master.")
        s_codec = max(s_codec, 25.0)
    if cutoff_hz and cutoff_hz < DET["cutoff"]["lossy_below_hz"]:
        rep["warnings"].append(f"Band-limited source: nothing above {cutoff_hz / 1000:.1f} kHz. EQ cannot recover it; "
                               "Master-Craft will not boost above the cutoff.")
    issues.append(_issue("codec", "Codec artifacts (cutoff, holes, pre-echo)", s_codec,
        ("Lossy-codec fingerprints: " + "; ".join(codec_parts) + ". Lost content cannot be recovered, only kept from getting worse.")
        if codec_parts else "No clear codec damage (full bandwidth, few spectral holes).",
        "No boosts above the cutoff; optional HF rebuild (off by default); keep limiting moderate." if codec_parts else "None.",
        {"cutoff_hz": cutoff_hz, "wall_db_per_500hz": round(wall_db, 1) if wall_db else None,
         "hole_ratio": round(holes, 4), "pre_echo": pre, "mp3_input": is_mp3}, category="repair"))

    # ---------------- clipping (1.6)
    P_(70, "clipping")
    d = DET["clipping"]
    runs = find_clip_runs(x, d["abs_threshold"], d["min_run"])
    nr = len(runs)
    longest = max((r[1] for r in runs), default=0)
    rps = nr / max(dur, 1e-3)
    hist_spike = _hist_spike(x)
    clipped = rps > d["flag_runs_per_s"] or longest > d["flag_max_run"] or hist_spike
    s_clip = max(sev_map(rps, d["flag_runs_per_s"], 5, 20), sev_map(longest, d["flag_max_run"], 25, 60),
                 40.0 if hist_spike else 0.0, min(25.0, nr * 2.0))
    issues.append(_issue("clipping", "Clipping (flat-topped samples)", s_clip,
        f"{nr} clipped run(s) ({rps:.2f}/s), longest {longest} samples. "
        + ("The waveform was squared off somewhere upstream; that is audible as crackle on loud hits." if clipped
           else "No meaningful clipping." if nr == 0 else "A few isolated clips; minor."),
        "Cubic declip of runs up to 32 samples, then -3 dB headroom before processing." if nr else "None.",
        {"runs": nr, "runs_per_s": round(rps, 3), "longest_run": longest, "histogram_spike": hist_spike},
        category="repair"))
    s_tp = sev_map(tp_db - (d["tp_flag_dbtp"] - 1.0), 1.0, 2.0, 3.5)  # -1 dBTP ->30, 0 ->70, +1.5 ->100
    issues.append(_issue("true_peak", "True-peak overs (inter-sample)", s_tp,
        f"True peak (4x oversampled) is {tp_db:+.2f} dBTP vs sample peak {spk_db:+.2f} dBFS. "
        + ("Above -1 dBTP, lossy encoding on Spotify/YouTube can distort." if tp_db > d["tp_flag_dbtp"] else "Safe."),
        "True-peak limiter to the ceiling (verified at 4x).", {"true_peak_dbtp": round(tp_db, 2)}, category="dynamics"))

    # ---------------- stereo (1.7)
    P_(80, "stereo")
    if nch == 2:
        st = _stereo(x, sr, profile)
        rep["stereo"] = st
        d = DET["stereo"]
        parts, sv = [], 0.0
        cw = profile["stereo"]["corr_window"]
        if st["corr"] < d["polarity_flip_corr"]:
            parts.append("one channel looks polarity-inverted"); sv = max(sv, 95)
        if st["bands"]["low"]["corr"] < d["bass_mono_corr"]:
            parts.append(f"bass is not mono (r={st['bands']['low']['corr']:.2f} below 150 Hz)")
            sv = max(sv, sev_map(d["bass_mono_corr"] - st["bands"]["low"]["corr"], 0.05, 0.3, 0.8) + 10)
        if any(b["corr"] < d["anti_phase_corr"] for b in st["bands"].values()) or st["neg_window_pct"] > 5:
            parts.append(f"anti-phase content ({st['neg_window_pct']:.1f}% of windows r<-0.2)")
            sv = max(sv, 60 + st["neg_window_pct"])
        if st["mono_loss_lu"] > d["mono_loss_lu"]:
            parts.append(f"loses {st['mono_loss_lu']:.1f} LU in mono"); sv = max(sv, 50 + 8 * st["mono_loss_lu"])
        narrow = st["sm_db"] < d["narrow_sm_db"] or (st["corr"] > 0.95 and cw[1] < 0.85)
        if narrow:
            parts.append(f"narrow image (side {st['sm_db']:.1f} dB under mid; r={st['corr']:.2f})")
            sv = max(sv, sev_map(d["narrow_sm_db"] - st["sm_db"] + 1, 1, 5, 10))
        if st["corr"] < cw[0] - 0.1:
            parts.append(f"very wide (r={st['corr']:.2f}, genre window {cw[0]:.2f}-{cw[1]:.2f})")
            sv = max(sv, 35)
        issues.append(_issue("stereo", "Stereo / phase / mono compatibility", sv,
            ("Stereo problems: " + "; ".join(parts) + ".") if parts else
            f"Healthy image (r={st['corr']:.2f}, side {st['sm_db']:.1f} dB under mid, mono fold {st['mono_loss_lu']:.1f} LU).",
            "Mono-safe bass (side removed below crossover); M/S width toward the genre window; no Haas tricks.",
            {k: v for k, v in st.items() if k != "bands"} | {"bands": st["bands"]}, category="stereo"))
        hfb, mdb = st["bands"]["high"], st["bands"]["mid"]
        swirl = (hfb["corr"] < 0.3 and mdb["corr"] > 0.7) or \
                (hfb["sm_db"] - mdb["sm_db"] > 6 and st["hf_corr_std"] > 0.25)
        s_sw = 60.0 if swirl else sev_map(max(0, mdb["corr"] - hfb["corr"] - 0.2), 0.2, 0.4, 0.6)
        rep["hf_swirl"] = bool(swirl)
    else:
        rep["stereo"] = None
        s_sw = 0.0
        swirl = False
    # Suno shimmer composite: swirl + resonances + fizz
    s_fz_v = next((i["severity"] for i in issues if i["key"] == "fizz"), 0.0)
    s_sh = max(s_sw, 0.6 * s_res + 0.4 * s_fz_v)
    issues.append(_issue("shimmer", "Suno-style high-end shimmer", s_sh,
        ("Phasey, swirling top end: the 4-16 kHz band is much less correlated between channels than the mids, which reads as "
         "'watery' shimmer. " if swirl else "") +
        ("Combined with the resonances/fizz above, the top end has the typical AI-render sheen." if s_sh >= 30 else
         "Little AI-style shimmer detected."),
        "Dynamic side-only cut 6-16 kHz when the image swirls; no widening of the top; fizz/resonance fixes." if s_sh >= 30 else "None.",
        {"hf_swirl": bool(swirl)}, category="repair"))

    # ---------------- low end (1.8)
    d = DET["low_end"]
    def_63, def_125, def_31 = -dev[bi[63]], -dev[bi[125]], -dev[bi[31.5]]
    deficit = float(max(def_63, def_125))
    excess = float(max(dev[bi[63]], dev[bi[125]]))
    sub_missing = bool(valid[bi[31.5]] and def_31 > 10)
    kb = float(10 * np.log10(_band_power(Pa, freqs, 40, 120).mean() / _band_power(Pa, freqs, 120, 500).mean()))
    if deficit >= excess:
        s_le = sev_map(deficit, d["deficit_flag_db"], 6, 10)
        expl = (f"Bass bands are {deficit:.1f} dB under the target. The song will sound thin on big speakers."
                if deficit > d["deficit_flag_db"] else "Low end is in range.")
        fixl = f"Low shelf near 90 Hz, up to +{d['max_shelf_db']:.0f} dB (after the mud cut)." if deficit > d["deficit_flag_db"] else "None."
        title = "Weak low end"
    else:
        s_le = sev_map(excess, 3.0, 6, 10)
        expl = (f"Bass bands are {excess:.1f} dB over the target: boomy, and it eats limiter headroom."
                if excess > 3 else "Low end is in range.")
        fixl = "Low shelf cut near 90 Hz (max -4 dB)." if excess > 3 else "None."
        title = "Boomy low end"
    if sub_missing:
        s_le = max(s_le, 45)
        expl += " The sub-bass (~30 Hz) is essentially missing; boosting it would only raise rumble."
        fixl += " Psychoacoustic bass (harmonics of 50-120 Hz) instead of a sub boost."
    issues.append(_issue("low_end", f"Low end ({title.split()[0].lower()})", s_le, expl, fixl,
        {"deficit_db": round(deficit, 2), "excess_db": round(excess, 2), "sub_missing": sub_missing,
         "kick_bass_ratio_db": round(kb, 2)}))

    # ---------------- dynamics / loudness
    plr = tp_db - integ
    s_dyn = sev_map(8.0 - plr, 1.0, 3.0, 5.0)   # PLR 7 ->30, 5 ->70, 3 ->100 (Master-Craft heuristic)
    issues.append(_issue("dynamics", "Dynamic range & loudness", s_dyn,
        f"Integrated {integ:.1f} LUFS, LRA {ls['lra']:.1f} LU, PLR {plr:.1f} dB, crest {m_metrics['crest_factor_db']:.1f} dB. "
        + ("Already heavily squashed: more loudness will cost punch." if plr < 7 else
           "Plenty of dynamic range to work with." if plr > 10 else "Moderate density."),
        "Glue + limiter sized so transients survive; loud targets are capped at 6 dB of limiting.",
        {}, category="dynamics"))

    # tonal balance overall
    ex = np.abs(dev[valid]) - TONAL["tolerance_db"]
    tb = float(np.sqrt(np.mean(np.maximum(ex, 0) ** 2))) if ex.size else 0
    worst = int(np.argmax(np.abs(dev)))
    issues.append(_issue("tonal_balance", "Tonal balance vs genre curve", sev_map(tb, 1.0, 2.5, 5.0),
        f"Average deviation beyond ±{TONAL['tolerance_db']:.0f} dB is {tb:.1f} dB; largest at {BAND_CENTERS[worst]:g} Hz "
        f"({dev[worst]:+.1f} dB). Nearest genre curve: {rep['genre_suggestion'][0][0]}.",
        f"Broad EQ correcting {int(TONAL['correction_fraction'] * 100)}% of deviations beyond tolerance, max ±{TONAL['max_db_per_band']:.0f} dB per band.",
        {}))

    # "already mastered" heuristic (research2 2.1 takeaway 3, CloudBounce-style): loud, dense and
    # sitting on the ceiling already. Informational only - the chain still runs, but the note tells
    # the user that a re-master is mostly re-limiting their own limiting.
    already = bool(integ > -11.0 and plr < 9.0 and tp_db > -1.0)
    rep["already_mastered"] = already
    if already:
        rep["warnings"].append(
            f"Input already looks mastered ({integ:.1f} LUFS, PLR {plr:.1f} dB, TP {tp_db:+.2f} dBTP): "
            "there is little headroom left to improve. If it is a previous master, start from the mix instead.")

    # artifact severity (brief 5: warm default when high)
    sv = {i["key"]: i["severity"] for i in issues}
    rep["artifact_severity"] = round(max(sv.get("shimmer", 0), sv.get("codec", 0), sv.get("resonances", 0),
                                         sv.get("fizz", 0)), 1)
    rep["overall_severity"] = round(float(np.mean(sorted(sv.values())[-4:])), 1)
    rep["_internal"] = {"mud_fc": mud_fc, "mud_var": mud_var, "resonances": res, "cutoff_hz": cutoff_hz,
                        "clip_runs": runs, "mom_lufs": mom, "dev": dev, "valid": valid, "swirl": swirl,
                        "sub_missing": sub_missing, "harsh_dyn": harsh_frac > DET["harsh"]["flag_frame_fraction"]}
    rep["issues"].sort(key=lambda i: -i["severity"])
    P_(100, "done")
    return rep


# ------------------------------------------------------------------ sub-detectors
def _resonances(P, freqs, times, active, sr, d):
    lo, hi = d["search_hz"]
    hi = min(hi, sr / 2 * 0.9)
    m = (freqs >= lo) & (freqs <= hi)
    if m.sum() < 10:
        return []

    def rcurve(spec):
        return 10 * np.log10(frac_oct_smooth(spec, freqs, 24) + 1e-20) - 10 * np.log10(frac_oct_smooth(spec, freqs, 3) + 1e-20)

    whole = rcurve(P[:, active].astype(np.float64).mean(axis=1) + 1e-20)
    pk, props = signal.find_peaks(np.where(m, whole, -99), height=d["peak_over_db"], distance=3)
    if pk.size == 0:
        return []
    # persistence across 10-second segments
    seg_len = 10.0
    edges = np.arange(times[0], times[-1] + seg_len, seg_len)
    seg_peaks = []
    for a in edges:
        sel = active & (times >= a) & (times < a + seg_len)
        if sel.sum() < 5:
            continue
        r = rcurve(P[:, sel].astype(np.float64).mean(axis=1) + 1e-20)
        p2, _ = signal.find_peaks(np.where(m, r, -99), height=d["peak_over_db"] * 0.75)
        seg_peaks.append(freqs[p2])
    out = []
    for i, h in zip(pk, props["peak_heights"]):
        f = freqs[i]
        if seg_peaks:
            pers = np.mean([np.any(np.abs(sp - f) <= 0.03 * f) for sp in seg_peaks])
        else:
            pers = 1.0
        if pers > d["persistence"]:
            out.append({"freq_hz": round(float(f), 1), "height_db": round(float(h), 2), "persistence": round(float(pers), 2)})
    out.sort(key=lambda r: -r["height_db"])
    merged = []
    for r in out:  # merge peaks within 5% (keep the strongest)
        if all(abs(r["freq_hz"] - m["freq_hz"]) > 0.05 * m["freq_hz"] for m in merged):
            merged.append(r)
    return merged[: d["max_notches"]]


def _spectral_holes(Pa, freqs, top, depth_db, max_frames=800):
    m = (freqs >= 5000) & (freqs < min(top, freqs[-1]))
    if m.sum() < 20 or Pa.shape[1] == 0:
        return 0.0
    sel = Pa[:, np.linspace(0, Pa.shape[1] - 1, min(max_frames, Pa.shape[1])).astype(int)]
    L = 10 * np.log10(sel[m].astype(np.float64) + 1e-20)
    df = freqs[1] - freqs[0]
    fc = np.sqrt(5000 * min(top, freqs[-1]))
    k = max(5, int(fc * (2 ** (1 / 6) - 2 ** (-1 / 6)) / df) | 1)
    # neighbourhood level: log-domain moving average over ±1/6 oct (fast stand-in for the median)
    med = uniform_filter1d(L, size=k, axis=0, mode="nearest")
    holes = (L < med - depth_db) & (med > -80)
    return float(holes.mean())


def _pre_echo(x, sr):
    """Spectral-flux onsets (n_fft 512 / hop 128); HF energy 5-20 ms vs 20-60 ms before."""
    if sr / 2 < 8000:
        return None
    mono = x.mean(axis=1)
    f, t, P = _stft_power(mono, sr, 512, 128, max_frames=200000)
    lp = np.log10(P + 1e-12)
    flux = np.maximum(np.diff(lp, axis=1), 0).sum(axis=0)
    if flux.size < 10:
        return None
    thr = np.median(flux) + 2.5 * np.std(flux)
    pk, _ = signal.find_peaks(flux, height=thr, distance=int(0.1 * sr / 128))
    if pk.size < 4:
        return {"onsets": int(pk.size), "fraction": 0.0}
    hf = apply_sos(mono, bandpass_sos(4000, min(16000, sr * 0.45), sr, 2)) ** 2
    cs = np.concatenate([[0.0], np.cumsum(hf)])
    on = (t[pk + 1] * sr).astype(int) - 256

    def e(a_ms, b_ms):
        a = np.clip(on - int(b_ms * sr / 1000), 0, len(hf))
        b = np.clip(on - int(a_ms * sr / 1000), 0, len(hf))
        return (cs[b] - cs[a]) / np.maximum(b - a, 1) + 1e-14
    r = 10 * np.log10(e(5, 20) / e(20, 60))
    frac = float(np.mean(r > DET["pre_echo"]["ratio_db"]))
    return {"onsets": int(pk.size), "fraction": round(frac, 3)}


def _hist_spike(x):
    a = np.abs(x).ravel()
    pk = a.max() if a.size else 0
    if pk < 0.1:
        return False
    h, _ = np.histogram(a, bins=1000, range=(0, pk))
    return bool(h[-1] > 10 * max(h[-2], 1) and h[-1] > 50)


def _stereo(x, sr, profile):
    L, R = x[:, 0], x[:, 1]
    M, S = (L + R) / 2, (L - R) / 2

    def c(a, b):
        den = np.sqrt(np.sum(a * a) * np.sum(b * b))
        return float(np.sum(a * b) / den) if den > 1e-20 else 1.0

    def smdb(m, s):
        return float(10 * np.log10((np.sum(s * s) + 1e-20) / (np.sum(m * m) + 1e-20)))

    bands = {}
    top = min(16000, sr * 0.45)
    for name, sos in (("low", lowpass_sos(150, sr, 4)), ("mid", bandpass_sos(150, 4000, sr, 2)),
                      ("high", bandpass_sos(4000, top, sr, 2))):
        b = apply_sos(x, sos)
        bm, bs = (b[:, 0] + b[:, 1]) / 2, (b[:, 0] - b[:, 1]) / 2
        bands[name] = {"corr": round(c(b[:, 0], b[:, 1]), 3), "sm_db": round(smdb(bm, bs), 2)}
    # windowed correlation (400 ms)
    w = max(1, min(int(0.4 * sr), x.shape[0]))
    nwin = max(1, x.shape[0] // w)
    Lw, Rw = L[: nwin * w].reshape(nwin, w), R[: nwin * w].reshape(nwin, w)
    num = (Lw * Rw).sum(1)
    den = np.sqrt((Lw ** 2).sum(1) * (Rw ** 2).sum(1))
    ok = den > 1e-8
    rw = np.where(ok, num / np.maximum(den, 1e-20), 1.0)
    hb = apply_sos(x, bandpass_sos(4000, top, sr, 2))
    hL, hR = hb[: nwin * w, 0].reshape(nwin, w), hb[: nwin * w, 1].reshape(nwin, w)
    hden = np.sqrt((hL ** 2).sum(1) * (hR ** 2).sum(1))
    hok = hden > 1e-10
    hr = (hL * hR).sum(1)[hok] / hden[hok] if hok.any() else np.array([1.0])
    lufs_st = LN.integrated_lufs(x, sr)
    lufs_mono = LN.integrated_lufs(np.stack([M, M], 1), sr)
    xm = apply_sos(x, highpass_sos(150, sr, 2))
    mm, ss = (xm[:, 0] + xm[:, 1]) / 2, (xm[:, 0] - xm[:, 1]) / 2
    return {"corr": round(c(L, R), 3), "sm_db": round(smdb(mm, ss), 2),
            "neg_window_pct": round(100 * float(np.mean(rw[ok] < -0.2)) if ok.any() else 0.0, 2),
            "mono_loss_lu": round(lufs_st - lufs_mono, 2), "hf_corr_std": round(float(np.std(hr)), 3),
            "balance_db": round(float(10 * np.log10((np.sum(L * L) + 1e-20) / (np.sum(R * R) + 1e-20))), 2),
            "bands": bands}


def summary_lines(rep):
    m = rep["metrics"]
    out = [f"{m['integrated_lufs']:.1f} LUFS | TP {m['true_peak_dbtp']:+.1f} dBTP | LRA {m['lra_lu']:.1f} LU | PLR {m['plr_db']:.1f} dB"] + \
        [f"[{i['severity']:5.1f}] {i['title']}: {i['explanation']}" for i in rep["issues"] if i["severity"] >= 15]
    bl = rep.get("blend_suggestion")
    if bl and bl.get("spec"):
        out.append(f"closest blend: -p \"{bl['spec']}\"  ({bl['display_name']})")
    return out
