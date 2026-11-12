"""BS.1770-4 / EBU R128 loudness measurement, true peak and related metrics."""
import numpy as np
from scipy import signal


def _kweight_sos(sr):
    # High shelf (+4 dB @ ~1.5 kHz) followed by RLB high-pass (~38 Hz); same as pyloudnorm.
    G, Q, fc = 3.99984385397, 0.7071752369554193, 1681.9744509555319
    A = 10 ** (G / 40.0)
    w0 = 2 * np.pi * fc / sr
    alpha = np.sin(w0) / (2 * Q)
    c = np.cos(w0)
    b0 = A * ((A + 1) + (A - 1) * c + 2 * np.sqrt(A) * alpha)
    b1 = -2 * A * ((A - 1) + (A + 1) * c)
    b2 = A * ((A + 1) + (A - 1) * c - 2 * np.sqrt(A) * alpha)
    a0 = (A + 1) - (A - 1) * c + 2 * np.sqrt(A) * alpha
    a1 = 2 * ((A - 1) - (A + 1) * c)
    a2 = (A + 1) - (A - 1) * c - 2 * np.sqrt(A) * alpha
    shelf = np.array([b0, b1, b2, a0, a1, a2]) / a0
    Q2, fc2 = 0.5003270373253953, 38.13547087613982
    w0 = 2 * np.pi * fc2 / sr
    alpha = np.sin(w0) / (2 * Q2)
    c = np.cos(w0)
    b0, b1, b2 = (1 + c) / 2, -(1 + c), (1 + c) / 2
    a0, a1, a2 = 1 + alpha, -2 * c, 1 - alpha
    hp = np.array([b0, b1, b2, a0, a1, a2]) / a0
    return np.vstack([shelf, hp])


def kweight(x, sr):
    """x: (n, ch). Returns K-weighted copy."""
    return signal.sosfilt(_kweight_sos(sr), x, axis=0)


def _window_power(kx, sr, win_s, hop_s):
    """Mean-square power (channel-summed, BS.1770 weights=1 for L/R) for sliding windows."""
    n = kx.shape[0]
    win = int(round(win_s * sr))
    hop = max(1, int(round(hop_s * sr)))
    p = np.sum(kx.astype(np.float64) ** 2, axis=1)
    if n < win:
        return np.array([p.mean() if n else 0.0])
    cs = np.concatenate([[0.0], np.cumsum(p)])
    starts = np.arange(0, n - win + 1, hop)
    return (cs[starts + win] - cs[starts]) / win


def _lufs(p):
    return -0.691 + 10 * np.log10(np.maximum(p, 1e-20))


def integrated_from_k(kx, sr):
    pw = _window_power(kx, sr, 0.4, 0.1)
    l = _lufs(pw)
    g = pw[l > -70]
    if g.size == 0:
        return -70.0
    rel = _lufs(g.mean()) - 10
    g2 = pw[(l > -70) & (l > rel)]
    if g2.size == 0:
        return -70.0
    return float(_lufs(g2.mean()))


def integrated_lufs(x, sr):
    x = as2d(x)
    return integrated_from_k(kweight(x, sr), sr)


def loudness_stats(x, sr):
    """Integrated, LRA, short-term max, momentary max (LUFS) plus short-term curve."""
    x = as2d(x)
    kx = kweight(x, sr)
    integ = integrated_from_k(kx, sr)
    mom = _lufs(_window_power(kx, sr, 0.4, 0.1))
    st_p = _window_power(kx, sr, 3.0, 0.1)
    st = _lufs(st_p)
    # LRA (EBU Tech 3342)
    g = st_p[st > -70]
    lra = 0.0
    if g.size > 1:
        rel = _lufs(g.mean()) - 20
        gs = st[(st > -70) & (st > rel)]
        if gs.size > 1:
            lra = float(np.percentile(gs, 95) - np.percentile(gs, 10))
    return {
        "integrated": float(integ),
        "lra": lra,
        "short_term_max": float(np.max(st)) if st.size else -70.0,
        "momentary_max": float(np.max(mom)) if mom.size else -70.0,
        "short_term": st,
    }


def true_peak(x, sr=None, oversample=4, chunk=1 << 20):
    """Oversampled (default 4x) true peak, linear. Chunked for long files."""
    x = as2d(x)
    if oversample <= 1:
        return float(np.max(np.abs(x))) if x.size else 0.0
    peak = 0.0
    pad = 64
    n = x.shape[0]
    for s in range(0, n, chunk):
        a = max(0, s - pad)
        b = min(n, s + chunk + pad)
        e = min(n, s + chunk)
        y = signal.resample_poly(x[a:b], oversample, 1, axis=0)
        # keep only this chunk's own span: the pads exist so the filter has context, but the
        # reconstruction rings at the artificial cut edges and gave phantom overs (QA H1)
        y = y[(s - a) * oversample:(e - a) * oversample]
        peak = max(peak, float(np.max(np.abs(y))) if y.size else 0.0,
                   float(np.max(np.abs(x[s:e]))) if e > s else 0.0)
    return peak


def oversampled_abs_max(x, oversample=4, chunk=1 << 19):
    """Per-original-sample max |.| over the oversampled reconstruction (max over channels).
    Returns array of length n (linear)."""
    x = as2d(x)
    n = x.shape[0]
    out = np.empty(n)
    if oversample <= 1:
        return np.max(np.abs(x), axis=1)
    pad = 64
    for s in range(0, n, chunk):
        a = max(0, s - pad)
        b = min(n, s + chunk + pad)
        y = signal.resample_poly(x[a:b], oversample, 1, axis=0)
        y = np.max(np.abs(y), axis=1)
        m = y[: (b - a) * oversample].reshape(-1, oversample).max(axis=1)
        e = min(n, s + chunk)
        out[s:e] = np.maximum(m[s - a: e - a], np.max(np.abs(x[s:e]), axis=1))
    return out


def db(x, floor=-200.0):
    return np.maximum(20 * np.log10(np.maximum(np.abs(x), 1e-12)), floor)


def as2d(x):
    x = np.asarray(x)
    if x.ndim == 1:
        x = x[:, None]
    return x
