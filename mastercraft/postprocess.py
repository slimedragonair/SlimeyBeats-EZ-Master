"""Export finalisation: optional SRC (soxr if present, else scipy), re-check true peak after SRC
(brief 3.2 step 19), dither and write. Several formats can be written from one master in one go."""
import numpy as np

from . import loudness as LN
from .audio_io import export_audio, mp3_rate, EXPORT_FORMATS, DEFAULT_FORMAT, output_name


def export_formats(general):
    """The formats to write for this run: `general['formats']` (a list) or the legacy single
    `general['format']`. Invalid/unknown entries are dropped, order follows EXPORT_FORMATS."""
    raw = general.get("formats")
    if isinstance(raw, str):
        raw = [p for p in raw.replace(";", ",").split(",")]
    if not raw:
        raw = [general.get("format") or DEFAULT_FORMAT]
    want = {str(f).strip().lower() for f in raw if str(f).strip()}
    out = [k for k in EXPORT_FORMATS if k in want]
    return out or [DEFAULT_FORMAT]


def resample(x, sr, tgt):
    if not tgt or tgt == sr:
        return x, sr
    try:
        import soxr  # LGPL-2.1+, dynamically loaded (installed as a librosa dependency)
        return soxr.resample(x, sr, tgt, quality="VHQ"), tgt
    except Exception:
        from math import gcd
        from scipy.signal import resample_poly
        g = gcd(int(tgt), int(sr))
        return resample_poly(x, tgt // g, sr // g, axis=0), tgt


def finalize_export(y, sr, path, general, ceiling_dbtp, fmt=None):
    fmt = fmt or export_formats(general)[0]
    req = general.get("output_sr") or 0
    y, osr = resample(y, sr, req)
    info = {}
    if fmt == "mp3" and osr != mp3_rate(osr):
        # the MP3 writer only takes 32/44.1/48 kHz: resample here so the TP check sees the real signal
        # and the report shows the real rate (QA M6)
        y, osr = resample(y, osr, mp3_rate(osr))
        info["sample_rate_note"] = f"MP3 supports 32/44.1/48 kHz only; written at {osr} Hz instead of {req or sr} Hz"
    tp = LN.true_peak(y, osr, 4)
    c = 10 ** ((ceiling_dbtp - 0.02) / 20)
    trim = 0.0
    if tp > c:
        trim = 20 * np.log10(c / tp)
        y = y * (c / tp)
    p = export_audio(y, osr, path, fmt)
    info.update({"path": str(p), "format": fmt, "label": EXPORT_FORMATS[fmt][0],
                 "sample_rate": osr, "post_src_trim_db": round(trim, 3)})
    if fmt == "mp3":
        info.update(_verify_mp3(y, osr, p, ceiling_dbtp))
    return p, info


def finalize_exports(y, sr, out_dir, stem, suffix, general, ceiling_dbtp, used=None, avoid_existing=False):
    """Write every selected format from the same master. Returns [(path, info)] in table order.

    Names: `song_master.wav` (24-bit), `song_master_16bit.wav`, `song_master.mp3`, ... so all the
    formats of one song sit next to each other and none can overwrite another."""
    from pathlib import Path
    from .audio_io import unique_output_path
    out_dir = Path(out_dir)
    used = used if used is not None else set()
    req = general.get("output_sr") or 0
    y_req, sr_req = resample(y, sr, req)
    done = []
    for fmt in export_formats(general):
        name = output_name(stem, suffix, fmt)
        p0 = out_dir / name
        p = unique_output_path(p0, used, avoid_existing=avoid_existing)
        g1 = dict(general)
        g1["output_sr"] = sr_req                      # already resampled once for all formats
        p, info = finalize_export(y_req, sr_req, p, g1, ceiling_dbtp, fmt=fmt)
        info["renamed"] = bool(p != p0)
        done.append((p, info))
    return done


def export_notes(exp, targets):
    """Notes about the written file (sample rate change, MP3 as decoded) for res['notes']."""
    n = []
    if exp.get("sample_rate_note"):
        n.append(exp["sample_rate_note"] + ".")
    if "decoded_lufs" in exp:
        miss = exp["decoded_lufs"] - targets["lufs"]
        fix = {"re-limited": "; the encoder's overshoot was taken out by re-limiting, not by turning it down",
               "trim": "; it was turned down to keep the ceiling", "re-limited + trim": "; it was re-limited and turned down"}
        n.append(f"MP3 as written (decoded): {exp['decoded_lufs']:.2f} LUFS, TP {exp['decoded_tp_dbtp']:+.2f} dBTP, "
                 f"{exp['sample_rate']} Hz{fix.get(exp.get('mp3_fix'), '')}.")
        if abs(miss) > 0.2:
            n.append(f"MP3 loudness is {miss:+.2f} LU from the {targets['lufs']:.1f} LUFS target after encoding.")
        if exp.get("mp3_over_ceiling"):
            n.append(f"WARNING: decoded MP3 true peak {exp['decoded_tp_dbtp']:+.2f} dBTP is over the {targets['ceiling_dbtp']:.1f} dBTP ceiling.")
    return n


MP3_TP_MARGIN_DB = 0.1   # decoded MP3 must sit this far under the ceiling (QA N7)


def _verify_mp3(y, osr, p, ceiling_dbtp, max_passes=4):
    """Decode the written MP3, measure it, and fix it if the encoder pushed the true peak over
    (ceiling - margin). QA N7: a plain gain trim cost 0.2-0.36 LU, so the fix is loudness-neutral:
    the pre-encode signal is re-limited to a lower ceiling (by the measured overshoot) at the same
    integrated loudness, re-encoded and measured again. Only if that is not enough is a trim used."""
    import soundfile as sf
    from .dsp.limiter import limit_to_target
    lim_c = ceiling_dbtp - MP3_TP_MARGIN_DB
    l_pre = LN.integrated_lufs(y, osr)
    z, relim_db, trim_db, how = y, 0.0, 0.0, "none"
    for k in range(1, max_passes + 1):
        d, dsr = sf.read(str(p), dtype="float64", always_2d=True)
        dtp = 20 * np.log10(max(LN.true_peak(d, dsr, 4), 1e-12))
        dl = LN.integrated_lufs(d, dsr)
        over = dtp - lim_c
        if over <= 0 or k == max_passes:
            break
        if k <= 2:  # loudness-neutral: lower the limiter ceiling by the encoder's overshoot
            relim_db += over + 0.05
            ztp = 20 * np.log10(max(LN.true_peak(y, osr, 4), 1e-12))
            z, _ = limit_to_target(y, osr, l_pre, min(ztp, ceiling_dbtp) - relim_db, 2.0, 30.0, 4, 0.1, 4, 0.1,
                                   max_gr_abort_db=relim_db + 3.0, release_slow_ms=250.0)
            how = "re-limited"
        else:       # last resort: plain trim
            trim_db -= over + 0.05
            z = z * 10 ** (-(over + 0.05) / 20)
            how = "re-limited + trim" if relim_db else "trim"
        export_audio(z, osr, p, "mp3")
    return {"sample_rate": int(dsr), "decoded_lufs": round(float(dl), 2), "decoded_tp_dbtp": round(float(dtp), 2),
            "mp3_encode_passes": k, "mp3_fix": how, "mp3_relimit_db": round(relim_db, 2),
            "mp3_extra_trim_db": round(trim_db, 2), "mp3_tp_margin_db": MP3_TP_MARGIN_DB,
            "mp3_over_ceiling": bool(dtp > ceiling_dbtp + 0.01)}
