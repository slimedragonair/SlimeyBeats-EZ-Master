"""Repair stages: declip, DC/rumble/polarity, HF-band denoise, HF rebuild, psychoacoustic bass."""
import numpy as np
from scipy import signal
from scipy.interpolate import CubicSpline

from .filters import highpass_sos, apply_sos, lp_zero_phase, bandpass_sos, lr4


def find_clip_runs(x, abs_thr=0.999, min_run=3, tol_db=0.05):
    """Runs (start, length, channel) of >= min_run same-sign samples that are either
    >= abs_thr, or flat-topped within tol_db of the file max (clip-then-normalize)."""
    runs = []
    peak = float(np.max(np.abs(x))) if x.size else 0.0
    if peak <= 1e-6:
        return runs
    near = peak * 10 ** (-tol_db / 20)
    for ch in range(x.shape[1]):
        v = x[:, ch]
        a = np.abs(v)
        cand_full = a >= abs_thr
        flat = np.zeros_like(cand_full)
        d = np.abs(np.diff(v)) <= 1e-5 * peak
        flat[1:] |= d
        flat[:-1] |= d
        cand = cand_full | ((a >= near) & flat)
        sgn = np.sign(v)
        idx = np.flatnonzero(np.diff(np.concatenate([[0], cand.view(np.int8), [0]])))
        for s, e in zip(idx[::2], idx[1::2]):
            if e - s >= min_run:
                # split on sign changes
                seg = sgn[s:e]
                cuts = np.flatnonzero(np.diff(seg)) + 1
                bounds = np.concatenate([[0], cuts, [e - s]])
                for b0, b1 in zip(bounds[:-1], bounds[1:]):
                    if b1 - b0 >= min_run:
                        runs.append((int(s + b0), int(b1 - b0), ch))
    return runs


def declip(x, runs, max_run=32, ctx=6, max_overshoot=1.41):
    """Cubic-spline reconstruction of clipped runs from bounding unclipped samples."""
    y = x.copy()
    n = x.shape[0]
    fixed = skipped = 0
    for s, L, ch in runs:
        if L > max_run:
            skipped += 1
            continue
        a, b = s - ctx, s + L + ctx
        if a < 0 or b > n:
            continue
        xi = np.r_[np.arange(a, s), np.arange(s + L, b)]
        cs = CubicSpline(xi, y[xi, ch])
        seg = cs(np.arange(s, s + L))
        orig = y[s:s + L, ch]
        # only allow reconstruction to extend beyond the clip level, same sign
        sgn = np.sign(orig.mean())
        seg = np.where(sgn * seg > np.abs(orig), seg, orig)
        lvl = float(np.max(np.abs(orig)))
        seg = np.clip(seg, -lvl * max_overshoot, lvl * max_overshoot)  # cap at +3 dB over the clip level
        y[s:s + L, ch] = np.clip(seg, -4, 4)
        fixed += 1
    return y, {"fixed_runs": fixed, "skipped_long_runs": skipped}


def dc_hpf_polarity(x, sr, hpf_hz=22.0, dc_remove=True, flip=False):
    y = x - x.mean(axis=0, keepdims=True) if dc_remove else x.copy()
    if flip and y.shape[1] == 2:
        y[:, 1] = -y[:, 1]
    if hpf_hz and hpf_hz > 0:
        # zero-phase (forward-backward 2nd order = 24 dB/oct) at 0.8 x the nominal corner: a
        # minimum-phase HPF rotates the phase of the 40-80 Hz kick fundamental and smears the hit,
        # a 1st-order one costs ~1 dB at 40-60 Hz (both measured on RuneCore). -0.35 dB at 40 Hz.
        y = signal.sosfiltfilt(highpass_sos(0.8 * hpf_hz, sr, 2), y, axis=0)
    return y


def _quiet_segments(x, sr, mom_lufs, mom_hop_s=0.1, thr=-35.0, min_s=1.0):
    q = mom_lufs < thr
    if q.sum() * mom_hop_s < min_s:
        return None
    hop = int(mom_hop_s * sr)
    idx = np.flatnonzero(q)
    segs = [x[i * hop: i * hop + hop] for i in idx]
    y = np.concatenate(segs, axis=0) if segs else None
    return y if y is not None and y.shape[0] > sr // 2 else None


def denoise_hf(x, sr, prop_decrease, n_std=1.75, split_hz=4000.0, noise_clip=None):
    """noisereduce (stationary) on the HF band only. Zero-phase complementary split keeps
    low-mid body untouched (brief 1.4)."""
    import noisereduce as nr
    low = lp_zero_phase(x, split_hz, sr)
    high = x - low
    kw = dict(sr=sr, stationary=True, prop_decrease=float(prop_decrease), n_std_thresh_stationary=float(n_std),
              freq_mask_smooth_hz=500, time_mask_smooth_ms=50, n_fft=2048)
    yn = None
    if noise_clip is not None:
        yn = (noise_clip - lp_zero_phase(noise_clip, split_hz, sr)).T
    try:
        hd = nr.reduce_noise(y=high.T, y_noise=yn, **kw)
    except TypeError:  # older API without some kwargs
        hd = nr.reduce_noise(y=high.T, sr=sr, stationary=True, prop_decrease=float(prop_decrease), y_noise=yn)
    hd = np.asarray(hd, float).T
    if hd.ndim == 1:
        hd = hd[:, None]
    hd = hd[: x.shape[0]]
    if hd.shape[0] < x.shape[0]:
        hd = np.pad(hd, ((0, x.shape[0] - hd.shape[0]), (0, 0)))
    return np.nan_to_num(low + hd)


def hf_rebuild(x, sr, fc, mix_db=-23.0, drive_db=9.0):
    """Brief 1.5 option: harmonics from fc/2..fc, saturated, HPF'd at fc (8th order), mixed low."""
    if fc is None or fc >= 0.45 * sr:
        return x
    src = apply_sos(x, bandpass_sos(fc / 2, fc, sr, 4))
    d = 10 ** (drive_db / 20)
    sat = np.tanh(src * d) / np.tanh(d)
    hp = apply_sos(sat, signal.butter(8, min(fc, 0.45 * sr), "highpass", fs=sr, output="sos"))
    return x + hp * 10 ** (mix_db / 20)


def psycho_bass(x, sr, mix_db=-17.0):
    """Brief 1.8: band-pass 50-120 Hz, soft-saturate, keep 120-360 Hz harmonics, mix low."""
    b = apply_sos(x, bandpass_sos(50, 120, sr, 2))
    pk = np.max(np.abs(b)) + 1e-9
    sat = np.tanh(3.0 * b / pk) * pk
    h = apply_sos(sat, bandpass_sos(120, 360, sr, 2))
    return x + h * 10 ** (mix_db / 20)
