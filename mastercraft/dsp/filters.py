"""Filters: RBJ biquads (min-phase, analog-style EQ), Linkwitz-Riley, and zero-phase
linear-phase FIR crossovers that sum back to the input exactly."""
import numpy as np
from scipy import signal


def _clampf(f, sr):
    return float(np.clip(f, 5.0, 0.49 * sr))


def peaking(fc, gain_db, q, sr):
    fc = _clampf(fc, sr)
    A = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * fc / sr
    al = np.sin(w0) / (2 * q)
    c = np.cos(w0)
    b = [1 + al * A, -2 * c, 1 - al * A]
    a = [1 + al / A, -2 * c, 1 - al / A]
    return np.array(b + a) / a[0]


def shelf(fc, gain_db, sr, kind="low", q=0.7071):
    fc = _clampf(fc, sr)
    A = 10 ** (gain_db / 40)
    w0 = 2 * np.pi * fc / sr
    al = np.sin(w0) / (2 * q)
    c = np.cos(w0)
    s = 2 * np.sqrt(A) * al
    if kind == "low":
        b = [A * ((A + 1) - (A - 1) * c + s), 2 * A * ((A - 1) - (A + 1) * c), A * ((A + 1) - (A - 1) * c - s)]
        a = [(A + 1) + (A - 1) * c + s, -2 * ((A - 1) + (A + 1) * c), (A + 1) + (A - 1) * c - s]
    else:
        b = [A * ((A + 1) + (A - 1) * c + s), -2 * A * ((A - 1) + (A + 1) * c), A * ((A + 1) + (A - 1) * c - s)]
        a = [(A + 1) - (A - 1) * c + s, 2 * ((A - 1) - (A + 1) * c), (A + 1) - (A - 1) * c - s]
    return np.array(b + a) / a[0]


def apply_sos(x, sos):
    if sos is None or len(sos) == 0:
        return x
    return signal.sosfilt(np.atleast_2d(sos), x, axis=0)


def highpass_sos(fc, sr, order=2):
    return signal.butter(order, _clampf(fc, sr), "highpass", fs=sr, output="sos")


def lowpass_sos(fc, sr, order=2):
    return signal.butter(order, _clampf(fc, sr), "lowpass", fs=sr, output="sos")


def lr4(fc, sr, kind):
    """Linkwitz-Riley 4th order = two cascaded 2nd-order Butterworth."""
    s = signal.butter(2, _clampf(fc, sr), kind, fs=sr, output="sos")
    return np.vstack([s, s])


def bandpass_sos(lo, hi, sr, order=2):
    hi = min(hi, 0.45 * sr)
    lo = min(lo, hi * 0.7)
    return signal.butter(order, [_clampf(lo, sr), _clampf(hi, sr)], "bandpass", fs=sr, output="sos")


# ---------- linear-phase (zero-phase) FIR crossover ----------
_fir_cache = {}


def fir_lowpass(fc, sr, sharp=6.0):
    """Odd-length symmetric FIR; used centred => zero phase delay."""
    fc = _clampf(fc, sr)
    n = int(sharp * sr / fc) | 1
    n = int(np.clip(n, 63, 16383)) | 1
    key = (round(fc, 3), sr, n)
    h = _fir_cache.get(key)
    if h is None:
        h = signal.firwin(n, fc, fs=sr, window="blackmanharris" if n < 4000 else "hamming")
        _fir_cache[key] = h
    return h


def fir_apply(x, h):
    return signal.oaconvolve(x, h[:, None] if x.ndim == 2 else h, mode="same", axes=0)


def lp_zero_phase(x, fc, sr):
    if fc >= 0.48 * sr:
        return x.copy()
    return fir_apply(x, fir_lowpass(fc, sr))


def split_bands(x, xovers, sr):
    """Zero-phase complementary split. Returns list of len(xovers)+1 bands; sum == x."""
    bands = []
    prev = np.zeros_like(x)
    for fc in sorted(xovers):
        lp = lp_zero_phase(x, fc, sr)
        bands.append(lp - prev)
        prev = lp
    bands.append(x - prev)
    return bands


def band_zero_phase(x, lo, hi, sr):
    """Zero-phase band (complementary-safe): x == band + (x - band)."""
    hi_lp = lp_zero_phase(x, hi, sr) if hi < 0.48 * sr else x
    lo_lp = lp_zero_phase(x, lo, sr) if lo > 10 else 0.0
    return hi_lp - lo_lp


def eq_response_db(sos_list, freqs, sr):
    """Magnitude response (dB) of cascaded biquads at freqs."""
    h = np.ones(len(freqs), complex)
    for s in sos_list:
        _, hh = signal.sosfreqz(np.atleast_2d(s), worN=np.asarray(freqs, float), fs=sr)
        h *= hh
    return 20 * np.log10(np.maximum(np.abs(h), 1e-9))
