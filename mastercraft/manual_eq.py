"""Post-master manual EQ (FL-Studio-style 7-band parametric) with a true-peak guard.

Signal order: auto chain (master()) -> manual EQ -> hold gain + output fader -> true-peak guard.
The same streaming processor runs the live preview (small blocks from the audio callback) and the
offline export (whole file, fixed settings), so the export is the preview. Filters are RBJ
biquads (minimum phase, direct form I).

Transitions (v0.2.1, QA ear-safety): coefficients are never glided. Every change builds a new
filter chain at the new settings, warms it on the last ~170 ms of input (so it starts in its
steady state), and the output crossfades from the old chain to the new one over 20 ms. The
crossfade weights sum to 1, so every output sample lies between the old and the new chain's
sample: no transition can be louder than the louder of the two steady states (hard bound,
0 dB). The output/hold gain is part of the chain, so it moves in step with the bands.
"""
import json
import math
from pathlib import Path

import numpy as np
from scipy import signal

from . import loudness as LN
try:   # the kernels run in the audio callback and in a solver thread: let them drop the GIL
    from numba import njit as _nj

    def njit(fn):
        return _nj(cache=False, nogil=True)(fn)
except Exception:  # pragma: no cover
    from .jit import njit

NBANDS = 7
TYPES = ["bell", "low_shelf", "high_shelf", "low_cut", "high_cut"]
TYPE_LABELS = {"bell": "Bell", "low_shelf": "Low shelf", "high_shelf": "High shelf", "low_cut": "Low cut", "high_cut": "High cut"}
GAIN_RANGE = 18.0
OUT_RANGE = 12.0          # output fader +/- dB
HOLD_CAP_DB = 30.0        # hold gain is never more than this either way
F_MIN, F_MAX = 20.0, 20000.0
Q_MIN, Q_MAX = 0.1, 18.0
COLORS = ["#a46ce0", "#e05cc0", "#e07a5c", "#d8d05a", "#6cd86c", "#4fc9b0", "#5c9ce0"]
REGIONS = [("SUB", 20, 60), ("BASS", 60, 250), ("LOW MID", 250, 500), ("MID", 500, 2000),
           ("HIGH MID", 2000, 4000), ("PRS", 4000, 6000), ("TREBLE", 6000, 20000)]
NOTES = [(f"C{k}", 16.3516 * 2 ** k) for k in range(1, 10)]          # C1 32.7 Hz .. C9 8372 Hz
DEFAULT_BANDS = [("low_shelf", 50.0, 0.707), ("bell", 120.0, 1.0), ("bell", 300.0, 1.0), ("bell", 800.0, 1.0),
                 ("bell", 2000.0, 1.0), ("bell", 5000.0, 1.0), ("high_shelf", 10000.0, 0.707)]
MACROS = {   # name -> list of (band index, type, freq, gain, q). Absolute settings: applying twice = once.
    "Brightness / Air": [(6, "high_shelf", 6000.0, 2.5, 0.707)],
    "Low-end weight": [(0, "low_shelf", 90.0, 2.0, 0.707)],
    "Vocal presence": [(4, "bell", 3000.0, 2.0, 1.0)],
}
XFADE_MS = 20.0           # crossfade between old and new chain
WARM_MS = 170.0           # history the new chain is warmed on (minimum)
# v0.2.2 (QA v0.2.1: low-frequency high-Q jumps): warm-up and crossfade scale with the slowest pole of
# the chains involved (decay time constant tau ~ Q/(pi f)); warm = WARM_TAU x tau, fade = XFADE_TAU x tau
WARM_TAU = 8.0
WARM_MAX_MS = 1500.0
XFADE_TAU = 1.5
XFADE_MAX_MS = 300.0
GAIN_RAMP_MS = 80.0      # gain-only changes (OUT fader, hold gain): dB-domain raised-cosine ramp
LOOKAHEAD_MS = 2.0        # true-peak guard
RELEASE_MS = 60.0
TP_MARGIN_DB = 0.15       # guard ceiling under the export ceiling (covers the 4x interpolator's error)


# ------------------------------------------------------------------ state
def default_band(i):
    t, f, q = DEFAULT_BANDS[i]
    return {"type": t, "freq": f, "gain": 0.0, "q": q, "on": True}


def default_state():
    return {"version": 2, "bands": [default_band(i) for i in range(NBANDS)], "bypass": False,
            "hold_loudness": True, "out_gain_db": 0.0}


def _bool(v, default):
    """JSON bools, and strings such as "false"/"0"/"off" written by hand (QA v0.2.0 bug 9)."""
    if isinstance(v, bool):
        return v
    if isinstance(v, (int, float)) and math.isfinite(v):
        return v != 0
    if isinstance(v, str):
        s = v.strip().lower()
        if s in ("true", "1", "yes", "on"):
            return True
        if s in ("false", "0", "no", "off", ""):
            return False
    return default


def _num(v, default, lo, hi):
    try:
        f = float(v)
    except (TypeError, ValueError):
        return default
    return float(np.clip(f, lo, hi)) if math.isfinite(f) else default


def clean_state(st):
    """Validated copy (missing keys filled, ranges clamped). Accepts presets from older versions."""
    out = default_state()
    if not isinstance(st, dict):
        return out
    for i, b in enumerate((st.get("bands") or [])[:NBANDS]):
        if not isinstance(b, dict):
            continue
        d = out["bands"][i]
        if b.get("type") in TYPES:
            d["type"] = b["type"]
        d["freq"] = _num(b.get("freq", d["freq"]), d["freq"], F_MIN, F_MAX)
        d["gain"] = _num(b.get("gain", d["gain"]), d["gain"], -GAIN_RANGE, GAIN_RANGE)
        d["q"] = _num(b.get("q", d["q"]), d["q"], Q_MIN, Q_MAX)
        d["on"] = _bool(b.get("on", True), True)
    out["bypass"] = _bool(st.get("bypass", False), False)
    out["hold_loudness"] = _bool(st.get("hold_loudness", True), True)
    out["out_gain_db"] = _num(st.get("out_gain_db", 0.0), 0.0, -OUT_RANGE, OUT_RANGE)
    return out


def is_flat(st):
    return st["bypass"] or (abs(st.get("out_gain_db", 0.0)) < 1e-9 and all(
        (not b["on"]) or (b["type"] in ("bell", "low_shelf", "high_shelf") and abs(b["gain"]) < 1e-9) for b in st["bands"]))


def read_json(path):
    """JSON file, with or without a UTF-8 BOM (PowerShell 5 writes one)."""
    return json.loads(Path(path).read_text(encoding="utf-8-sig"))


def save_preset(st, path):
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    tmp = p.with_suffix(p.suffix + ".tmp")
    tmp.write_text(json.dumps(clean_state(st), indent=2), encoding="utf-8")
    tmp.replace(p)
    return p


def load_preset(path):
    return clean_state(read_json(path))


def sidecar_path(master_path):
    """Per-song EQ state, next to the master it belongs to: `<master>.eq.json`."""
    p = Path(master_path)
    return p.with_name(p.stem + ".eq.json")


def file_hash(path, chunk=1 << 20):
    """SHA-1 of a file (the sidecar remembers which master it was made on)."""
    import hashlib
    h = hashlib.sha1()
    with open(path, "rb") as f:
        while True:
            b = f.read(chunk)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def apply_macro(st, name):
    """Set the macro's band(s) to the macro's values. Idempotent: applying it twice = once."""
    st = clean_state(st)
    for i, t, f, g, q in MACROS[name]:
        st["bands"][i].update(type=t, freq=f, gain=g, q=q, on=True)
    return st


def macro_active(st, name):
    return all(st["bands"][i]["type"] == t and abs(st["bands"][i]["freq"] - f) < 1e-6 and abs(st["bands"][i]["gain"] - g) < 1e-6
               and st["bands"][i]["on"] for i, t, f, g, q in MACROS[name])


# ------------------------------------------------------------------ filters
@njit
def _coefs(t, f, g, q, sr, out):
    """RBJ cookbook biquad, normalised (b0 b1 b2 a1 a2) into out."""
    f = min(max(f, 5.0), 0.49 * sr)
    w = 2.0 * np.pi * f / sr
    cw, sw = np.cos(w), np.sin(w)
    al = sw / (2.0 * q)
    A = 10.0 ** (g / 40.0)
    if t == 0:      # bell
        b0, b1, b2 = 1 + al * A, -2 * cw, 1 - al * A
        a0, a1, a2 = 1 + al / A, -2 * cw, 1 - al / A
    elif t == 1:    # low shelf
        sa = 2 * np.sqrt(A) * al
        b0 = A * ((A + 1) - (A - 1) * cw + sa); b1 = 2 * A * ((A - 1) - (A + 1) * cw); b2 = A * ((A + 1) - (A - 1) * cw - sa)
        a0 = (A + 1) + (A - 1) * cw + sa; a1 = -2 * ((A - 1) + (A + 1) * cw); a2 = (A + 1) + (A - 1) * cw - sa
    elif t == 2:    # high shelf
        sa = 2 * np.sqrt(A) * al
        b0 = A * ((A + 1) + (A - 1) * cw + sa); b1 = -2 * A * ((A - 1) + (A + 1) * cw); b2 = A * ((A + 1) + (A - 1) * cw - sa)
        a0 = (A + 1) - (A - 1) * cw + sa; a1 = 2 * ((A - 1) - (A + 1) * cw); a2 = (A + 1) - (A - 1) * cw - sa
    elif t == 3:    # low cut (high-pass, 12 dB/oct)
        b0, b1, b2 = (1 + cw) / 2, -(1 + cw), (1 + cw) / 2
        a0, a1, a2 = 1 + al, -2 * cw, 1 - al
    else:           # high cut (low-pass, 12 dB/oct)
        b0, b1, b2 = (1 - cw) / 2, 1 - cw, (1 - cw) / 2
        a0, a1, a2 = 1 + al, -2 * cw, 1 - al
    out[0] = b0 / a0; out[1] = b1 / a0; out[2] = b2 / a0; out[3] = a1 / a0; out[4] = a2 / a0


def band_coefs(b, sr):
    c = np.zeros(5)
    _coefs(TYPES.index(b["type"]), float(b["freq"]), float(b["gain"]), float(b["q"]), float(sr), c)
    return c


def band_response_db(b, freqs, sr):
    c = band_coefs(b, sr)
    _, h = signal.freqz([c[0], c[1], c[2]], [1.0, c[3], c[4]], worN=np.asarray(freqs, float), fs=sr)
    return 20 * np.log10(np.maximum(np.abs(h), 1e-9))


def response_db(st, freqs, sr):
    """Total EQ magnitude (dB) of a state's bands (without hold gain / output fader)."""
    tot = np.zeros(len(freqs))
    if st["bypass"]:
        return tot
    for b in st["bands"]:
        if b["on"]:
            tot += band_response_db(b, freqs, sr)
    return tot


TP_TAPS = 32


def _tp_fir():
    """4x interpolation taps for the points 1/4, 2/4, 3/4 between hist[15] and hist[16]
    (32 taps, Kaiser 8; the 16-tap version under-read fast transients by ~0.2 dB)."""
    K = TP_TAPS
    j = np.arange(K)
    w = np.kaiser(K, 8.0)
    taps = []
    for fr in (0.25, 0.5, 0.75):
        h = np.sinc(j - (K // 2 - 1 + fr)) * w
        taps.append(h / h.sum())
    return np.array(taps)


def safe_db(g, cap=HOLD_CAP_DB):
    """Finite, capped gain in dB (silent or broken inputs must never produce inf/NaN gains)."""
    try:
        g = float(g)
    except (TypeError, ValueError):
        return 0.0
    return float(np.clip(g, -cap, cap)) if math.isfinite(g) else 0.0


# ------------------------------------------------------------------ kernels
@njit
def _eq(x, y, coef, on, z, g):
    """One fixed filter chain: 7 biquads (skipped when off) then gain g. DF1 state z (band, ch, 4)."""
    n, nch = x.shape
    nb = coef.shape[0]
    for i in range(n):
        for c in range(nch):
            v = x[i, c]
            for b in range(nb):
                if on[b] == 0:
                    continue
                f = coef[b, 0] * v + coef[b, 1] * z[b, c, 0] + coef[b, 2] * z[b, c, 1] - coef[b, 3] * z[b, c, 2] - coef[b, 4] * z[b, c, 3]
                z[b, c, 1] = z[b, c, 0]; z[b, c, 0] = v
                z[b, c, 3] = z[b, c, 2]; z[b, c, 2] = f
                v = f
            y[i, c] = v * g


@njit
def _xfade(a, b, y, pos, nx):
    """y = a + (b - a) * w, w = raised cosine over nx samples starting at pos (w in [0, 1]: a convex mix)."""
    n, nch = a.shape
    for i in range(n):
        k = pos + i
        w = 1.0 if k >= nx else 0.5 - 0.5 * np.cos(np.pi * k / nx)
        for c in range(nch):
            y[i, c] = a[i, c] + (b[i, c] - a[i, c]) * w


@njit
def _lim(x, y, n0, hist, fir, ring, dq_v, dq_i, dq_ht, avg, lim, L, D):
    """True-peak guard: 4x interpolated peak, min over the L-sample lookahead, moving average over L
    (attack), one-pole release. Output delayed by D = L + K/2 - 1 samples (K = interpolator taps).
    lim = [ceiling lin, release coef, r, avg sum, avg pos]."""
    n, nch = x.shape
    R = ring.shape[0]
    K = hist.shape[1]
    mid = K // 2 - 1
    for i in range(n):
        gi = n0 + i
        pk = 0.0
        for c in range(nch):
            v = x[i, c]
            for k in range(K - 1):
                hist[c, k] = hist[c, k + 1]
            hist[c, K - 1] = v
            ring[gi % R, c] = v
            p = abs(hist[c, mid])
            for r in range(3):
                s = 0.0
                for k in range(K):
                    s += fir[r, k] * hist[c, k]
                if abs(s) > p:
                    p = abs(s)
            if p > pk:
                pk = p
        g = 1.0
        if pk > lim[0]:
            g = lim[0] / pk
        h, t = dq_ht[0], dq_ht[1]
        while t > h and dq_v[(t - 1) % (L + 1)] >= g:
            t -= 1
        dq_v[t % (L + 1)] = g; dq_i[t % (L + 1)] = gi; t += 1
        while dq_i[h % (L + 1)] <= gi - L:
            h += 1
        dq_ht[0], dq_ht[1] = h, t
        mn = dq_v[h % (L + 1)]
        pos = int(lim[4])
        lim[3] += mn - avg[pos]
        avg[pos] = mn
        lim[4] = (pos + 1) % L
        a = lim[3] / L
        if a < lim[2]:
            lim[2] = a
        else:
            lim[2] += (a - lim[2]) * lim[1]
        src = gi - D
        for c in range(nch):
            y[i, c] = ring[src % R, c] * lim[2] if src >= 0 else 0.0


class Chain:
    """A filter chain frozen at one state + gain."""

    def __init__(self, st, gain_db, sr, nch):
        self.st = st
        self.gain_db = gain_db
        self.coef = np.zeros((NBANDS, 5))
        self.on = np.zeros(NBANDS, np.int64)
        for i, b in enumerate(st["bands"]):
            _coefs(TYPES.index(b["type"]), float(b["freq"]), float(b["gain"]), float(b["q"]), float(sr), self.coef[i])
            self.on[i] = 1 if (b["on"] and not st["bypass"]) else 0
        self.g = 1.0 if st["bypass"] else 10 ** (gain_db / 20)
        self.z = np.zeros((NBANDS, nch, 4))
        self.tau = 0.0                       # slowest pole decay time constant (s) of the enabled bands
        for i in range(NBANDS):
            if self.on[i]:
                r = float(np.max(np.abs(np.roots([1.0, self.coef[i, 3], self.coef[i, 4]]))))
                if 0.0 < r < 1.0:
                    self.tau = max(self.tau, -1.0 / (sr * np.log(r)))
                elif r >= 1.0:
                    self.tau = max(self.tau, 10.0)

    def filters_equal(self, other):
        """Same filters (coefficients + on flags), possibly a different gain."""
        return bool(np.array_equal(self.on, other.on) and np.array_equal(self.coef, other.coef))

    def run(self, x):
        y = np.empty_like(x)
        _eq(x, y, self.coef, self.on, self.z, self.g)
        return y

    def same(self, st, gain_db):
        return st == self.st and abs(gain_db - self.gain_db) < 1e-12


class Processor:
    """Streaming post-master processor. set_state()/set_gain_db() from the GUI thread, process()
    from the audio callback; both under self.lock."""

    def __init__(self, sr, nch, ceiling_dbtp=-1.0):
        import threading
        self.sr, self.nch = int(sr), int(nch)
        self.lock = threading.Lock()
        self.ceiling_dbtp = float(ceiling_dbtp)
        self.L = max(8, int(round(LOOKAHEAD_MS * 1e-3 * self.sr)))
        self.D = self.L + TP_TAPS // 2 - 1   # output delay in samples (lookahead + interpolator)
        self.NX = max(16, int(round(XFADE_MS * 1e-3 * self.sr)))
        self.W = max(256, int(round(WARM_MS * 1e-3 * self.sr)))
        self.WMAX = max(self.W, int(round(WARM_MAX_MS * 1e-3 * self.sr)))   # history kept for warm-up / prime
        self.NG = max(16, int(round(GAIN_RAMP_MS * 1e-3 * self.sr)))
        self.fir = _tp_fir()
        self.state = default_state()
        self.gain_db = 0.0           # hold gain (the output fader is in the state)
        self.transitions = 0
        self.reset()

    def _total_gain(self, st, hold_db):
        return safe_db(hold_db) + safe_db(st.get("out_gain_db", 0.0), OUT_RANGE)

    def reset(self):
        """Fresh start (Play, seek, export): the chain is built at the current state, no transition."""
        nch = self.nch
        self.chain = Chain(self.state, self._total_gain(self.state, self.gain_db), self.sr, nch)
        self.next = None             # chain being faded in
        self.pending = None          # (state, gain) waiting for the current fade to finish
        self.xpos = 0
        self.nx = self.NX                # length of the fade in progress (adaptive)
        self.ramp = None                 # gain-only ramp in progress: [db0, db1, pos]
        self._hbuf = np.zeros((2 * self.WMAX, nch)); self._hn = 0
        self._set_lim(self._lim_state())
        self.gx = None                   # gain-only transition in progress (primed parallel guard)
        self.n = 0

    def _lim_state(self):
        nch = self.nch
        R = 1
        while R < self.D + 2:
            R *= 2
        return [np.zeros((nch, TP_TAPS)), np.zeros((R, nch)), np.ones(self.L + 1),
                np.full(self.L + 1, -(1 << 40), np.int64), np.zeros(2, np.int64), np.ones(self.L),
                np.array([10 ** ((self.ceiling_dbtp - TP_MARGIN_DB) / 20), 1 - np.exp(-1 / (RELEASE_MS * 1e-3 * self.sr)),
                          1.0, float(self.L), 0.0])]

    def _set_lim(self, S):
        self.hist, self.ring, self.dq_v, self.dq_i, self.dq_ht, self.avg, self.lim = S

    def _run_lim(self, x, n0, S):
        y = np.empty_like(x)
        hist, ring, dq_v, dq_i, dq_ht, avg, lim = S
        _lim(x, y, n0, hist, self.fir, ring, dq_v, dq_i, dq_ht, avg, lim, self.L, self.D)
        return y

    def set_state(self, st, gain_db=None, snap=False):
        """snap=True: switch at once (only when nothing is playing). Otherwise crossfade."""
        with self.lock:
            self.state = clean_state(st)
            if gain_db is not None:
                self.gain_db = safe_db(gain_db)
            if snap:
                self.chain = Chain(self.state, self._total_gain(self.state, self.gain_db), self.sr, self.nch)
                self.next = None; self.pending = None
            else:
                self.pending = (self.state, self._total_gain(self.state, self.gain_db))

    def set_gain_db(self, g, snap=False):
        self.set_state(self.state, g, snap=snap)

    def _start_fade(self):
        st, g = self.pending
        self.pending = None
        target = self.next or self.chain
        if target.same(st, g):
            return
        ch = Chain(st, g, self.sr, self.nch)
        cur = self.chain
        if self.next is None and not st["bypass"] and not cur.st["bypass"] and ch.filters_equal(cur):
            # gain-only change (OUT fader / hold gain): same filters, so keep their state and ramp the
            # gain in the dB domain from where it is now (no crossfade of two loud signals)
            # The output guard is primed too: a second guard runs the recent history at the new gain (so
            # it is in its steady state, gain reduction included), and the two guard outputs are mixed
            # with a weight that moves the level linearly in dB over GAIN_RAMP_MS. A convex mix of two
            # steady outputs never exceeds the louder one (a plain ramp into the guard overshoots because
            # the guard's release memory lags the new gain).
            H = min(len(self.hist_in), int(0.4 * self.sr))
            self.transitions += 1
            if self.ramp is None and H >= 4 * (self.D + self.L):
                ch.z[:] = cur.z                                   # same filters, same input: exact state
                warm = self.warm_len_for(ch)
                tmp = Chain(st, g, self.sr, self.nch)
                hh = self.hist_in[-(warm + H):]
                yh = tmp.run(hh)[-H:]
                S = self._lim_state()
                self._run_lim(np.ascontiguousarray(yh), self.n - H, S)
                self.gx = {"chain": ch, "lim": S, "pos": 0, "k": 10 ** ((g - cur.gain_db) / 20)}
                return
            db0 = self._ramp_db() if self.ramp is not None else cur.gain_db
            cur.st, cur.gain_db, cur.g = st, g, 10 ** (g / 20)
            self.ramp = [db0, g, 0]
            return
        tau = max(ch.tau, cur.tau)
        warm = int(min(self.WMAX, max(self.W, WARM_TAU * tau * self.sr)))
        h = self.hist_in
        if len(h):
            ch.run(h[-warm:])                # warm: the new chain starts in its steady state
        if self.next is not None:            # never happens (fades are queued), kept for safety
            self.chain = self.next
        self.next = ch
        self.xpos = 0
        self.nx = int(min(XFADE_MAX_MS * 1e-3 * self.sr, max(self.NX, XFADE_TAU * tau * self.sr)))
        self.transitions += 1

    @property
    def hist_in(self):
        return self._hbuf[max(0, self._hn - self.WMAX):self._hn]

    def _push_hist(self, x):
        n = len(x)
        if n >= self.WMAX:
            self._hbuf[:self.WMAX] = x[-self.WMAX:]; self._hn = self.WMAX; return
        if self._hn + n > len(self._hbuf):
            keep = self.WMAX - n
            self._hbuf[:keep] = self._hbuf[self._hn - keep:self._hn]; self._hn = keep
        self._hbuf[self._hn:self._hn + n] = x; self._hn += n

    def _ramp_db(self, k=0):
        db0, db1, pos = self.ramp
        w = min(1.0, (pos + k) / self.NG)
        return db0 + (db1 - db0) * (0.5 - 0.5 * np.cos(np.pi * w))

    def warm_len_for(self, ch):
        return int(min(self.WMAX, max(self.W, WARM_TAU * ch.tau * self.sr)))

    def warm_len(self):
        """Samples of history prime() needs for the current chain (scales with its slowest pole)."""
        return self.warm_len_for(self.chain)

    def process(self, x):
        x = np.nan_to_num(np.ascontiguousarray(x, dtype=np.float64), nan=0.0, posinf=0.0, neginf=0.0)
        with self.lock:
            if self.next is None and self.gx is None and self.pending is not None:
                self._start_fade()
            a = self.chain.run(x)
            if self.ramp is not None:
                db0, db1, pos = self.ramp
                k = np.minimum(1.0, (pos + np.arange(len(x))) / self.NG)
                a *= (10 ** ((db0 + (db1 - db0) * (0.5 - 0.5 * np.cos(np.pi * k)) - db1) / 20))[:, None]
                self.ramp[2] += len(x)
                if self.ramp[2] >= self.NG:
                    self.ramp = None
            if self.next is not None:
                b = self.next.run(x)
                pre = np.empty_like(a)
                _xfade(a, b, pre, self.xpos, self.nx)
                self.xpos += len(x)
                if self.xpos >= self.nx:
                    self.chain, self.next = self.next, None
                    self.ramp = None
            else:
                pre = a
            np.nan_to_num(pre, copy=False, nan=0.0, posinf=0.0, neginf=0.0)
            y = np.empty_like(pre)
            _lim(pre, y, self.n, self.hist, self.fir, self.ring, self.dq_v, self.dq_i, self.dq_ht,
                 self.avg, self.lim, self.L, self.D)
            if self.gx is not None:
                gx = self.gx
                yb = self._run_lim(gx["chain"].run(x), self.n, gx["lim"])
                s = np.minimum(1.0, (gx["pos"] + np.arange(len(x)) + 1) / self.NG)
                s = 0.5 - 0.5 * np.cos(np.pi * s)              # raised-cosine progress ...
                k = gx["k"]
                w = s if abs(k - 1.0) < 1e-9 else (k ** s - 1.0) / (k - 1.0)   # ... linear in dB
                y += (yb - y) * w[:, None]
                gx["pos"] += len(x)
                if gx["pos"] >= self.NG:
                    self.chain = gx["chain"]; self._set_lim(gx["lim"]); self.gx = None
            self.n += len(x)
            self._push_hist(x)
        return y

    def prime(self, history):
        """After reset(): run the audio just before the play/seek position through the chain and the
        guard (output discarded), so playback starts in the steady state instead of from cold filters."""
        h = LN.as2d(np.asarray(history, dtype=np.float64))[-self.warm_len():]
        if len(h):
            self.process(h)

    def busy(self):
        return self.next is not None or self.pending is not None or self.ramp is not None or self.gx is not None

    def gain_reduction_db(self):
        return -20 * np.log10(max(self.lim[2], 1e-9))


# ------------------------------------------------------------------ loudness hold
class LoudnessModel:
    """Predicts how much an EQ curve changes integrated loudness, from the song's K-weighted
    spectrum of the gated (loud) part. Used for instant loudness hold while editing; the exact
    hold gain comes from solve_gain() (offline renders) once you stop moving controls."""

    def __init__(self, x, sr):
        self.sr = sr
        x = LN.as2d(x)
        k = LN.kweight(x, sr)
        blk = int(0.4 * sr)
        nb = max(1, len(k) // blk)
        e = np.array([np.mean(np.sum(k[i * blk:(i + 1) * blk] ** 2, 1)) for i in range(nb)])
        l = -0.691 + 10 * np.log10(np.maximum(e, 1e-20))
        loud = l > -70
        self.silent = not loud.any()
        thr = (-0.691 + 10 * np.log10(max(e[loud].mean(), 1e-20)) - 10) if loud.any() else 0.0
        keep = loud & (l > thr)
        seg = np.concatenate([k[i * blk:(i + 1) * blk] for i in np.nonzero(keep)[0]]) if keep.any() else k
        f, p = signal.welch(seg, sr, nperseg=min(8192, max(256, len(seg))), axis=0)
        self.f, self.p = f[1:], p[1:].sum(axis=1)

    def delta_db(self, st):
        tot = float(np.sum(self.p))
        if self.silent or not math.isfinite(tot) or tot <= 1e-30:
            return 0.0
        h = response_db(st, self.f, self.sr)
        return safe_db(10 * np.log10(max(np.sum(self.p * 10 ** (h / 10)), 1e-30) / tot))


def render(x, sr, st, gain_db, ceiling_dbtp, block=4096):
    """Offline render, exactly what the preview plays for a fixed state (delay compensated).
    gain_db is the hold gain; the state's output fader is added."""
    x = LN.as2d(np.asarray(x, dtype=np.float64))
    pr = Processor(sr, x.shape[1], ceiling_dbtp)
    pr.set_state(st, gain_db, snap=True)
    pr.reset()
    xp = np.vstack([x, np.zeros((pr.D, x.shape[1]))])
    out = np.empty_like(xp)
    for i in range(0, len(xp), block):
        out[i:i + block] = pr.process(xp[i:i + block])
    return out[pr.D:]


def _lufs(y, sr):
    v = LN.integrated_lufs(y, sr)
    return float(v) if math.isfinite(v) else -70.0


def solve_gain_ex(x, sr, st, hold_lufs, ceiling_dbtp, model=None, tol=0.05, iters=8):
    """Hold gain (dB) so the rendered output (with the output fader at 0) measures hold_lufs.
    Spectral estimate first, then secant steps on real renders. Returns a dict:
    gain_db, lufs (measured, fader at 0), converged, iterations, note."""
    st = clean_state(st)
    st0 = dict(st, out_gain_db=0.0)
    res = {"gain_db": 0.0, "lufs": None, "converged": True, "iterations": 0, "note": None}
    if not st["hold_loudness"] or st["bypass"]:
        res["lufs"] = _lufs(render(x, sr, st0, 0.0, ceiling_dbtp), sr)
        return res
    hold = float(hold_lufs) if hold_lufs is not None and math.isfinite(float(hold_lufs)) else -70.0
    model = model or LoudnessModel(x, sr)
    if model.silent or hold <= -69.5:
        res.update(lufs=-70.0, note="The master is silent: loudness hold is off (gain 0 dB).")
        return res
    g = safe_db(-model.delta_db(st))
    prev = None
    lm = None
    for k in range(1, iters + 1):
        lm = _lufs(render(x, sr, st0, g, ceiling_dbtp), sr)
        res["iterations"] = k
        err = hold - lm
        if abs(err) <= tol:
            break
        if prev is not None and abs(lm - prev[1]) > 1e-6:
            slope = (lm - prev[1]) / (g - prev[0]) if abs(g - prev[0]) > 1e-9 else 1.0
            slope = float(np.clip(slope, 0.2, 1.5))
        else:
            slope = 0.87 if err > 0 else 1.0     # the guard eats part of a boost
        prev = (g, lm)
        g = safe_db(g + err / slope)
    res.update(gain_db=round(float(g), 4), lufs=lm)
    if lm is None or abs(hold - lm) > tol + 0.05:
        res["converged"] = False
        res["note"] = (f"Loudness hold did not fully converge: {lm:.2f} LUFS vs {hold:.2f} after {res['iterations']} renders "
                       f"(the true-peak guard limits heavy boosts).")
    return res


def solve_gain(x, sr, st, hold_lufs, ceiling_dbtp, model=None, tol=0.05, iters=8):
    r = solve_gain_ex(x, sr, st, hold_lufs, ceiling_dbtp, model, tol, iters)
    return r["gain_db"], r["lufs"]


def apply(x, sr, st, hold_lufs, ceiling_dbtp, gain_db=None):
    """Offline manual-EQ stage for export/CLI. Returns (y, info)."""
    st = clean_state(st)
    sol = None
    if gain_db is None:
        sol = solve_gain_ex(x, sr, st, hold_lufs, ceiling_dbtp)
        gain_db = sol["gain_db"]
    gain_db = safe_db(gain_db)
    y = render(x, sr, st, gain_db, ceiling_dbtp)
    tp = LN.true_peak(y, sr, 4)
    info = {"state": st, "hold_gain_db": round(float(gain_db), 3), "out_gain_db": st["out_gain_db"],
            "hold_lufs": round(float(hold_lufs), 2) if hold_lufs is not None and math.isfinite(float(hold_lufs)) else None,
            "lufs": round(_lufs(y, sr), 2),
            "true_peak_dbtp": round(20 * np.log10(tp), 2) if tp > 0 else None,
            "converged": sol["converged"] if sol else True, "notes": [sol["note"]] if sol and sol["note"] else []}
    return y, info