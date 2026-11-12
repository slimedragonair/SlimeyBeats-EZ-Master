"""Master-Craft command line (batch).

Examples:
  python -m mastercraft.cli song.wav --profile Pop
  python -m mastercraft.cli "D:\\Suno\\*.wav" --profile "Hip-Hop/Trap" --mode loud --format wav24,mp3 --out D:\\Masters
  python -m mastercraft.cli song.wav --reference "D:\\Refs\\favourite.wav" --mode loud
  python -m mastercraft.cli song.mp3 --analyze-only --report-json song_report.json
  python -m mastercraft.cli --calibrate refs\\*.wav --name "My Pop refs" --base pop

Exit codes: 0 = every file OK, 1 = some files failed (the rest were written),
2 = usage error (bad option, unknown profile, no inputs, bad --settings file),
3 = every file failed.
"""
import argparse
import glob
import json
import os
import sys
import time
from pathlib import Path

from .paths import setup_env, OUTPUT_DIR


def _expand(inputs, missing=None):
    """Files from paths/folders/globs. Plain paths that don't exist (and globs that match nothing)
    are appended to `missing` so the batch can report them (QA N1)."""
    files = []
    from .audio_io import AUDIO_EXTS
    for pat in inputs:
        p = Path(pat)
        if p.is_dir():
            files += [str(f) for f in sorted(p.iterdir()) if f.suffix.lower() in AUDIO_EXTS]
            continue
        g = sorted(glob.glob(pat, recursive=True))
        files += g if g else ([pat] if p.exists() else [])
        if not g and not p.exists() and missing is not None:
            missing.append(pat)
    seen, out = set(), []
    for f in files:
        k = os.path.normcase(os.path.abspath(f))
        if k not in seen:
            seen.add(k)
            out.append(f)
    return out


def _json_default(o):
    import numpy as np
    if isinstance(o, (np.floating,)):
        return float(o)
    if isinstance(o, (np.integer,)):
        return int(o)
    if isinstance(o, np.ndarray):
        return o.tolist()
    if isinstance(o, (np.bool_,)):
        return bool(o)
    return str(o)


def json_safe(o):
    """Plain JSON types; NaN/inf -> null (QA v0.2.0 #3: reports must be valid JSON)."""
    import math
    import numpy as np
    if isinstance(o, dict):
        return {str(k): json_safe(v) for k, v in o.items()}
    if isinstance(o, (list, tuple)):
        return [json_safe(v) for v in o]
    if isinstance(o, np.ndarray):
        return json_safe(o.tolist())
    if isinstance(o, (bool, np.bool_)):
        return bool(o)
    if isinstance(o, (int, np.integer)):
        return int(o)
    if isinstance(o, (float, np.floating)):
        f = float(o)
        return f if math.isfinite(f) else None
    if o is None or isinstance(o, str):
        return o
    return _json_default(o)


def dump_json(obj, path):
    Path(path).write_text(json.dumps(json_safe(obj), indent=2, allow_nan=False), encoding="utf-8")


def main(argv=None):
    setup_env()
    from .audio_io import EXPORT_FORMATS
    fmt_keys = list(EXPORT_FORMATS)
    ap = argparse.ArgumentParser(prog="mastercraft", description="Master-Craft: automatic mastering for Suno songs")
    ap.add_argument("inputs", nargs="*", help="files, folders or globs")
    ap.add_argument("-p", "--profile", default=None,
                    help='genre profile, or a blend like "metal:0.6+edm:0.4" (see --list-profiles)')
    ap.add_argument("-m", "--mode", choices=["streaming_safe", "loud"], default=None)
    ap.add_argument("--spotify-strict", action="store_true",
                    help="with --mode loud only: use a -2 dBTP ceiling (does not switch loud mode on)")
    ap.add_argument("-l", "--lufs", type=float, default=None, help="target integrated LUFS (overrides profile/mode)")
    ap.add_argument("-c", "--ceiling", type=float, default=None, help="true-peak ceiling dBTP")
    ap.add_argument("-i", "--intensity", type=float, default=None, help="0..1.5 (default 1.0)")
    ap.add_argument("-f", "--format", default=None,
                    help="one or more formats, comma separated (all written from one master): " + ", ".join(fmt_keys)
                         + "  e.g. -f wav24,mp3,flac16")
    ap.add_argument("--sr", type=int, default=None, help="output sample rate (0 = keep)")
    ap.add_argument("-o", "--out", default=None, help="output folder")
    ap.add_argument("--suffix", default=None)
    ap.add_argument("--analyze-only", action="store_true")
    ap.add_argument("--report-json", default=None, help="JSON report path (single input) or folder")
    ap.add_argument("--no-report", action="store_true", help="don't write the per-file JSON report next to masters")
    ap.add_argument("--disable", default="", help="comma list of modules to switch off, e.g. denoise,saturation,stereo")
    ap.add_argument("--settings", default=None, help="settings JSON to use instead of the app's settings.json")
    ap.add_argument("--no-safety", action="store_true", help="skip the post-process safety re-analysis")
    ap.add_argument("--list-profiles", action="store_true")
    ap.add_argument("--calibrate", action="store_true",
                    help="measure reference tracks (inputs) and save their median tonal curve as a custom profile")
    ap.add_argument("--name", default=None, help="custom profile name for --calibrate")
    ap.add_argument("--base", default="neutral", help="base profile for --calibrate")
    ap.add_argument("--no-overwrite", action="store_true",
                    help="never replace existing output files (add _2, _3 ..). Names are always unique within one batch")
    ap.add_argument("--eq-preset", metavar="FILE", default=None,
                    help="apply a Manual EQ preset (JSON from the GUI) after the auto chain; loudness and ceiling are held")
    ap.add_argument("--reference", metavar="FILE", default=None,
                    help="match the tonal balance (and optionally width/loudness) of a reference track; "
                         "the profile then only supplies dynamics and punch limits")
    ap.add_argument("--match-strength", type=float, default=None,
                    help="0..1.5 scale of the reference match (default 1.0)")
    ap.add_argument("--match-loudness", action="store_true",
                    help="with --reference: take the loudness target from the reference too")
    ap.add_argument("--no-match-width", action="store_true",
                    help="with --reference: keep the profile's stereo width instead of the reference's")
    ap.add_argument("--debug", action="store_true", help="print full tracebacks for failures")
    ap.add_argument("--quiet", action="store_true")
    a = ap.parse_args(argv)

    from . import settings as SET, profiles as PR
    if a.list_profiles:
        for k, d, c in PR.list_profiles():
            print(f"{k:24s} {d}")
        print("\nblends (weighted mix of two curves), e.g.:")
        for ex in PR.COMBO_EXAMPLES:
            print(f"  -p \"{ex.split('  ')[0]}\"   {ex.split('(')[-1].rstrip(')')}")
        return 0
    def _prof_key(name):
        try:
            return PR.resolve_key(name)
        except KeyError:
            ap.error(f"unknown profile '{name}'. Known: " + ", ".join(k for k, _, _ in PR.list_profiles()))

    missing = []
    files = _expand(a.inputs, missing)
    if not files:
        ap.error("no input files found" + (": " + ", ".join(missing) if missing else ""))

    if a.calibrate:
        _prof_key(a.base)
        return calibrate(files, a.name or "Calibrated", a.base)

    if a.settings:
        try:
            s = SET.load_file(a.settings)
        except Exception as e:
            ap.error(f"cannot use --settings {a.settings}: {e}")
    else:
        s = SET.load()
    if SET.LAST_MIGRATION:
        print("note: old defaults in the settings file were upgraded in memory ("
              + ", ".join(f"{k.split('.')[-1]} {o} -> {n}" for k, o, n in SET.LAST_MIGRATION) + "); the file is not modified.")
    g = s["general"]
    if a.profile:
        g["profile"] = _prof_key(a.profile)
    if a.mode:
        g["mode"] = a.mode
    if a.spotify_strict:
        g["spotify_strict"] = True
        if (a.mode or g.get("mode")) != "loud":
            print("warning: --spotify-strict only applies in loud mode (add --mode loud); ignored.", file=sys.stderr)
    if a.lufs is not None:
        g["target_lufs"] = a.lufs
    if a.ceiling is not None:
        g["ceiling_dbtp"] = a.ceiling
    if a.intensity is not None:
        g["intensity"] = a.intensity
    # QA L3: say so when a value is out of range instead of clamping silently
    if g.get("intensity") is not None and not 0.0 <= float(g["intensity"]) <= 1.5:
        v = min(max(float(g["intensity"]), 0.0), 1.5)
        print(f"warning: intensity {g['intensity']} is outside 0..1.5; using {v}.", file=sys.stderr)
        g["intensity"] = v
    if g.get("ceiling_dbtp") is not None and float(g["ceiling_dbtp"]) > 0.0:
        print(f"warning: ceiling {g['ceiling_dbtp']} dBTP is above 0; using 0.0 dBTP.", file=sys.stderr)
        g["ceiling_dbtp"] = 0.0
    if g.get("ceiling_dbtp") is not None and float(g["ceiling_dbtp"]) < -6.0:
        print(f"warning: ceiling {g['ceiling_dbtp']} dBTP is unusually low.", file=sys.stderr)
    if g.get("target_lufs") is not None and not -30.0 <= float(g["target_lufs"]) <= -5.0:
        print(f"warning: target {g['target_lufs']} LUFS is outside the usual -30..-5 range; it may not be reachable.",
              file=sys.stderr)
    if a.format:
        want = [p.strip().lower() for p in a.format.replace(";", ",").split(",") if p.strip()]
        bad = [w for w in want if w not in EXPORT_FORMATS]
        if bad:
            ap.error(f"unknown format(s) {', '.join(bad)}. Known: {', '.join(fmt_keys)}")
        ordered = [k for k in EXPORT_FORMATS if k in want]     # table order, duplicates dropped
        g["formats"] = ordered
        g["format"] = ordered[0]
    if a.sr is not None:
        g["output_sr"] = a.sr
    if a.suffix is not None:
        g["suffix"] = a.suffix
    g["suffix"] = (g.get("suffix") or "").strip()
    if a.no_safety:
        s["safety"]["enabled"] = False
    for mname in filter(None, [m.strip() for m in a.disable.split(",")]):
        if mname in s["modules"]:
            s["modules"][mname]["enabled"] = False
        else:
            ap.error(f"unknown module '{mname}'. Known: {', '.join(s['modules'])}")
    # custom profiles may carry settings
    prof = PR.get_profile(g["profile"])
    if prof.get("settings"):
        SET._merge(s, {"modules": prof["settings"].get("modules", {})})
    out_dir = Path(a.out or g.get("out_dir") or OUTPUT_DIR)

    from .audio_io import load_audio_ex, mp3_available, AudioLoadError, unique_output_path, report_path_for
    from .analysis import analyze, summary_lines
    from .engine import master, strip_internal
    from .postprocess import finalize_exports, export_notes, export_formats
    fmts = export_formats(g)
    if "mp3" in fmts and not mp3_available():
        if len(fmts) == 1:
            print("MP3 export is not available in this installation; use wav/flac.", file=sys.stderr)
            return 2
        print("warning: MP3 export is not available in this installation; skipping the MP3 file.", file=sys.stderr)
        g["formats"] = [f for f in fmts if f != "mp3"]
        g["format"] = g["formats"][0]

    eq_state = None
    if a.eq_preset:
        from . import manual_eq as MEQ
        try:
            eq_state = MEQ.load_preset(a.eq_preset)
        except Exception as e:
            ap.error(f"can't read --eq-preset {a.eq_preset}: {e}")
    # ---- reference track (one analysis for the whole batch)
    ref = None
    ref_path = a.reference or g.get("reference") or None
    rm = s["modules"].setdefault("reference_match", {})
    if a.match_strength is not None:
        rm["strength"] = float(min(max(a.match_strength, 0.0), 1.5))
    if a.match_loudness:
        rm["match_loudness"] = True
    if a.no_match_width:
        rm["match_width"] = False
    if ref_path:
        from . import reference as REF
        try:
            ref = REF.load_target(ref_path)
            if not a.quiet:
                print(f"reference: {ref_path} ({ref['lufs']:.1f} LUFS, PLR {ref['plr_db']:.1f} dB, "
                      f"side/mid {ref['sm_db']:.1f} dB, match strength {rm.get('strength', 1.0):.2f}"
                      + (", loudness from the reference" if rm.get("match_loudness") else "") + ")")
        except Exception as e:
            ap.error(f"can't read --reference {ref_path}: {e}")
    n_fail = 0
    used = set()
    for m in missing:   # QA N1: a typo in one name of a batch must not pass silently
        print(f"[-] {m}\n    FAILED: File not found: {m}", file=sys.stderr)
        n_fail += 1
    n_total = len(files) + len(missing)
    for idx, f in enumerate(files, 1):
        t0 = time.time()
        print(f"[{idx}/{len(files)}] {f}")
        try:
            x, sr, load_warn = load_audio_ex(f)
            for w in load_warn:
                print("    warning:", w)
            if a.analyze_only:
                rep = analyze(x, sr, prof, f)
                rep["warnings"] = load_warn + rep.get("warnings", [])
                out = strip_internal(rep)
                if not a.quiet:
                    for line in summary_lines(rep):
                        print("   ", line)
                    if ref is not None:
                        from . import reference as REF
                        from .profiles import BAND_CENTERS
                        res_b = REF.residual_bands(x, sr, ref, list(BAND_CENTERS))
                        print("    vs reference (dB):", ", ".join(f"{k} {v:+.1f}" for k, v in res_b.items()))
                rp = _report_path(a.report_json, f, len(files), out_dir, "_analysis.json")
                dump_json(out, rp)
                print(f"    report -> {rp}")
            else:
                def prog(p, msg):
                    if not a.quiet:
                        print(f"\r    {p:3d}% {msg:<20s}", end="", flush=True)
                y, res = master(x, sr, s, f, progress=prog, reference=ref)
                print()
                if eq_state is not None:
                    from . import manual_eq as MEQ
                    from .analysis import level_metrics
                    y, eqi = MEQ.apply(y, sr, eq_state, res["output"]["metrics"]["integrated_lufs"], res["targets"]["ceiling_dbtp"])
                    res["manual_eq"] = eqi
                    # the report / out: line describe the file that is written (post-EQ); auto-chain values kept
                    res["output"]["metrics_before_manual_eq"] = res["output"]["metrics"]
                    res["output"]["metrics"] = level_metrics(y, sr)
                    tp_s = f"{eqi['true_peak_dbtp']:+.2f}" if eqi.get("true_peak_dbtp") is not None else "n/a"
                    res["notes"].append(f"Manual EQ preset applied after the auto chain (hold gain {eqi['hold_gain_db']:+.2f} dB): "
                                        f"{eqi['lufs']:.2f} LUFS, TP {tp_s} dBTP. Output metrics are post-EQ; "
                                        f"spectrum/punch in this report are of the auto master.")
                    res["notes"] += eqi.get("notes", [])
                res["notes"] = load_warn + res["notes"]
                exports = finalize_exports(y, sr, out_dir, Path(f).stem, g["suffix"], g,
                                           res["targets"]["ceiling_dbtp"], used=used,
                                           avoid_existing=a.no_overwrite)
                if any(e.get("renamed") for _p, e in exports):
                    print("    note: output name already used in this batch"
                          + (" or on disk" if a.no_overwrite else "") + f"; writing "
                          + ", ".join(Path(p).name for p, e in exports if e.get("renamed")))
                res["exports"] = [e for _p, e in exports]
                res["export"] = exports[0][1]
                for _p, e in exports:
                    res["notes"] += export_notes(e, res["targets"])
                i, o = res["input"]["metrics"], res["output"]["metrics"]
                print(f"    in : {i['integrated_lufs']:6.2f} LUFS  TP {i['true_peak_dbtp']:+.2f} dBTP  LRA {i['lra_lu']:.1f}  PLR {i['plr_db']:.1f}")
                print(f"    out: {o['integrated_lufs']:6.2f} LUFS  TP {o['true_peak_dbtp']:+.2f} dBTP  LRA {o['lra_lu']:.1f}  PLR {o['plr_db']:.1f}"
                      f"   (target {res['targets']['lufs']:.1f} / {res['targets']['ceiling_dbtp']:.1f})")
                for op_, exp in exports:
                    if "decoded_lufs" in exp:
                        print(f"    mp3: {exp['decoded_lufs']:6.2f} LUFS  TP {exp['decoded_tp_dbtp']:+.2f} dBTP  (decoded file, "
                              f"{exp['sample_rate']} Hz, {exp['mp3_encode_passes']} encode pass(es), fix: {exp.get('mp3_fix', 'none')})")
                    elif exp["sample_rate"] != sr:
                        print(f"    {exp['format']}: {exp['sample_rate']} Hz (resampled from {sr} Hz), "
                              f"post-SRC trim {exp['post_src_trim_db']:.2f} dB")
                pu = res.get("punch")
                if pu:
                    v = pu["vs_input"]
                    print("    punch: loss p90 " + ", ".join(
                        f"{k} {v[k].get('tsr_loss_db_p90', 0):+.1f} dB ({100 * v[k].get('frac_hits_smeared', 0):.0f}% smeared)"
                        for k in v if v[k].get("hits", 0)) + f"  -> score {pu['score_db']:.1f} (limit {pu['threshold_db']:.1f})")
                if not a.quiet:
                    print("    chain: " + " > ".join(st["module"] for st in res["plan"]["steps"]))
                    for nline in res["notes"]:
                        print("    note:", nline)
                for op_, exp in exports:
                    print(f"    -> {op_}  ({exp['label']})")
                print(f"    done in {time.time() - t0:.1f}s")
                if a.report_json or not a.no_report and g.get("write_report", True):
                    for op_, exp in exports:      # one report per written file
                        if a.report_json:
                            rp = _report_path(a.report_json, f, len(files), op_.parent, "_master_report.json")
                            if len(exports) > 1:
                                rp = rp.with_name(rp.stem + f"_{exp['format']}" + rp.suffix)
                        else:
                            rp = report_path_for(op_)
                        dump_json({**res, "export": exp}, rp)
        except AudioLoadError as e:  # keep going on batch
            print(f"    FAILED: {e}", file=sys.stderr)
            n_fail += 1
        except Exception as e:  # keep going on batch
            if a.debug or os.environ.get("MASTERCRAFT_DEBUG"):
                import traceback
                traceback.print_exc()
            print(f"    FAILED: {type(e).__name__}: {e}  (run with --debug for details)", file=sys.stderr)
            n_fail += 1
    if n_total > 1 or n_fail:
        print(f"Done: {n_total - n_fail} OK, {n_fail} failed.")
    return 0 if n_fail == 0 else (3 if n_fail == n_total else 1)


def _report_path(arg, f, nfiles, default_dir, suffix):
    if arg and nfiles == 1 and not Path(arg).is_dir() and arg.lower().endswith(".json"):
        Path(arg).parent.mkdir(parents=True, exist_ok=True)
        return Path(arg)
    d = Path(arg) if arg else Path(default_dir)
    d.mkdir(parents=True, exist_ok=True)
    return d / (Path(f).stem + suffix)


def calibrate(files, name, base):
    """Brief section 6 step 3: median octave-band curve of reference tracks -> custom profile."""
    import numpy as np
    from .audio_io import load_audio
    from .analysis import analyze
    from . import profiles as PR
    prof = PR.get_profile(base)
    curves, corrs, lufs = [], [], []
    for f in files:
        try:
            x, sr = load_audio(f)
        except Exception as e:
            print(f"  skipped {f}: {e}", file=sys.stderr)
            continue
        r = analyze(x, sr, prof, f, fast=True)
        curves.append(r["tonal"]["measured_db"])
        lufs.append(r["metrics"]["integrated_lufs"])
        if r.get("stereo"):
            corrs.append(r["stereo"]["corr"])
        print(f"  {Path(f).name}: {r['metrics']['integrated_lufs']:.1f} LUFS, curve {r['tonal']['measured_db']}")
    if not curves:
        print("No readable reference files.", file=sys.stderr)
        return 3
    med = np.median(np.array(curves), axis=0)
    prof["tonal_db"] = [round(float(v), 1) for v in med]
    if corrs:
        c = float(np.median(corrs))
        prof["stereo"]["corr_window"] = [round(max(0.3, c - 0.1), 2), round(min(0.98, c + 0.05), 2)]
    prof["lufs"]["loud"] = round(float(np.median(lufs)), 1)
    prof["calibrated_from"] = [Path(f).name for f in files]
    p = PR.save_custom_profile(name, prof, base=base.lower() if base.lower() in PR.BUILTIN else "neutral")
    print(f"Saved custom profile '{name}' -> {p}\n  tonal_db = {prof['tonal_db']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
