"""Transient / punch measurement around detected kick and snare hits.

detect_hits(): onset detection on a kick band (40-120 Hz) and a snare band (snare body
180-350 Hz + crack 1.5-5 kHz) of the *input*.  hit_metrics(): for each hit and band,
  tsr_db    transient-to-sustain ratio  = peak(|band|, 0..15 ms) / rms(band, 30..120 ms)
  crest_db  per-hit peak-to-RMS         = peak(|band|, 0..15 ms) / rms(band, -5..80 ms)
  attack_ms time for the 1 ms RMS envelope to rise from (peak-12 dB) to (peak-1 dB)
compare() measures the same hit times in a processed signal (ratios are level-independent)
and summarises the punch loss overall and per song section, with timestamps."""
import numpy as np
from scipy.ndimage import uniform_filter1d, minimum_filter1d
from scipy.signal import butter, sosfiltfilt, find_peaks

BANDS = {"kick": [(40.0, 120.0)], "snare": [(180.0, 350.0), (1500.0, 5000.0)],
         "crack": [(5000.0, 12000.0)], "full": [(25.0, 18000.0)]}
DETECT = ("kick", "snare")                     # bands onsets are detected on
# measured view -> hit set it is measured at. "crack" = snare/kick top end (5-12 kHz), where
# de-essers and fizz/harsh dynamic EQ act; "full" = broadband, where saturation/limiting act.
MEASURE = {"kick": "kick", "snare": "snare", "crack": "snare", "full": "all"}
PRE_MS, PK_MS, SUS0_MS, SUS1_MS = 2.0, 15.0, 30.0, 120.0


def _mono(x):
    x = np.asarray(x, np.float64)
    return x.mean(axis=1) if x.ndim == 2 else x


def valid_bands(kind, sr):
    """The kind's sub-bands clamped below Nyquist; bands that collapse (lo >= 0.8 * clamped hi,
    e.g. crack 5-12 kHz at 11.025 kHz) are dropped instead of crashing the filter design (QA H2)."""
    out = []
    for lo, hi in BANDS[kind]:
        hi = min(hi, 0.45 * sr)
        if lo < 0.8 * hi:
            out.append((lo, hi))
    return out


_MIN_LEN = 64  # sosfiltfilt needs more samples than its pad length


def band_signal(x, sr, kind):
    """Band-passed mono signal, or None when no sub-band fits the sample rate / signal is too short."""
    m = _mono(x)
    bands = valid_bands(kind, sr)
    if not bands or len(m) < _MIN_LEN:
        return None
    out = np.zeros_like(m)
    for lo, hi in bands:
        sos = butter(2, [lo, hi], btype="bandpass", fs=sr, output="sos")
        out += sosfiltfilt(sos, m)
    return out


def _env_db(b, sr, ms=1.0):
    n = max(1, int(sr * ms / 1000))
    return 10 * np.log10(np.maximum(uniform_filter1d(b * b, n, mode="nearest"), 0.0) + 1e-20)


def detect_hits(x, sr, max_hits=1500):
    """Returns dict kind -> array of onset sample indices (only the loud, clear hits)."""
    hits = {}
    dec = max(1, int(sr / 1000))  # work on a 1 kHz envelope grid
    for kind in DETECT:
        b = band_signal(x, sr, kind)
        if b is None or len(b) < int(0.5 * sr):
            hits[kind] = np.zeros(0, np.int64)  # band unavailable at this rate / file too short to judge
            continue
        e = _env_db(b, sr, 4.0)[::dec]
        rise = e - minimum_filter1d(e, 25, origin=12)       # rise over the previous ~25 ms
        loud = np.percentile(e, 97)
        ok = (rise > 7.0) & (e > loud - 18.0)
        cand, _ = find_peaks(np.where(ok, rise, 0.0), distance=90)
        if len(cand) > max_hits:
            cand = cand[np.argsort(rise[cand])[-max_hits:]]
            cand.sort()
        # refine: onset = start of the rise (where envelope is 6 dB below the post-onset peak)
        on = []
        for c in cand:
            seg = e[max(c - 25, 0): c + 15]
            pk = seg.max()
            k = np.argmax(seg >= pk - 6.0)
            on.append((max(c - 25, 0) + k) * dec)
        hits[kind] = np.array(sorted(set(on)), dtype=np.int64)
    return hits


def hits_for(view, hits, sr=48000):
    src = MEASURE.get(view, view)
    if src != "all":
        return hits.get(src, np.zeros(0, np.int64))
    u = np.unique(np.concatenate([hits[k] for k in DETECT if k in hits] or [np.zeros(0, np.int64)]))
    if len(u) < 2:
        return u
    keep = np.concatenate([[True], np.diff(u) > int(0.03 * sr)])  # merge kick+snare onsets < 30 ms apart
    return u[keep]


def hit_metrics(x, sr, hits):
    res = {}
    ms = sr / 1000.0
    for kind in MEASURE:
        idx = hits_for(kind, hits, sr)
        b = band_signal(x, sr, kind)
        if b is None:
            res[kind] = np.zeros((0, 4), np.float64)
            continue
        e1 = _env_db(b, sr, 1.0)
        n = len(b)
        rows = []
        for k_i, t in enumerate(idx):
            a0, a1 = int(t - PRE_MS * ms), int(t + PK_MS * ms)
            # the sustain window must end before the next hit, otherwise a double-kick or a
            # 16th-note snare run puts the NEXT attack inside the "tail" and the ratio stops
            # reacting to anything (at 6+ hits/s the 30-120 ms window always overlapped)
            nxt = float(idx[k_i + 1] - t) / sr * 1000.0 if k_i + 1 < len(idx) else 1e9
            sus_end = SUS1_MS if nxt > SUS1_MS * 1.6 else max(SUS0_MS + 8.0, min(SUS1_MS, 0.7 * nxt))
            s0, s1 = int(t + SUS0_MS * ms), int(t + sus_end * ms)
            c0, c1 = int(t - 5 * ms), int(t + 80 * ms)
            if t - 10 * ms < 0 or s1 >= n:
                rows.append((np.nan, np.nan, np.nan, np.nan))
                continue
            pk = np.max(np.abs(b[a0:a1])) + 1e-12
            sus = np.sqrt(np.mean(b[s0:s1] ** 2)) + 1e-12
            rms = np.sqrt(np.mean(b[c0:c1] ** 2)) + 1e-12
            seg = e1[int(t - 10 * ms): a1 + int(10 * ms)]
            p = int(np.argmax(seg)); top = seg[p]
            pre = seg[:p + 1]
            hi_i = int(np.argmax(pre >= top - 1.0))
            lo_i = np.where(pre[:hi_i + 1] <= top - 12.0)[0]
            att = (hi_i - lo_i[-1]) / ms if len(lo_i) else np.nan
            rows.append((20 * np.log10(pk / sus), 20 * np.log10(pk / rms), att, 20 * np.log10(pk)))
        res[kind] = np.array(rows, np.float64).reshape(-1, 4)
    return res


def _sections(times_s, loss, thr=1.5, gap_s=6.0, min_hits=3):
    """Group hits whose TSR loss exceeds thr into time ranges."""
    bad = np.where(loss > thr)[0]
    out = []
    if not len(bad):
        return out
    start = prev = bad[0]; members = [bad[0]]
    for i in bad[1:]:
        if times_s[i] - times_s[prev] <= gap_s:
            members.append(i); prev = i
            continue
        out.append(members); members = [i]; prev = i
    out.append(members)
    secs = []
    for mem in out:
        if len(mem) < min_hits:
            continue
        secs.append({"start_s": round(float(times_s[mem[0]]), 2), "end_s": round(float(times_s[mem[-1]] + 0.12), 2),
                     "hits": len(mem), "mean_tsr_loss_db": round(float(np.nanmean(loss[mem])), 2),
                     "max_tsr_loss_db": round(float(np.nanmax(loss[mem])), 2)})
    return secs


def _nmed(v):
    """nanmedian rounded to 0.01, or None when every value is NaN (no RuntimeWarning; QA N6)."""
    v = np.asarray(v, float)
    v = v[np.isfinite(v)]
    return round(float(np.median(v)), 2) if v.size else None


def summary(m):
    """Absolute punch numbers of one signal (from hit_metrics)."""
    out = {}
    for kind, a in m.items():
        a = a[np.isfinite(a[:, 0])] if len(a) else a  # edge hits are NaN rows (QA L4)
        if not len(a):
            out[kind] = {"hits": 0}
            continue
        out[kind] = {"hits": int(len(a)), "tsr_db": _nmed(a[:, 0]),
                     "hit_crest_db": _nmed(a[:, 1]), "attack_ms": _nmed(a[:, 2])}
    return out


def compare(m_in, m_out, hits, sr, thr=1.5):
    """Punch change in -> out at the same hit positions. Positive loss = less punch."""
    rep = {}
    for kind in m_in:
        a, b = m_in[kind], m_out.get(kind, np.zeros((0, 4)))
        if not len(a) or len(b) != len(a):
            rep[kind] = {"hits": 0}
            continue
        ok = np.isfinite(a[:, 0]) & np.isfinite(b[:, 0])  # drop edge (NaN) rows so nan-reductions never see all-NaN (QA L4)
        t_all = hits_for(kind, hits, sr) / sr
        a, b = a[ok], b[ok]
        if not len(a):
            rep[kind] = {"hits": 0}
            continue
        loss = a[:, 0] - b[:, 0]
        closs = a[:, 1] - b[:, 1]
        datt = b[:, 2] - a[:, 2]
        dlev = b[:, 3] - a[:, 3]
        dlev = dlev - np.nanmedian(dlev)  # hit-to-hit inconsistency ("splotchy"), level-independent
        t = t_all[ok] if len(t_all) == len(ok) else np.arange(len(a), dtype=float)
        rep[kind] = {"hits": int(len(a)),
                     "tsr_loss_db_median": round(float(np.nanmedian(loss)), 2),
                     "tsr_loss_db_p90": round(float(np.nanpercentile(loss, 90)), 2),
                     "crest_loss_db_median": round(float(np.nanmedian(closs)), 2),
                     "attack_change_ms_median": round(float(np.nanmedian(datt)), 2) if np.isfinite(datt).any() else None,
                     "frac_hits_smeared": round(float(np.nanmean(loss > thr)), 3),
                     "hit_level_spread_db": round(float(np.nanpercentile(np.abs(dlev), 90)), 2),
                     "sections": _sections(t, loss, thr)}
    return rep


def punch_report(x, sr, hits=None):
    hits = detect_hits(x, sr) if hits is None else hits
    m = hit_metrics(x, sr, hits)
    return hits, m, summary(m)


def punch_score(cmp):
    """Single 'punch loss' number (dB, worst p90 TSR loss over kick/snare/crack/full) for the safety loop."""
    v = [c.get("tsr_loss_db_p90", 0.0) for c in cmp.values() if c.get("hits", 0) >= 4]
    return float(max(v)) if v else 0.0


LOW_BANDS = ((30.0, 80.0), (80.0, 160.0), (160.0, 400.0))


def lowband_metrics(x, sr, win_s=0.4):
    """Low/low-mid balance and dynamics. Per band: level re full-band RMS (dB, loudness-independent),
    crest = median over 400 ms windows of peak/RMS (how 'hitty' the band is, not the song's one peak),
    and for stereo the side/mid ratio of the band (mono-bass check)."""
    x = np.asarray(x, np.float64)
    m = _mono(x)
    tot = 10 * np.log10(np.mean(m ** 2) + 1e-20)
    w = max(1, int(win_s * sr))
    out = {}
    for lo, hi in LOW_BANDS:
        if len(m) < _MIN_LEN or hi >= 0.45 * sr:
            continue
        sos = butter(2, [lo, hi], btype="bandpass", fs=sr, output="sos")
        b = sosfiltfilt(sos, m)
        nb = len(b) // w
        if nb < 1:
            continue  # shorter than one analysis window
        seg = b[: nb * w].reshape(nb, w)
        rms = np.sqrt(np.mean(seg ** 2, axis=1)) + 1e-12
        pk = np.max(np.abs(seg), axis=1) + 1e-12
        act = rms > np.percentile(rms, 90) * 10 ** (-20 / 20)
        r = {"level_re_total_db": round(float(10 * np.log10(np.mean(b ** 2) + 1e-20) - tot), 2),
             "crest_db": round(float(np.median(20 * np.log10(pk[act] / rms[act]))), 2)}
        if x.ndim == 2 and x.shape[1] == 2:
            s = sosfiltfilt(sos, (x[:, 0] - x[:, 1]) * 0.5)
            r["side_mid_db"] = round(float(10 * np.log10((np.mean(s ** 2) + 1e-20) / (np.mean(b ** 2) + 1e-20))), 2)
        out[f"{int(lo)}-{int(hi)}"] = r
    return out


def hit_mask(n, sr, hits, pre_ms=3.0, post_ms=45.0, kinds=("kick", "snare")):
    """0..1 per-sample mask that is 1 from just before each drum onset to post_ms after it,
    with 5 ms raised-cosine edges. Used to keep dynamic EQ / de-esser / saturation off the hits."""
    m = np.zeros(n, np.float64)
    pre, post = int(pre_ms * sr / 1000), int(post_ms * sr / 1000)
    for k in kinds:
        for t in hits.get(k, ()):
            m[max(0, t - pre): min(n, t + post)] = 1.0
    e = max(1, int(0.005 * sr))
    return np.clip(uniform_filter1d(m, 2 * e + 1, mode="nearest") * 1.0, 0.0, 1.0)
