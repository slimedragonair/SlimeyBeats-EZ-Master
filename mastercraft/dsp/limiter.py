"""True-peak-aware lookahead limiter (research brief 3.3 #2).

Gain computer runs on the 4x-oversampled peak envelope. The gain curve is built so it is
mathematically guaranteed to be <= the required gain at every sample:
  required g_req = min(1, ceiling / tp_env)
  h  = forward min over lookahead L     (gain is down before the peak arrives)
  r  = block-min + exponential release   (r <= h everywhere)
  g  = causal box average of r over L    (smooth attack ramp, still <= g_req)
Offline processing, so lookahead costs no latency. Output is verified with an
independent 4x true-peak meter and trimmed if needed."""
import numpy as np
from scipy.ndimage import minimum_filter1d

from ..jit import njit
from ..loudness import oversampled_abs_max, true_peak, integrated_lufs


@njit
def _release(hb, a):
    out = np.empty_like(hb)
    y = hb[0]
    for i in range(hb.shape[0]):
        t = hb[i]
        if t < y:
            y = t
        else:
            y = a * y + (1.0 - a) * t
            if y > t:
                y = t
        out[i] = y
    return out


@njit
def _release_dual(hb, a_fast, a_slow, a_slow_att):
    """Two-stage (program-dependent) release on the required-gain curve.
    f: instant attack, fast release (isolated kick/snare peaks recover in ~30 ms).
    s: slow follower (slow attack, slow release) that settles on the *sustained* reduction
       in dense passages, so the gain does not flutter/pump on every hit.
    out = min(f, s) <= f <= required gain, so the ceiling guarantee is kept."""
    out = np.empty_like(hb)
    f = hb[0]
    s = 1.0
    for i in range(hb.shape[0]):
        t = hb[i]
        if t < f:
            f = t
        else:
            f = a_fast * f + (1.0 - a_fast) * t
            if f > t:
                f = t
        if t < s:
            s = a_slow_att * s + (1.0 - a_slow_att) * t
        else:
            s = a_slow * s + (1.0 - a_slow) * t
        out[i] = f if f < s else s
    return out


def _gr_db(g, pct=0.02):
    """Limiter GR 'at any point', ignoring the single most extreme 0.02% of samples
    (isolated spikes) so one stray transient can't veto a clean target."""
    q = float(np.percentile(g, pct))
    return -20 * np.log10(max(q, 1e-9))


class TruePeakLimiter:
    def __init__(self, sr, ceiling_db=-1.0, lookahead_ms=3.0, release_ms=100.0, oversample=4,
                 safety_margin_db=0.2, release_slow_ms=None, slow_attack_ms=80.0):
        self.sr = sr
        self.ceiling_db = ceiling_db
        self.L = max(3, int(round(lookahead_ms * 1e-3 * sr)) | 1)
        self.release_ms = release_ms
        self.release_slow_ms = release_slow_ms
        self.slow_attack_ms = slow_attack_ms
        self.os = int(oversample)
        self.margin = safety_margin_db
        self._env = None

    def prepare(self, x):
        """Oversampled peak envelope of the pre-gain signal (scales linearly with gain)."""
        self._env = oversampled_abs_max(x, self.os)
        return self

    def gain_curve(self, pre_gain_lin):
        n = self._env.size
        c = 10 ** ((self.ceiling_db - self.margin) / 20)
        env = self._env * pre_gain_lin
        greq = np.minimum(1.0, c / np.maximum(env, 1e-12))
        L = self.L
        hc = minimum_filter1d(greq, size=L, mode="nearest")
        half = L // 2
        h = np.concatenate([hc[half:], np.full(half, hc[-1])])  # h[s] = min greq[s .. s+L-1]
        h = np.minimum(h, greq)
        B = min(16, L)
        nb = int(np.ceil(n / B))
        hp = np.concatenate([h, np.ones(nb * B - n)])
        hb = hp.reshape(nb, B).min(axis=1)
        a = float(np.exp(-B / (self.release_ms * 1e-3 * self.sr)))
        if self.release_slow_ms:
            a_s = float(np.exp(-B / (self.release_slow_ms * 1e-3 * self.sr)))
            a_sa = float(np.exp(-B / (self.slow_attack_ms * 1e-3 * self.sr)))
            r = _release_dual(hb.astype(np.float64), a, a_s, a_sa)
        else:
            r = _release(hb.astype(np.float64), a)
        rs = np.repeat(r, B)[:n]
        cs = np.concatenate([np.full(L, rs[0]), rs]).cumsum()
        cs = np.concatenate([[0.0], cs])
        g = (cs[L:L + n + 1][1:] - cs[1:n + 1]) / L
        # g[t] = mean(rs[t-L+1 .. t]) (with rs[<0] = rs[0])
        return np.minimum(g, 1.0)

    def process(self, x, pre_gain_db=0.0):
        if self._env is None:
            self.prepare(x)
        pg = 10 ** (pre_gain_db / 20)
        g = self.gain_curve(pg)
        y = x * (pg * g)[:, None]
        return y, g


def limit_to_target(x, sr, target_lufs, ceiling_db, lookahead_ms=3.0, release_ms=100.0,
                    oversample=4, safety_margin_db=0.2, iterations=6, tolerance_lu=0.2,
                    max_gr_abort_db=6.0, progress=None, release_slow_ms=None, clip_db=0.0):
    """Iterate pre-limiter gain until integrated loudness hits target (±tolerance).
    clip_db > 0: a gentle 4x-oversampled soft clipper rounds the top clip_db of the peaks
    (at the level the limiter will see) before limiting, so the limiter does less gain riding."""
    cinfo = {}
    if clip_db > 0.05:
        from .tone import soft_clip
        G0 = float(np.clip(target_lufs - integrated_lufs(x, sr), -40.0, 30.0))
        gl = 10 ** (G0 / 20)
        c = 10 ** ((ceiling_db - safety_margin_db) / 20)
        # the clipper only acts on what would exceed the ceiling (+ its knee): it takes the place
        # of the first clip_db of limiting on the shortest peaks
        z, cinfo = soft_clip(x * gl, sr, clip_db, c * 10 ** (clip_db / 2 / 20), oversample)
        x = z / gl
    lim = TruePeakLimiter(sr, ceiling_db, lookahead_ms, release_ms, oversample, safety_margin_db,
                          release_slow_ms).prepare(x)
    l0 = integrated_lufs(x, sr)
    G = float(np.clip(target_lufs - l0, -40.0, 30.0))  # never blast near-silence by +50 dB
    info = {"iterations": 0, "reachable": True}
    best = None
    for it in range(max(1, iterations)):
        y, g = lim.process(x, G)
        lo = integrated_lufs(y, sr)
        gr = _gr_db(g)
        info.update(iterations=it + 1, out_lufs=lo, pre_gain_db=G, max_gr_db=gr)
        best = (y, g, G, lo, gr)
        if gr > max_gr_abort_db + 1e-6:
            break
        err = target_lufs - lo
        if abs(err) <= tolerance_lu * 0.5:
            break
        G += err * (1.15 if err > 0 else 1.0)
        if progress:
            progress(it)
    y, g, G, lo, gr = best
    if gr > max_gr_abort_db + 1e-6:
        # back off until the limiter works within its clean range; bisection on pre-gain
        hi_g, lo_g = G, G - 12.0
        for _ in range(10):
            mid = 0.5 * (hi_g + lo_g)
            gm = lim.gain_curve(10 ** (mid / 20))
            if _gr_db(gm) > max_gr_abort_db:
                hi_g = mid
            else:
                lo_g = mid
        G = lo_g
        y, g = lim.process(x, G)
        lo = integrated_lufs(y, sr)
        gr = _gr_db(g)
        info.update(reachable=False, out_lufs=lo, pre_gain_db=G, max_gr_db=gr)
    # independent true-peak verification and trim
    tp = true_peak(y, sr, 4)
    tp_db = 20 * np.log10(max(tp, 1e-12))
    trim = 0.0
    if tp_db > ceiling_db - 0.05:
        trim = (ceiling_db - 0.05) - tp_db
        y = y * 10 ** (trim / 20)
        tp_db += trim
        lo += trim
    info.update(**cinfo)
    if release_slow_ms:
        info["release"] = f"dual {release_ms:.0f}/{release_slow_ms:.0f} ms"
    info.update(abs_max_gr_db=-20 * np.log10(max(float(g.min()), 1e-9)), out_tp_dbtp=tp_db, trim_db=trim, out_lufs=lo,
                gr_pct_over_1db=round(100.0 * float(np.mean(g < 10 ** (-1 / 20))), 2))
    return y, info
