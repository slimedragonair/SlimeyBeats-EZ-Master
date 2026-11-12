"""Reference-track matching: sit a master on another track's tonal balance (and optionally its
loudness and stereo width).

Own implementation of the idea used by Matchering (GPL-3.0, ideas only - no code taken),
Phantom's `match_to_reference` (AGPL-3.0, its masking analysis is patent-pending so nothing of it
is used here) and the "reference mastering" mode of the commercial services. The shared design
points are written down in docs/improvement_plan.md and docs/research2_eq_automastering.md §1.3:

  * analyse the loudest 50 % of the active frames (a chorus, not a whole song: verses make the LTAS
    dark and thin),
  * 1/3-octave smoothing, level-aligned over 100 Hz-10 kHz so the match is loudness-neutral,
  * a bounded correction, realised as a minimum-phase FIR (no pre-ringing on the kick),
  * the same for Mid and Side (width), and a delay compensation so the punch metrics still line up.

Nothing here imports Matchering or Phantom; the curve is computed with numpy/scipy on our own
STFT of the input.
"""
import numpy as np
from scipy import signal

from . import loudness as LN
from .analysis import _stft_power, frac_oct_smooth

GRID_LO, GRID_HI = 25.0, 16000.0
GRID_POINTS = 97                 # 1/12 octave from 25 Hz to 16 kHz
LEVEL_BAND = (100.0, 10000.0)    # level alignment / zero-mean band
MAX_DB_DEFAULT = 6.0


def _grid(sr):
    hi = min(GRID_HI, sr / 2 * 0.94)
    return np.geomspace(GRID_LO, hi, GRID_POINTS)


def _ltas_curve(x, sr, loudest_fraction=0.5):
    """Third-octave-ish spectrum (dB) of the loudest part, zero-mean over 100 Hz-10 kHz."""
    mono = x.mean(axis=1) if x.ndim == 2 else x
    nfft = 4096 if sr <= 50000 else 8192
    f, t, P = _stft_power(mono, sr, nfft, nfft // 2, max_frames=1500)
    tot = P.sum(axis=0)
    loud = 10 * np.log10(np.maximum(tot, 1e-20))
    act = loud > loud.max() - 40.0                 # active frames only
    if act.sum() < 4:
        act = np.ones_like(act)
    la = loud[act]
    thr = np.percentile(la, 100 * (1.0 - loudest_fraction))
    sel = act & (loud >= thr)
    if sel.sum() < 2:
        sel = act
    ltas = P[:, sel].astype(np.float64).mean(axis=1) + 1e-20
    sm = frac_oct_smooth(ltas, f, 6)
    g = _grid(sr)
    db = 10 * np.log10(np.maximum(np.interp(g, f, sm), 1e-20))
    band = (g >= LEVEL_BAND[0]) & (g <= LEVEL_BAND[1])
    return g, db - float(np.mean(db[band])) if band.any() else db


def _stereo_sm_db(x, sr):
    """Side/mid level ratio (dB) over 150 Hz-10 kHz."""
    if x.ndim != 2 or x.shape[1] != 2:
        return 0.0
    sos = signal.butter(2, [150.0, min(10000.0, sr * 0.45)], btype="bandpass", fs=sr, output="sos")
    b = signal.sosfilt(sos, x, axis=0)
    m, s = (b[:, 0] + b[:, 1]) * 0.5, (b[:, 0] - b[:, 1]) * 0.5
    return float(10 * np.log10((np.mean(s ** 2) + 1e-20) / (np.mean(m ** 2) + 1e-20)))


def target(x, sr, name=None, loudest_fraction=0.5):
    """Reference metrics from an audio array (see module docstring)."""
    g, db = _ltas_curve(x, sr, loudest_fraction)
    return {"name": name, "sr": int(sr), "freqs": g, "db": db,
            "lufs": float(LN.integrated_lufs(x, sr)),
            "plr_db": float(LN.db(LN.true_peak(x, sr, 4)) - LN.integrated_lufs(x, sr)),
            "sm_db": _stereo_sm_db(x, sr)}


def load_target(path, loudest_fraction=0.5):
    """Reference metrics from a file (uses the app's own loader, so MP3/FLAC/OGG work)."""
    from .audio_io import load_audio
    x, sr = load_audio(path)
    t = target(x, sr, name=str(path), loudest_fraction=loudest_fraction)
    return t


def match_gain_db(src_curve, ref_curve, strength=1.0, max_db=MAX_DB_DEFAULT, cutoff_hz=None):
    """Correction (dB, on the source grid) that moves `src_curve` onto `ref_curve`.

    Bounded to ±max_db, smoothed to ~1/3 octave, zero-mean over 100 Hz-10 kHz, and zero at/below
    30 Hz and above the codec cutoff (a reference cannot give back content that was never there)."""
    gs, ds = np.asarray(src_curve[0], float), np.asarray(src_curve[1], float)
    gr, dr = np.asarray(ref_curve[0], float), np.asarray(ref_curve[1], float)
    dref = np.interp(gs, gr, dr)                      # ref curve onto the source grid
    want = dref - ds
    # smooth ~1/3 octave (the grid is 1/12 octave: 4 bins)
    k = 5
    want = np.convolve(want, np.ones(k) / k, mode="same")
    want *= float(strength)
    band = (gs >= LEVEL_BAND[0]) & (gs <= LEVEL_BAND[1])
    want -= float(np.mean(want[band] if band.any() else want))
    want = np.clip(want, -max_db, max_db)
    want[gs < 30.0] = 0.0
    if cutoff_hz:
        want[gs > max(cutoff_hz - 200.0, 1000.0)] = 0.0
    return want


def design_fir(freqs, gain_db, sr, taps=4097):
    """Minimum-phase FIR for a dB curve (unity outside the band)."""
    nyq = sr / 2.0
    f = np.concatenate([[0.0], np.asarray(freqs, float), [nyq]])
    g = np.concatenate([[gain_db[0]], np.asarray(gain_db, float), [gain_db[-1]]])
    f = np.clip(f, 0.0, nyq)
    f = np.maximum.accumulate(f)                       # firwin2 needs a non-decreasing grid
    lin = 10 ** (g / 20.0)
    taps = int(max(255, min(taps, 16385)))
    if taps % 2 == 0:
        taps += 1
    h = signal.firwin2(taps, f / nyq, lin, antisymmetric=False)
    try:
        h = signal.minimum_phase(h, half=False)
    except TypeError:                                   # older SciPy without `half`
        h = signal.minimum_phase(h)
    return h


FIT_CENTERS = (40.0, 80.0, 160.0, 320.0, 700.0, 1600.0, 4000.0, 9000.0)
FIT_Q = 0.8


def design_chain(freqs, gain_db, sr, max_filters=7, q_min=0.5, q_max=1.4, gain_limit=None):
    """Greedy minimum-phase EQ chain that realises a target curve (broad peaking filters).

    Each step places a bell at the point of largest remaining deviation, estimates its Q from the
    width of that deviation, subtracts its response and repeats. Broad minimum-phase bells have no
    pre-ringing and a small, nearly constant group delay, so the kick is not smeared (a 4097-tap
    minimum-phase FIR matched slightly better but cost more punch). Filter count and Q are capped
    because every extra overlapping bell adds group-delay ripple in the 200-900 Hz region where the
    snare body lives: 9 filters @Q 2.5 cost 2.3 dB of measured punch on a hard cross-genre match,
    7 filters @Q 1.4 cost 1.2 dB for 0.4 dB more curve error.

    Returns ([(freq_hz, gain_db, q)], info)."""
    from .dsp import filters as F
    g = np.asarray(freqs, float)
    target = np.asarray(gain_db, float)
    hi = min(16000.0, sr * 0.45)
    m = (g >= 30.0) & (g <= hi)
    resid = np.where(m, target, 0.0)
    cap = float(np.max(np.abs(target[m]))) if m.any() else 6.0
    lim = float(gain_limit) if gain_limit is not None else max(1.0, cap)
    chain = []
    for _ in range(int(max(1, max_filters))):
        idx = int(np.argmax(np.abs(resid)))
        if abs(resid[idx]) < 0.4:
            break
        f0 = float(g[idx])
        gain = float(np.clip(resid[idx], -lim, lim))
        half = abs(gain) / 2.0
        # Q from the width of the deviation around f0 (notch half-width where it falls below half)
        left = idx
        while left > 0 and abs(resid[left]) >= half and g[left] > f0 / 4.0:
            left -= 1
        right = idx
        while right < resid.size - 1 and abs(resid[right]) >= half and g[right] < f0 * 4.0:
            right += 1
        bw = max(float(g[right] - g[left]), f0 * 0.15)
        q = float(np.clip(f0 / bw, q_min, q_max))
        chain.append((round(f0, 1), round(gain, 2), round(q, 2)))
        # keep the residual zero outside the fit band, otherwise the next step chases the edge
        resid = np.where(m, resid - F.eq_response_db([F.peaking(f0, gain, q, sr)], g, sr), 0.0)
    fit = F.eq_response_db([F.peaking(f, gg, q, sr) for f, gg, q in chain], g, sr) if chain else np.zeros_like(g)
    # judge the fit at ~1/3 octave: chasing 1/12-octave wiggles of a reference curve is neither
    # audible nor desirable (the smoothing is part of the design, not a failure of it)
    k = 5
    ts = np.convolve(np.where(m, target, 0.0), np.ones(k) / k, mode="same")
    fs = np.convolve(np.where(m, fit, 0.0), np.ones(k) / k, mode="same")
    info = {"filters": len(chain),
            "residual_max_db": round(float(np.max(np.abs((ts - fs)[m]))), 2) if m.any() else None,
            "residual_rms_db": round(float(np.sqrt(np.mean(((ts - fs)[m]) ** 2))), 2) if m.any() else None,
            "gain_cap_db": round(lim, 2),
            "response": [round(float(v), 2) for v in fit]}
    return chain, info


def _align(y, x, sr, max_lag_ms=40.0):
    """Compensate the FIR's group delay by cross-correlation of a flat mid band (±40 ms)."""
    n = min(len(x), len(y))
    if n < int(0.05 * sr):
        return y
    sos = signal.butter(2, [400.0, min(4000.0, sr * 0.45)], btype="bandpass", fs=sr, output="sos")
    xm = signal.sosfilt(sos, x[:n].mean(axis=1) if x.ndim == 2 else x[:n])
    ym = signal.sosfilt(sos, y[:n].mean(axis=1) if y.ndim == 2 else y[:n])
    d = max(1, int(sr / 4000))
    a, b = xm[::d], ym[::d]
    c = signal.correlate(b, a, mode="full", method="fft")
    lags = signal.correlation_lags(b.size, a.size, mode="full")
    lim = int(max_lag_ms * 1e-3 * sr / d)
    m = np.abs(lags) <= lim
    if not m.any():
        return y
    lag = int(lags[m][int(np.argmax(c[m]))]) * d
    if lag == 0:
        return y
    out = np.zeros_like(y)
    if lag > 0:                     # y is late by `lag`: drop the head
        if lag < len(y):
            out[: len(y) - lag] = y[lag:]
    else:
        k = -lag
        if k < len(y):
            out[k:] = y[: len(y) - k]
    return out


def apply(x, sr, freqs, gain_db, taps=4097, side_db=0.0, align=True, realize="biquads"):
    """Realise a match curve on the audio, then optionally match the side/mid ratio.

    realize="biquads" (default): 8 broad minimum-phase peaking filters fitted to the curve - the
    chain's own filter type, no pre-ringing, no delay compensation needed.
    realize="fir": one minimum-phase FIR (tighter fit, but its group delay moved the kick: -1.7 dB
    of measured punch on the reference test, so it is not the default)."""
    y = np.array(x, np.float64, copy=True)
    info = {}
    if realize == "fir":
        h = design_fir(freqs, gain_db, sr, taps)
        if y.ndim == 1:
            y = signal.lfilter(h, [1.0], y)
        else:
            for ch in range(y.shape[1]):              # lfilter needs a 1-D numerator
                y[:, ch] = signal.lfilter(h, [1.0], y[:, ch])
        if align:
            y = _align(y, np.asarray(x, np.float64), sr)
        info["realize"] = "fir"
        info["taps"] = int(h.size)
    else:
        from .dsp import filters as F
        chain_, finfo = design_chain(freqs, gain_db, sr)
        for f0, gg, q in chain_:
            y = F.apply_sos(y, F.peaking(f0, gg, q, sr))
        info.update({"realize": "biquads", "chain": chain_, **finfo})
        gain_db = np.asarray(gain_db, float)     # keep the report on the requested curve
    if abs(side_db) > 0.05 and y.ndim == 2 and y.shape[1] == 2:
        m, s = (y[:, 0] + y[:, 1]) * 0.5, (y[:, 0] - y[:, 1]) * 0.5
        s = s * 10 ** (side_db / 20.0)
        y = np.stack([m + s, m - s], axis=1)
    info.update({"max_gain_db": round(float(np.max(gain_db)), 2), "min_gain_db": round(float(np.min(gain_db)), 2),
                 "side_db": round(float(side_db), 2)})
    return y, info


def residual_bands(x, sr, ref, bands):
    """Measured-vs-reference difference (dB) on the profile's octave bands, for the report."""
    g, db = _ltas_curve(x, sr)
    dr = np.interp(np.asarray(g), np.asarray(ref["freqs"], float), np.asarray(ref["db"], float))
    out = {}
    for fc in bands:
        lo, hi = fc / np.sqrt(2), fc * np.sqrt(2)
        m = (g >= lo) & (g < hi)
        if not np.any(m) or hi > sr / 2 * 0.98:
            continue
        out[str(fc)] = round(float(np.mean(db[m]) - np.mean(dr[m])), 2)
    return out
