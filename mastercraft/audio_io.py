"""Audio file loading and export (WAV 16/24/32f, FLAC, MP3)."""
import os
import sys
import shutil
import subprocess
from pathlib import Path

import numpy as np
import soundfile as sf

from .paths import TMP_DIR

AUDIO_EXTS = {".wav", ".flac", ".mp3", ".ogg", ".oga", ".aiff", ".aif", ".m4a", ".aac", ".opus", ".wma"}

# key -> (label shown in the UI/CLI, extension, filename tag). The primary format of an extension
# has no tag, so the default single-format export keeps its old name (`song_master.wav`); the others
# get a tag so that exporting several WAV/FLAC depths into one folder can never overwrite itself.
EXPORT_FORMATS = {
    "wav24": ("WAV 24-bit - master", ".wav", ""),
    "wav16": ("WAV 16-bit - CD, TPDF dither", ".wav", "16bit"),
    "wav32f": ("WAV 32-bit float - editing", ".wav", "32f"),
    "flac24": ("FLAC 24-bit - lossless archive", ".flac", ""),
    "flac16": ("FLAC 16-bit - small lossless", ".flac", "16bit"),
    "mp3": ("MP3 320 kbps - streaming / sharing", ".mp3", ""),
}
DEFAULT_FORMAT = "wav24"


def format_label(fmt):
    return EXPORT_FORMATS.get(fmt, (str(fmt), "", ""))[0]


def format_ext(fmt):
    return EXPORT_FORMATS.get(fmt, (fmt, "." + str(fmt), ""))[1]


def format_tag(fmt):
    return EXPORT_FORMATS.get(fmt, (fmt, "", ""))[2]


def output_name(stem, suffix, fmt):
    """'song' + '_master' + '16bit' + '.wav' -> song_master_16bit.wav (tags keep formats apart)."""
    tag = format_tag(fmt)
    return f"{stem}{suffix}{('_' + tag) if tag else ''}{format_ext(fmt)}"


class AudioLoadError(RuntimeError):
    """Readable 'cannot use this file' error (no traceback needed)."""


def load_audio(path):
    """Returns (float64 array (n, ch), sr). See load_audio_ex for the load warnings."""
    x, sr, _ = load_audio_ex(path)
    return x, sr


import contextlib


@contextlib.contextmanager
def _quiet_stderr():
    """Silence C-level stderr (mpg123 inside libsndfile prints 'Note: Illegal Audio-MPEG-Header'
    and similar straight to fd 2; QA N5). Python-level errors are still raised normally."""
    try:
        fd = sys.stderr.fileno() if sys.stderr else 2
        sys.stderr and sys.stderr.flush()
        saved = os.dup(fd)
    except Exception:  # no real stderr (pythonw / GUI): nothing to silence
        yield
        return
    try:
        dn = os.open(os.devnull, os.O_WRONLY)
        os.dup2(dn, fd)
        os.close(dn)
        yield
    finally:
        os.dup2(saved, fd)
        os.close(saved)


_MAGIC = [(b"RIFF", 0, "WAV"), (b"RF64", 0, "WAV (RF64)"), (b"fLaC", 0, "FLAC"), (b"OggS", 0, "Ogg"),
          (b"ID3", 0, "MP3"), (b"FORM", 0, "AIFF"), (b"ftyp", 4, "MP4/M4A"),
          (b"\x30\x26\xb2\x75", 0, "WMA/ASF"), (b"caff", 0, "CAF")]


def _sniff(path):
    """Container named by the first bytes, or None if the header is not a known audio format."""
    try:
        with open(path, "rb") as fh:
            h = fh.read(16)
    except OSError:
        return None
    for m, off, name in _MAGIC:
        if h[off:off + len(m)] == m:
            return name
    if len(h) >= 3 and h[0] == 0xFF and (h[1] & 0xE0) == 0xE0 and (h[1] >> 3 & 3) != 1 and (h[1] >> 1 & 3) != 0 \
            and (h[2] >> 4) not in (0, 15) and (h[2] >> 2 & 3) != 3 \
            and Path(path).suffix.lower() in (".mp3", ".mp2", ".mpa", ".aac"):   # bare frame sync: only trust it for MPEG names
        return "MP3 frame"
    return None


def _sf_reason(err, path):
    """libsndfile's message, made accurate: for a file that exists it says 'does not exist or is not a
    regular file' when it simply doesn't recognise the header (QA N5)."""
    msg = str(err).strip() or type(err).__name__
    if os.path.isfile(path) and ("does not exist" in msg or "not a regular file" in msg or "Format not recognised" in msg):
        kind = _sniff(path)
        msg = ("unrecognised file header (not a known audio format)" if kind is None
               else f"{kind} header found but libsndfile can't decode it (corrupt or unsupported codec)")
    return msg


def load_audio_ex(path):
    """Returns (float64 array (n, ch), sr, warnings). Uses soundfile (libsndfile: WAV/FLAC/OGG/MP3/AIFF).
    For other containers (m4a/aac/wma) it can call the user's own ffmpeg.exe if one is on PATH
    (external program, never bundled or linked). Raises AudioLoadError with a readable message."""
    path = str(path)
    name = Path(path).name
    if not os.path.isfile(path):
        raise AudioLoadError(f"File not found: {path}")
    if os.path.getsize(path) == 0:
        raise AudioLoadError(f"Empty file (0 bytes): {name}")
    warnings = []
    err = None
    try:
        with _quiet_stderr():
            x, sr = sf.read(path, dtype="float64", always_2d=True)
        frames, mask = _wav_header_info(path)
        try:  # QA L9: header promises more frames than the file holds
            if not frames:
                with _quiet_stderr():
                    frames = sf.info(path).frames
            if frames > 0 and len(x) < 0.98 * frames:
                warnings.append(f"Input looks truncated: the header promises {frames / sr:.1f} s but only "
                                f"{len(x) / sr:.1f} s of audio could be read. Re-download or re-export the file.")
        except Exception:
            pass
        x, w2 = _sanitize(x, mask)
        return x, int(sr), warnings + w2
    except Exception as e:  # noqa
        err = e
    ff = shutil.which("ffmpeg")
    ff_msg = "no ffmpeg on PATH for other formats"
    if ff:
        import uuid
        TMP_DIR.mkdir(parents=True, exist_ok=True)
        tmp = TMP_DIR / f"decode_{os.getpid()}_{uuid.uuid4().hex[:8]}.wav"
        try:
            r = subprocess.run([ff, "-y", "-v", "error", "-i", path, "-c:a", "pcm_f32le", str(tmp)],
                               capture_output=True, text=True, errors="replace",
                               creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            if r.returncode == 0 and tmp.exists():
                x, sr = sf.read(str(tmp), dtype="float64", always_2d=True)
                x, w2 = _sanitize(x, _wav_header_info(str(tmp))[1])
                return x, int(sr), warnings + w2
            lines = [l.strip() for l in (r.stderr or "").splitlines() if l.strip()]
            ff_msg = "ffmpeg: " + (lines[-1] if lines else f"failed (exit {r.returncode})")
        except Exception as e2:  # noqa
            ff_msg = f"ffmpeg: {e2}"
        finally:
            try:
                tmp.unlink()
            except OSError:
                pass
    raise AudioLoadError(f"Not an audio file, corrupt, or unsupported format: {name} "
                         f"(libsndfile: {_sf_reason(err, path)}; {ff_msg})")


def _wav_declared_frames(path):
    return _wav_header_info(path)[0]


def _wav_header_info(path):
    """(frames the RIFF/WAVE data chunk claims, WAVE_FORMAT_EXTENSIBLE dwChannelMask or 0).
    libsndfile reports what is actually there, and doesn't expose the speaker mask."""
    import struct
    frames, mask = 0, 0
    try:
        with open(path, "rb") as fh:
            h = fh.read(12)
            if len(h) < 12 or h[:4] != b"RIFF" or h[8:12] != b"WAVE":
                return 0, 0
            align = 0
            for _ in range(64):
                ch = fh.read(8)
                if len(ch) < 8:
                    break
                cid, size = ch[:4], struct.unpack("<I", ch[4:])[0]
                if cid == b"fmt ":
                    fmt = fh.read(size)
                    align = struct.unpack("<H", fmt[12:14])[0] if len(fmt) >= 14 else 0
                    if len(fmt) >= 24 and struct.unpack("<H", fmt[:2])[0] == 0xFFFE:
                        mask = struct.unpack("<I", fmt[20:24])[0]
                    if size % 2:
                        fh.read(1)
                    continue
                if cid == b"data":
                    frames = size // align if align and size != 0xFFFFFFFF else 0
                    break
                fh.seek(size + (size % 2), 1)
    except (OSError, struct.error):
        pass
    return frames, mask


# WAVE_FORMAT_EXTENSIBLE speaker bits -> (short name, gain to L, gain to R). -3 dB = 0.7071.
_G3 = 0.70710678
SPEAKERS = [("L", 1, 0), ("R", 0, 1), ("C", _G3, _G3), ("LFE", _G3, _G3), ("Ls", _G3, 0), ("Rs", 0, _G3),
            ("Lc", 1, 0), ("Rc", 0, 1), ("Cs", _G3, _G3), ("Lss", _G3, 0), ("Rss", 0, _G3),
            ("Tc", _G3, _G3), ("Tfl", _G3, 0), ("Tfc", _G3, _G3), ("Tfr", 0, _G3), ("Tbl", _G3, 0),
            ("Tbc", _G3, _G3), ("Tbr", 0, _G3)]
# standard orders when the file carries no mask (WAV/FLAC/Vorbis channel order, SMPTE)
DEFAULT_MASKS = {3: 0x7, 4: 0x33, 5: 0x37, 6: 0x3F, 7: 0x70F, 8: 0x63F}
LAYOUT_NAMES = {0x7: "3.0", 0x33: "quad", 0x107: "3.1 surround", 0x37: "5.0", 0x607: "5.0 (side)",
                0x3F: "5.1", 0x60F: "5.1 (side)", 0x70F: "6.1", 0x63F: "7.1", 0xFF: "7.1 (wide)"}


def fold_matrix(n, mask=0):
    """(n x 2 matrix, description, layout_known). Layout-aware downmix (QA N4): centre and LFE go
    to both sides at -3 dB, surrounds at -3 dB to their own side, fronts at unity. Unknown layouts
    are folded alternately L/R with equal-power gains."""
    known = True
    if not mask or bin(mask).count("1") != n:
        mask = DEFAULT_MASKS.get(n, 0)
    if not mask:
        known = False
        m = np.zeros((n, 2))
        g = np.sqrt(2.0 / n)
        m[0::2, 0] = g
        m[1::2, 1] = g
        return m, f"unknown {n}-channel layout", False
    rows, names = [], []
    for bit, (nm, gl, gr) in enumerate(SPEAKERS):
        if mask >> bit & 1:
            rows.append((gl, gr))
            names.append(nm)
    if len(rows) != n:  # mask uses bits beyond the table
        return fold_matrix(n, 0)
    lay = LAYOUT_NAMES.get(mask, f"mask 0x{mask:X}")
    return np.array(rows, dtype=np.float64), f"{lay}: " + " ".join(names), known


def _sanitize(x, mask=0):
    """-> (x, warnings). Mono stays (n,1); >2 channels are downmixed to stereo; NaN/Inf zeroed."""
    x = np.asarray(x, dtype=np.float64)
    w = []
    if x.ndim == 1:
        x = x[:, None]
    bad = ~np.isfinite(x)
    if bad.any():
        w.append(f"Input contained {int(bad.sum())} NaN/Inf samples; they were replaced with silence.")
        x = np.nan_to_num(x, nan=0.0, posinf=0.0, neginf=0.0)
    if x.shape[1] > 2:
        n = x.shape[1]
        m, desc, known = fold_matrix(n, mask)
        x = x @ m
        if known:
            w.append(f"Input has {n} channels ({desc}); downmixed to stereo: centre/LFE at -3 dB into both sides, "
                     f"surrounds at -3 dB into their own side.")
        else:
            w.append(f"Input has {n} channels ({desc}); folded to stereo with equal-power gains "
                     f"(odd channels -> L, even -> R). Check the result; export a stereo mix for best results.")
    return x, w


def report_path_for(op):
    """JSON report next to a master. WAV keeps `<name>.report.json`; other formats add the
    format so a WAV and an MP3 master of the same song never share one report (QA N3)."""
    op = Path(op)
    if op.suffix.lower() == ".wav":
        return op.with_suffix(".report.json")
    return op.with_name(f"{op.stem}_{op.suffix[1:].lower()}.report.json")


def unique_output_path(path, used=None, avoid_existing=False):
    """Output path that no other file of the same batch uses (`used` = set of normcase'd paths,
    updated in place). With avoid_existing, also skips names that already exist on disk.
    Collisions get _2, _3, .. before the extension (QA M2)."""
    path = Path(path)
    used = used if used is not None else set()
    k = 1
    cand = path
    def taken(c):
        r = report_path_for(c)
        return (os.path.normcase(str(c.absolute())) in used or os.path.normcase(str(r.absolute())) in used
                or (avoid_existing and (c.exists() or r.exists())))
    while taken(cand):
        k += 1
        cand = path.with_name(f"{path.stem}_{k}{path.suffix}")
    used.add(os.path.normcase(str(cand.absolute())))
    used.add(os.path.normcase(str(report_path_for(cand).absolute())))
    return cand


MP3_RATES = (32000, 44100, 48000)


def mp3_rate(sr):
    """The sample rate libsndfile's MP3 writer will use for `sr`."""
    return sr if sr in MP3_RATES else (48000 if sr > 44100 else 44100)


def tpdf_dither(x, bits, rng=None):
    rng = rng or np.random.default_rng(12345)
    lsb = 1.0 / (2 ** (bits - 1))
    d = (rng.random(x.shape) - rng.random(x.shape)) * lsb
    return x + d


def mp3_available():
    try:
        return "MP3" in sf.available_formats()
    except Exception:
        return False


def export_audio(x, sr, path, fmt="wav24", mp3_bitrate=320):
    """Write x (n, ch) to path. Returns final path."""
    x = np.nan_to_num(np.asarray(x, dtype=np.float64))
    x = np.clip(x, -1.0, 1.0)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    if fmt in ("wav16", "flac16"):
        y = np.clip(tpdf_dither(x, 16), -1.0, 1.0 - 1.0 / 32768)
        sub = "PCM_16"
    elif fmt in ("wav24", "flac24"):
        y, sub = x, "PCM_24"
    elif fmt == "wav32f":
        y, sub = x.astype(np.float32), "FLOAT"
    elif fmt == "mp3":
        return _export_mp3(x, sr, path, mp3_bitrate)
    else:
        raise ValueError(f"unknown format {fmt}")
    container = "FLAC" if fmt.startswith("flac") else "WAV"
    sf.write(str(path), y, sr, subtype=sub, format=container)
    return path


def _export_mp3(x, sr, path, bitrate):
    if sr not in MP3_RATES:  # finalize_export already resamples; this is the fallback for direct calls
        from scipy.signal import resample_poly
        from math import gcd
        tgt = mp3_rate(sr)
        g = gcd(tgt, sr)
        x = np.clip(resample_poly(x, tgt // g, sr // g, axis=0), -1, 1)
        sr = tgt
    if not mp3_available():
        raise RuntimeError("MP3 export unavailable: the installed libsndfile has no MP3 writer")
    # libsndfile's built-in MP3 writer (soundfile >= 0.12 / libsndfile >= 1.1)
    sf.write(str(path), x.astype(np.float32), sr, format="MP3", subtype="MPEG_LAYER_III",
             compression_level=0.0, bitrate_mode="CONSTANT")
    return path
