"""Saturation, stereo (M/S) and style filters."""
import numpy as np
from scipy import signal

from .filters import lp_zero_phase, band_zero_phase, apply_sos, lowpass_sos
from .dynamics import block_level_db, rms_window_db, gain_computer, _smooth_gr, coef, expand_gain


def saturate(x, sr, drive_db, mix, asym=0.08, oversample=2, band_hz=None, protect=None):
    """Parallel tanh saturation, slight asymmetry (even harmonics), 2x oversampled.
    Gain-compensated so wet and dry are level-matched; DC from asymmetry removed.
    protect: 0..1 per-sample drum-hit mask; the wet mix is faded out on the hits so the
    kick/snare attacks are never flattened (colour on the sustain only)."""
    if mix <= 0 or drive_db <= 0:
        return x
    src = lp_zero_phase(x, band_hz, sr) if band_hz else x
    os_ = max(1, int(oversample))
    up = signal.resample_poly(src, os_, 1, axis=0) if os_ > 1 else src
    d = 10 ** (drive_db / 20)
    pk = np.percentile(np.abs(up), 99.9) + 1e-9
    u = up / pk
    w = np.tanh(d * (u + asym)) - np.tanh(d * asym)
    w = w / (np.tanh(d) + 1e-9) * pk
    wet = signal.resample_poly(w, 1, os_, axis=0)[: x.shape[0]] if os_ > 1 else w
    wet = wet - wet.mean(axis=0, keepdims=True)
    rms_s = np.sqrt(np.mean(src ** 2) + 1e-20)
    rms_w = np.sqrt(np.mean(wet ** 2) + 1e-20)
    wet *= rms_s / rms_w
    if protect is not None:
        mx = mix * (1.0 - np.asarray(protect, np.float64))
        return x + (mx[:, None] if x.ndim == 2 else mx) * (wet - src)
    return x + mix * (wet - src)


def soft_clip(x, sr, drive_db=1.0, ceiling_lin=1.0, oversample=4):
    """Gentle oversampled soft clipper placed before the limiter: only the top few dB of the
    peaks are rounded (linear below `knee`), so the limiter has less to do and does not have
    to duck whole drum hits. Returns (y, info)."""
    if drive_db <= 0.05:
        return x, {}
    up = signal.resample_poly(x, oversample, 1, axis=0)
    c = ceiling_lin
    knee = c * 10 ** (-drive_db / 20)            # samples below this pass unchanged
    a = np.abs(up)
    over = a > knee
    span = c - knee
    out = up.copy()
    # smooth rational curve from knee to the ceiling: knee + span * u/(1+u), u=(a-knee)/span
    u = (a[over] - knee) / span
    out[over] = np.sign(up[over]) * (knee + span * u / (1.0 + u))
    y = signal.resample_poly(out, 1, oversample, axis=0)[: x.shape[0]]
    return y, {"clip_drive_db": round(drive_db, 2), "clipped_pct": round(100.0 * float(np.mean(over)), 3)}


def ms_encode(x):
    return (x[:, 0] + x[:, 1]) * 0.5, (x[:, 0] - x[:, 1]) * 0.5


def ms_decode(m, s):
    return np.stack([m + s, m - s], axis=1)


def corr(a, b):
    den = np.sqrt(np.sum(a * a) * np.sum(b * b))
    return float(np.sum(a * b) / den) if den > 1e-20 else 1.0


def stereo_process(x, sr, mono_below_hz=120.0, mono_amount=1.0, side_gain_db=0.0, widen_above_hz=300.0,
                   swirl_cut_db=0.0, swirl_lo=6000.0, swirl_hi=16000.0):
    """Mono-safe bass (side removed below crossover, zero-phase), M/S side gain above
    widen_above_hz (never Haas), optional dynamic side-only HF cut for phasey swirl."""
    if x.shape[1] != 2:
        return x, {}
    m, s = ms_encode(x)
    info = {}
    if mono_below_hz and mono_amount > 0:
        s_low = lp_zero_phase(s[:, None], mono_below_hz, sr)[:, 0]
        s = s - mono_amount * s_low
        info["mono_below_hz"] = mono_below_hz
    if abs(side_gain_db) > 0.05:
        s_lo = lp_zero_phase(s[:, None], widen_above_hz, sr)[:, 0]
        s = s_lo + (s - s_lo) * 10 ** (side_gain_db / 20)
        info["side_gain_db"] = round(side_gain_db, 2)
    if swirl_cut_db > 0.05:
        sb = band_zero_phase(s[:, None], swirl_lo, min(swirl_hi, 0.45 * sr), sr)[:, 0]
        mb = band_zero_phase(m[:, None], swirl_lo, min(swirl_hi, 0.45 * sr), sr)[:, 0]
        ls = rms_window_db(block_level_db(sb), 20, sr)
        lm = rms_window_db(block_level_db(mb), 20, sr)
        det = ls - lm
        thr = float(np.median(det))
        gr = np.minimum(gain_computer(det, thr, 2.5, 3.0), swirl_cut_db)
        gr = _smooth_gr(gr.astype(np.float64), coef(10, sr), coef(150, sr))
        g = expand_gain(gr, s.size)
        s = s + (g - 1.0) * sb
        info["swirl_gr_max_db"] = round(float(gr.max()), 2)
    return ms_decode(m, s), info


def style_lpf(x, sr, fc):
    return apply_sos(x, lowpass_sos(fc, sr, 2))
