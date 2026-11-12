"""Envelope followers, gain computers, compressors and dynamic EQ bands.
Control-rate processing (hop of a few samples) with numba when available."""
import numpy as np

from ..jit import njit
from .filters import band_zero_phase

HOP = 16


@njit
def _smooth_gr_auto(target_gr, att, rel_fast, rel_slow, slow_a):
    """Program-dependent release. s tracks the sustained (slow-averaged) gain reduction; when
    the current reduction is mostly a short transient peak (s << y) the fast release is used,
    so the compressor recovers right after a kick instead of leaving a pumping hole; when the
    whole passage is loud (s ~ y) it releases slowly and does not flutter."""
    out = np.empty_like(target_gr)
    y = 0.0
    s = 0.0
    for i in range(target_gr.shape[0]):
        t = target_gr[i]
        s = slow_a * s + (1.0 - slow_a) * t
        if t > y:
            y = att * y + (1.0 - att) * t
        else:
            w = s / y if y > 1e-6 else 1.0
            if w > 1.0:
                w = 1.0
            rel = w * rel_slow + (1.0 - w) * rel_fast
            y = rel * y + (1.0 - rel) * t
        out[i] = y
    return out


@njit
def _smooth_gr(target_gr, att, rel):
    # target_gr: positive dB of gain reduction wanted; attack when GR increases.
    out = np.empty_like(target_gr)
    y = 0.0
    for i in range(target_gr.shape[0]):
        t = target_gr[i]
        if t > y:
            y = att * y + (1.0 - att) * t
        else:
            y = rel * y + (1.0 - rel) * t
        out[i] = y
    return out


def coef(ms, sr, hop=HOP):
    ms = max(float(ms), 0.01)
    return float(np.exp(-hop / (ms * 1e-3 * sr)))


def block_level_db(x, hop=HOP, mode="rms"):
    """Linked (max over channels) block level in dB. x: (n,) or (n,ch)."""
    if x.ndim == 1:
        x = x[:, None]
    n = x.shape[0]
    nb = int(np.ceil(n / hop))
    pad = nb * hop - n
    xp = np.pad(x, ((0, pad), (0, 0)))
    b = xp.reshape(nb, hop, x.shape[1])
    if mode == "peak":
        v = np.max(np.abs(b), axis=1)
        lv = np.max(v, axis=1)
        return 20 * np.log10(np.maximum(lv, 1e-10))
    ms = np.mean(b ** 2, axis=1)
    lv = np.max(ms, axis=1)
    return 10 * np.log10(np.maximum(lv, 1e-20))


def rms_window_db(level_db_blocks, win_ms, sr, hop=HOP):
    """Smooth block power with a moving window (RMS detector)."""
    p = 10 ** (level_db_blocks / 10)
    w = max(1, min(int(win_ms * 1e-3 * sr / hop), p.size))
    if w > 1:
        k = np.ones(w) / w
        p = np.convolve(p, k, mode="same")
    return 10 * np.log10(np.maximum(p, 1e-20))


def gain_computer(level_db, thr, ratio, knee=6.0):
    """Static downward-compression curve. Returns positive GR dB."""
    over = level_db - thr
    gr = np.zeros_like(level_db)
    slope = 1.0 - 1.0 / max(ratio, 1.0001)
    if knee > 0:
        k2 = knee / 2
        m = over > k2
        gr[m] = over[m] * slope
        mk = (over > -k2) & ~m
        gr[mk] = slope * (over[mk] + k2) ** 2 / (2 * knee)
    else:
        gr = np.maximum(over, 0) * slope
    return gr


def expand_gain(gr_db_blocks, n, hop=HOP):
    """Interpolate block GR (dB) to per-sample linear gain."""
    t_b = (np.arange(gr_db_blocks.size) + 0.5) * hop
    g = np.interp(np.arange(n), t_b, gr_db_blocks)
    return 10 ** (-g / 20)


def compress(x, sr, thr, ratio, attack_ms, release_ms, knee=6.0, max_gr=None, detector_ms=5.0,
             key=None, makeup_db=0.0, auto_release=False):
    """Linked-stereo feed-forward compressor. Returns (y, gr_blocks_db).
    auto_release: program-dependent release (fast ~release/3 after short peaks, slow
    ~release*2.5 when gain reduction is sustained)."""
    k = x if key is None else key
    lv = rms_window_db(block_level_db(k), detector_ms, sr)
    gr = gain_computer(lv, thr, ratio, knee)
    if max_gr is not None:
        gr = np.minimum(gr, max_gr)
    if auto_release:
        gr = _smooth_gr_auto(gr.astype(np.float64), coef(attack_ms, sr), coef(release_ms / 3.0, sr),
                             coef(release_ms * 2.5, sr), coef(400.0, sr))
    else:
        gr = _smooth_gr(gr.astype(np.float64), coef(attack_ms, sr), coef(release_ms, sr))
    g = expand_gain(gr, x.shape[0]) * 10 ** (makeup_db / 20)
    return x * (g[:, None] if x.ndim == 2 else g), gr


def auto_threshold(x, sr, ratio, target_gr, pct=95.0, active_db=None):
    """threshold = P95(short-term RMS dBFS) - GR * r/(r-1)  (research brief Table B)."""
    lv = rms_window_db(block_level_db(x), 3000.0, sr)
    if active_db is not None:
        lv = lv[lv > active_db] if np.any(lv > active_db) else lv
    p = np.percentile(lv, pct)
    r = max(ratio, 1.01)
    return float(p - target_gr * r / (r - 1)), float(p)


def glue_compress(x, sr, ratio, attack_ms, release_ms, target_gr, knee=6.0, auto_release=False):
    """Wideband glue with auto threshold, verified by simulation and adjusted (±1 dB)."""
    if target_gr <= 0.05 or ratio <= 1.01:
        return x, {"threshold_db": None, "gr_loud_db": 0.0}
    floor = 10 * np.log10(np.mean(x ** 2) + 1e-20) - 20
    thr, p95 = auto_threshold(x, sr, ratio, target_gr, active_db=floor)
    y, gr = compress(x, sr, thr, ratio, attack_ms, release_ms, knee, detector_ms=10.0, auto_release=auto_release)
    st = rms_window_db(block_level_db(x), 3000.0, sr)
    loud = st >= np.percentile(st, 90)
    meas = float(np.mean(gr[loud])) if np.any(loud) else float(np.mean(gr))
    if meas > 1e-3:
        adj = float(np.clip((meas - target_gr) * ratio / (ratio - 1) * 0.7, -1.0, 1.0))
        if abs(meas - target_gr) > 0.25:
            thr += adj
            y, gr = compress(x, sr, thr, ratio, attack_ms, release_ms, knee, detector_ms=10.0, auto_release=auto_release)
            meas = float(np.mean(gr[loud])) if np.any(loud) else float(np.mean(gr))
    return y, {"threshold_db": round(thr, 2), "gr_loud_db": round(meas, 2), "gr_max_db": round(float(gr.max()), 2)}


def _block_mask(protect, nb, hop=HOP):
    """Per-sample 0..1 protect mask -> per-block max."""
    n = protect.size
    pad = nb * hop - n
    pm = np.pad(protect, (0, max(pad, 0)))[: nb * hop]
    return pm.reshape(nb, hop).max(axis=1)


def dynamic_band(x, sr, lo, hi, max_cut_db, ratio, attack_ms, release_ms, thresh_over_median_db,
                 relative_to_broadband=False, detector_ms=5.0, active_floor_db=-60.0, channel_mask=None,
                 protect=None):
    """Dynamic EQ band: zero-phase band extracted, reduced when it exceeds
    (its own median level + offset) [or its median ratio to broadband], then recombined:
        y = x + (g - 1) * band   (exact passthrough when g == 1)."""
    band = band_zero_phase(x, lo, hi, sr)
    lb = rms_window_db(block_level_db(band), detector_ms, sr)
    lf = rms_window_db(block_level_db(x), detector_ms, sr)
    act = lf > max(active_floor_db, np.percentile(lf, 99) - 40)
    if not np.any(act):
        return x, {"gr_max_db": 0.0, "active_pct": 0.0}
    det = lb - lf if relative_to_broadband else lb
    thr = float(np.median(det[act]) + thresh_over_median_db)
    gr = gain_computer(det, thr, ratio, knee=3.0)
    gr[~act] = 0.0
    gr = np.minimum(gr, max_cut_db)
    prot_pct = 0.0
    if protect is not None:
        # transient-aware: no new gain reduction is started on a drum hit (the hit is passed
        # through untouched; reduction already running from a sustained 's' or harsh note
        # is allowed to continue but not deepen)
        pb = _block_mask(protect, gr.size)
        prot_pct = 100.0 * float(np.mean(pb > 0.5))
        gr = gr * (1.0 - pb)
    gr = _smooth_gr(gr.astype(np.float64), coef(attack_ms, sr), coef(release_ms, sr))
    g = expand_gain(gr, x.shape[0])
    if x.ndim == 2:
        g2 = g[:, None]
        delta = (g2 - 1.0) * band
        if channel_mask is not None:
            delta = delta * np.asarray(channel_mask)[None, :]
        y = x + delta
    else:
        y = x + (g - 1.0) * band
    return y, {"gr_max_db": round(float(gr.max()), 2), "gr_mean_db": round(float(gr[act].mean()), 2),
               "active_pct": round(100.0 * float(np.mean(gr > 0.5)), 1), "threshold_db": round(thr, 2),
               **({"protected_pct": round(prot_pct, 1)} if protect is not None else {})}
