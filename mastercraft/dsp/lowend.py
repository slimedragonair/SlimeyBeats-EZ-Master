"""Low-end punch: kick-band transient shaping and kick-triggered sub ducking.

Nothing else in the chain *adds* low-end attack. The compressors, the mud cut, the
de-esser and the limiter can only take punch away, so on a mid-forward Suno mix the
low end ends up softer than it went in: woolly sub, no "slap". These two moves put the
attack back without an EQ tilt (an EQ boost would lift the sustain with it):

  transient_shape()   boost the first few ms of the kick band, shorten its sustain
  duck_sub_on_hits()  dip 20-60 Hz for the length of a hit, so the kick owns that
                      moment and the sustained bass gets out of its way

Both work on a zero-phase band extraction and return `x + (g - 1) * band`, so the stage
is an exact passthrough wherever the gain is 1 (no phase rotation, no level drift).
"""
import numpy as np

from .dynamics import HOP, block_level_db, coef, expand_gain, rms_window_db, _smooth_gr
from .filters import band_zero_phase


def _blocks(n, hop=HOP):
    return int(np.ceil(n / hop))


def _mask_attack_sustain(band, sr, split_db=12.0, smooth_ms=2.0):
    """0..1 masks (block rate) for the attack and the sustain of a low band.

    A fast follower (0.5 ms up / 20 ms down) minus a slow one (30 ms up / 150 ms down) is the
    classic transient detector: the slow follower is still climbing when the hit lands, so the
    difference is large on an attack and near zero in a steady bass bed. Centred moving-average
    windows do NOT work here: a 40 ms centred RMS already contains the peak, so the difference
    collapses to ~3 dB and the shaper does almost nothing."""
    lv = rms_window_db(block_level_db(band), 1.5, sr)
    fast = _smooth_gr(lv.astype(np.float64), coef(0.5, sr), coef(20.0, sr))
    slow = _smooth_gr(lv.astype(np.float64), coef(30.0, sr), coef(150.0, sr))
    t = np.clip((fast - slow) / float(split_db), 0.0, 1.0)
    # soft gate: fully open ~10 dB above the quiet floor of this band, closed in silence only
    # (an earlier version scaled the whole mask against the *bed* level, which halved every move)
    floor = float(np.percentile(fast, 5)) - 6.0
    act = np.clip((fast - floor) / 10.0, 0.0, 1.0)
    k = max(1, int(smooth_ms * 1e-3 * sr / HOP))
    if k > 1:
        ker = np.ones(k) / k
        t = np.convolve(t, ker, mode="same")
        act = np.convolve(act, ker, mode="same")
    t = t * act
    return t, np.clip((1.0 - t) * act, 0.0, 1.0)


def transient_shape(x, sr, lo=50.0, hi=160.0, attack_db=2.0, sustain_db=1.0,
                    attack_ms=4.0, release_ms=70.0):
    """Transient shaper on one low band (kick body by default).

    attack_db  : gain on the attack share (where the band jumps above its own sustain)
    sustain_db : gain reduction on the remaining share (the tail after the hit)
    attack_ms / release_ms are accepted for compatibility but the shape now comes from the
    detector masks themselves."""
    if attack_db <= 0.01 and sustain_db <= 0.01:
        return x, {}
    band = band_zero_phase(x, lo, hi, sr)
    t, s = _mask_attack_sustain(band, sr)
    g_db = float(attack_db) * t - float(sustain_db) * s
    g = expand_gain(-g_db.astype(np.float64), x.shape[0])   # expand_gain works in positive GR
    d = (g - 1.0)
    y = x + (d[:, None] if x.ndim == 2 else d) * band
    return y, {"attack_applied_db": round(float(g_db.max()), 2), "sustain_applied_db": round(float(-g_db.min()), 2),
               "attack_share": round(float(np.mean(t)), 3), "kick_band_hz": [lo, hi]}


def duck_sub_on_hits(x, sr, hits, lo=22.0, hi=60.0, depth_db=2.0, attack_ms=4.0,
                     release_ms=90.0, hold_ms=60.0):
    """Dip the sub band for the length of each kick hit (the kick bounces the bass out of
    its way). Depth is scaled down when hits arrive faster than ~2/s, so a dense double-kick
    passage is tightened but not pumped."""
    if depth_db <= 0.01 or hits is None or len(hits) == 0:
        return x, {}
    n = x.shape[0]
    nb = _blocks(n)
    gr = np.zeros(nb, np.float64)
    post = int(max(hold_ms, 1.0) * 1e-3 * sr) // HOP
    for t in np.asarray(hits).ravel():
        a = max(0, int(t) // HOP - 1)
        b = min(nb, (int(t) + post) // HOP + 1)
        if b > a:
            gr[a:b] = 1.0
    dur = max(n / float(sr), 1e-6)
    rate = len(hits) / dur
    depth = float(depth_db) * float(min(1.0, max(0.35, 2.0 / max(rate, 1e-6))))
    gr *= depth
    gr = _smooth_gr(gr, coef(attack_ms, sr), coef(release_ms, sr))
    band = band_zero_phase(x, lo, hi, sr)
    g = expand_gain(gr, n)
    d = (g - 1.0)
    y = x + (d[:, None] if x.ndim == 2 else d) * band
    return y, {"sub_duck_db": round(float(gr.max()), 2), "hit_rate_per_s": round(float(rate), 2),
               "duck_band_hz": [lo, hi]}
