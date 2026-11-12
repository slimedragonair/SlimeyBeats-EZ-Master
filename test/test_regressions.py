r"""Regression tests for the QA report on 519b6b1 (H1-M8, L-items, preview temp files).

Run:  .venv\Scripts\python.exe test\test_regressions.py
All outputs go to test\out_reg\. Never touches the app's settings.json (uses copies under test\)."""
import json
import math
import os
import shutil
import subprocess
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
OUT = ROOT / "test" / "out_reg"
OUT.mkdir(parents=True, exist_ok=True)
os.environ["MASTERCRAFT_SETTINGS"] = str(OUT / "reg_settings.json")   # never the user's file
os.environ["MASTERCRAFT_DATA_DIR"] = str(OUT / "data_dir")             # presets of the tests stay under test\
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

from mastercraft import loudness as LN, punch as P, settings as SET
from mastercraft.audio_io import load_audio_ex, AudioLoadError, unique_output_path, _sanitize
from mastercraft.engine import master
from mastercraft.analysis import analyze
from mastercraft.profiles import get_profile
from mastercraft.postprocess import finalize_export

RESULTS = []
QA_IN = ROOT / "test" / "qa" / "inputs"
PY = sys.executable


def test(fn):
    only = [s for s in os.environ.get("MC_ONLY", "").split(",") if s]
    if only and not any(s in fn.__name__ for s in only):
        return fn
    try:
        fn()
        RESULTS.append((fn.__name__, "PASS", ""))
        print(f"PASS  {fn.__name__}")
    except Exception as e:  # noqa
        RESULTS.append((fn.__name__, "FAIL", f"{type(e).__name__}: {e}"))
        print(f"FAIL  {fn.__name__}: {e}")
        traceback.print_exc()
    return fn


def song(sr=48000, sec=8.0, seed=1):
    """Small drum-ish test signal: kicks + snares + pad, stereo."""
    rng = np.random.default_rng(seed)
    n = int(sr * sec)
    t = np.arange(n) / sr
    y = 0.05 * np.sin(2 * np.pi * 220 * t) + 0.03 * np.sin(2 * np.pi * 330 * t)
    for k, s0 in enumerate(np.arange(0.25, sec - 0.5, 0.5)):
        i = int(s0 * sr)
        m = min(n - i, int(0.25 * sr))
        tt = np.arange(m) / sr
        if k % 2 == 0:
            y[i:i + m] += 0.6 * np.sin(2 * np.pi * 55 * tt * (1 + 2 * np.exp(-tt * 40))) * np.exp(-tt * 12)
        else:
            y[i:i + m] += 0.3 * rng.standard_normal(m) * np.exp(-tt * 25)
    y = y + 0.002 * rng.standard_normal(n)
    return np.stack([y, 0.95 * y], 1) * 0.5


def write(name, x, sr, **kw):
    p = OUT / name
    sf.write(str(p), x, sr, **kw)
    return p


def defaults(**g):
    s = SET.defaults()
    s["general"].update(g)
    return s


# ---------------------------------------------------------------- H1
@test
def h1_true_peak_matches_unchunked():
    rng = np.random.default_rng(3)
    n = (1 << 20) * 2 + 5000
    x = 0.2 * rng.standard_normal((n, 2))
    x[(1 << 20) - 2] = 0.9          # loud sample right at a chunk edge
    x[(1 << 20) - 1] = -0.9
    ref = np.max(np.abs(resample_poly(x, 4, 1, axis=0)))
    got = LN.true_peak(x, 48000, 4)
    assert abs(20 * np.log10(got / ref)) < 0.02, (got, ref)


@test
def h1_qa_repro_reaches_target():
    f = QA_IN / "rc_excerpt40.wav"
    if not f.exists():
        return
    x, sr, _ = load_audio_ex(f)
    s = defaults(profile="pop", mode="loud", spotify_strict=True)
    y, res = master(x, sr, s, str(f))
    out = res["output"]["metrics"]["integrated_lufs"]
    assert abs(out - res["targets"]["lufs"]) <= 0.3 or any("LU from the" in n or "not reachable" in n for n in res["notes"]), \
        (out, res["notes"])
    assert LN.true_peak(y, sr, 4) <= 10 ** ((res["targets"]["ceiling_dbtp"] + 0.05) / 20)


# ---------------------------------------------------------------- H2 / L4
@test
def h2_low_sample_rates_master():
    for sr in (8000, 11025, 22050):
        x = song(sr, 6.0)
        y, res = master(x, sr, defaults(profile="rock"))
        assert np.isfinite(y).all() and res["output"]["metrics"]["integrated_lufs"] > -30, sr


@test
def l4_tiny_inputs_no_crash_no_nan():
    for n in (1, 15, 2400):
        x = np.zeros((n, 2)); x[0] = 0.5
        h = P.detect_hits(x, 48000)
        m = P.hit_metrics(x, 48000, h)
        c = P.compare(m, m, h, 48000)
        assert all(v.get("hits", 0) == 0 for v in c.values())
        P.lowband_metrics(x, 48000)
    x = song(48000, 0.05)
    h = P.detect_hits(x, 48000); m = P.hit_metrics(x, 48000, h)
    s = json.dumps(P.summary(m)) + json.dumps(P.compare(m, m, h, 48000))
    assert "NaN" not in s, s


# ---------------------------------------------------------------- M1
@test
def m1_migration_on_copy():
    src = ROOT / "test" / "qa" / "user_settings_copy.json"
    if not src.exists():
        src = OUT / "fake_old.json"
        src.write_text(json.dumps({"general": {"format": "wav32f"},
                                   "modules": {"limiter": {"lookahead_ms": 3.0, "release_ms": 100.0}}}))
    cp = OUT / "migrate_copy.json"
    shutil.copyfile(src, cp)
    raw_before = json.loads(cp.read_text(encoding="utf-8"))
    ch = SET.preview_migration(cp)
    assert cp.read_bytes() == src.read_bytes()           # preview is read-only
    assert ch == [("modules.limiter.lookahead_ms", 3.0, 2.0), ("modules.limiter.release_ms", 100.0, 30.0)], ch
    s = SET.load_file(cp)
    assert s["modules"]["limiter"]["lookahead_ms"] == 2.0 and s["modules"]["limiter"]["release_ms"] == 30.0
    assert "settings_version" not in s
    assert s["general"]["format"] == raw_before["general"]["format"]       # user values kept
    old_file, SET.SETTINGS_FILE = SET.SETTINGS_FILE, cp
    try:
        SET.save(s)
        saved = json.loads(cp.read_text(encoding="utf-8"))
        assert saved["settings_version"] == SET.SETTINGS_VERSION
        s2 = SET.load()
        assert SET.LAST_MIGRATION == [] and s2 == s
    finally:
        SET.SETTINGS_FILE = old_file
    # a user-changed limiter value is left alone
    custom = {"modules": {"limiter": {"lookahead_ms": 5.0, "release_ms": 100.0}}}
    assert SET.migrate(custom)[1] == [("modules.limiter.release_ms", 100.0, 30.0)]
    assert custom["modules"]["limiter"]["lookahead_ms"] == 5.0
    # already-versioned files are not migrated again
    assert SET.migrate({"settings_version": 2, "modules": {"limiter": {"release_ms": 100.0}}})[1] == []


# ---------------------------------------------------------------- M2
@test
def m2_unique_output_names():
    used = set()
    a = unique_output_path(OUT / "song_master.wav", used)
    b = unique_output_path(OUT / "song_master.wav", used)
    c = unique_output_path(OUT / "song_master.wav", used)
    assert (a.name, b.name, c.name) == ("song_master.wav", "song_master_2.wav", "song_master_3.wav")
    (OUT / "exists_master.wav").write_bytes(b"x")
    assert unique_output_path(OUT / "exists_master.wav", set(), avoid_existing=True).name == "exists_master_2.wav"


# ---------------------------------------------------------------- M3 / M4 / L1 / L9 (CLI)
def cli(*args, bat=False):
    env = dict(os.environ, PYTHONIOENCODING="utf-8")
    if bat:
        cmd = ["cmd", "/c", str(ROOT / "cli.bat"), *args]
    else:
        cmd = [PY, "-m", "mastercraft.cli", *args]
    r = subprocess.run(cmd, cwd=str(ROOT), capture_output=True, text=True, encoding="utf-8", errors="replace", env=env)
    return r.returncode, r.stdout + r.stderr


@test
def m3_m4_corrupt_input_batch_and_exit_codes():
    good = write("good_a.wav", song(48000, 6.0), 48000, subtype="PCM_24")
    bad = OUT / "bad_random.wav"; bad.write_bytes(os.urandom(4096))
    empty = OUT / "bad_empty.wav"; empty.write_bytes(b"")
    st = str(OUT / "cli_settings.json"); Path(st).write_text(json.dumps(SET.defaults()))
    o = str(OUT / "cli_out")
    rc, out = cli(str(bad), str(good), str(empty), "-o", o, "--no-report", "--settings", st, "--quiet")
    assert rc == 1, (rc, out)
    assert "Not an audio file" in out and "Empty file" in out and "Traceback" not in out, out
    assert (Path(o) / "good_a_master.wav").exists()
    rc, out = cli(str(bad), str(empty), "-o", o, "--settings", st)
    assert rc == 3, (rc, out)
    rc, out = cli(str(good), "-p", "nosuchprofile", "--settings", st)
    assert rc == 2 and "unknown profile" in out and "Traceback" not in out, (rc, out)
    rc, out = cli(str(bad), "-o", o, "--settings", st, bat=True)
    assert rc == 3, ("cli.bat must pass the exit code on", rc, out)
    rc, out = cli(str(OUT / "nope.wav"), bat=True)
    assert rc == 2, (rc, out)


@test
def m2_cli_same_stem_two_folders():
    st = str(OUT / "cli_settings.json")
    d1, d2 = OUT / "dupA", OUT / "dupB"
    d1.mkdir(exist_ok=True); d2.mkdir(exist_ok=True)
    write("dupA/song.wav", song(48000, 5.0, 1), 48000)
    write("dupB/song.wav", song(48000, 5.0, 2), 48000)
    o = OUT / "cli_dup"
    shutil.rmtree(o, ignore_errors=True)
    rc, out = cli(str(d1 / "song.wav"), str(d2 / "song.wav"), "-o", str(o), "--no-report", "--settings", st, "--quiet")
    assert rc == 0, out
    assert sorted(p.name for p in o.glob("*.wav")) == ["song_master.wav", "song_master_2.wav"], list(o.iterdir())


@test
def l9_truncated_and_l6_multichannel_warnings():
    p = write("trunc.wav", song(48000, 6.0), 48000, subtype="PCM_16")
    b = p.read_bytes()
    t = OUT / "trunc_cut.wav"; t.write_bytes(b[: len(b) // 5])
    x, sr, w = load_audio_ex(t)
    assert any("truncated" in s for s in w), w
    q = np.stack([np.ones(10), 2 * np.ones(10), 3 * np.ones(10), 4 * np.ones(10)], 1) * 0.1
    y, w = _sanitize(q)   # 4 ch without a mask = quad L R Ls Rs (QA N4)
    g = 0.70710678
    assert y.shape == (10, 2) and np.allclose(y[0], [0.1 + g * 0.3, 0.2 + g * 0.4]) and w, (y[0], w)


@test
def m3_load_error_message():
    bad = OUT / "bad_text.wav"; bad.write_text("not audio at all")
    try:
        load_audio_ex(bad)
    except AudioLoadError as e:
        assert "bad_text.wav" in str(e) and "libsndfile" in str(e) and "CalledProcessError" not in str(e), e
    else:
        raise AssertionError("no error")


# ---------------------------------------------------------------- M5 / L5 / L7
@test
def m5_no_limiter_note():
    s = defaults(profile="pop", mode="loud")
    s["modules"]["limiter"]["enabled"] = False
    y, res = master(song(48000, 8.0), 48000, s)
    assert res["limiter"].get("disabled") and any("Limiter is disabled" in n for n in res["notes"]), res["notes"]


@test
def l5_silent_input():
    y, res = master(np.zeros((48000 * 3, 2)), 48000, defaults())
    assert not res["plan"]["steps"] and any("silent" in n for n in res["notes"]) and not y.any()


@test
def l7_not_reachable_note_uses_real_cap():
    s = defaults(profile="pop", target_lufs=-3.0)
    y, res = master(song(48000, 8.0), 48000, s)
    lim_step = next(st for st in res["plan"]["steps"] if st["module"] == "limiter")
    n = [x for x in res["notes"] if "not reachable" in x or ("capped at" in x and "Make-up" not in x)]
    assert n and f"{lim_step['max_gr_abort_db']:.1f} dB" in n[0], (n, lim_step["max_gr_abort_db"])


# ---------------------------------------------------------------- M6
@test
def m6_mp3_remeasure_and_rate():
    from mastercraft.audio_io import mp3_available
    if not mp3_available():
        return
    x = song(48000, 8.0)
    s = defaults(profile="rock", mode="loud", format="mp3", output_sr=96000)
    y, res = master(x, 48000, s)
    p, exp = finalize_export(y, 48000, OUT / "mp3_96k.mp3", s["general"], res["targets"]["ceiling_dbtp"])
    assert exp["sample_rate"] == sf.info(str(p)).samplerate == 48000, exp
    assert exp["decoded_tp_dbtp"] <= res["targets"]["ceiling_dbtp"] + 0.01, exp
    assert "decoded_lufs" in exp and exp.get("sample_rate_note")


# ---------------------------------------------------------------- M7
@test
def m7_guard_charges_each_stage():
    import mastercraft.engine as E
    f = QA_IN / "mono_rc20.wav"
    x, sr = (load_audio_ex(f)[:2]) if f.exists() else (song(48000, 10.0)[:, :1], 48000)
    s = defaults(profile="rock", mode="loud")
    y, res = master(x, sr, s)
    g = res["punch"]["guard"]
    for r in g:
        if "per_stage_increase_db" in r:
            # stages that are not back-off candidates now carry their own share
            assert any(k not in E._BACKOFF_ORDER for k in r["per_stage_increase_db"]), r
    if res["punch"]["score_db"] > res["punch"]["threshold_db"]:
        assert any("could not get back under" in n for n in res["notes"]), res["notes"]


# ---------------------------------------------------------------- M8
@test
def m8_mud_centre_case_a():
    f = QA_IN / "muddy_rc40.wav"
    if not f.exists():
        return
    x, sr, _ = load_audio_ex(f)
    rep = analyze(x, sr, get_profile("rock"), str(f), fast=True)
    mud = next(i for i in rep["issues"] if i["key"] == "mud")
    fc = mud["details"]["center_hz"]
    assert 300 <= fc <= 450 and fc / 1.6 <= 300, fc


# ---------------------------------------------------------------- GUI preview temp files
@test
def gui_preview_unique_and_cleaned():
    from mastercraft.gui import worker as W
    a = W.Worker._preview(np.zeros((480, 2)), 48000, 0, "before", 0.0)
    b = W.Worker._preview(np.zeros((480, 2)), 48000, 0, "before", 0.0)
    assert a != b and str(os.getpid()) in Path(b).name and not Path(a).exists() and Path(b).exists()
    W.cleanup_previews()
    assert not Path(b).exists()


# ================================================================ v0.1.2 (QA retest N1-N9, M8, L8)
@test
def n9_corrupt_settings_never_overwritten():
    cp = OUT / "n9_settings.json"
    for f in OUT.glob("n9_settings.json*.bad"):
        f.unlink()
    bad_bytes = b'{"general": {"format": "mp3",, }'            # hand-edit typo
    cp.write_bytes(bad_bytes)
    old_file, SET.SETTINGS_FILE = SET.SETTINGS_FILE, cp
    try:
        s = SET.load()
        assert s == SET.defaults() and SET.LOAD_ERROR and SET.AUTOSAVE_BLOCKED, SET.LOAD_ERROR
        assert Path(SET.BACKUP_PATH).name == "n9_settings.json.bad" and Path(SET.BACKUP_PATH).read_bytes() == bad_bytes
        assert SET.save(s) is False and cp.read_bytes() == bad_bytes            # autosave is blocked
        SET.load()                                                            # 2nd failure: timestamped backup
        b2 = Path(SET.BACKUP_PATH)
        assert b2.name != "n9_settings.json.bad" and b2.name.endswith(".bad") and b2.read_bytes() == bad_bytes, b2
        assert SET.save(s, explicit=True) is True and json.loads(cp.read_text(encoding="utf-8"))["general"]
        assert not SET.AUTOSAVE_BLOCKED and SET.save(s) is True                # unblocked after explicit save
        assert (OUT / "n9_settings.json.bad").read_bytes() == bad_bytes         # backup untouched
    finally:
        SET.SETTINGS_FILE = old_file
        SET.AUTOSAVE_BLOCKED = False


@test
def n9_gui_warns_and_does_not_autosave():
    from PySide6.QtWidgets import QApplication
    from mastercraft.gui.main_window import MainWindow
    cp = OUT / "n9_gui_settings.json"
    cp.write_bytes(b"not json")
    old_file, SET.SETTINGS_FILE = SET.SETTINGS_FILE, cp
    try:
        app = QApplication.instance() or QApplication([])
        SET.load()
        w = MainWindow()
        assert "unreadable" in w._settings_banner.text()
        w.panel.changed.emit()          # whatever the panel autosaves on
        app.processEvents()
        w.close(); app.processEvents()  # closeEvent autosaves
        assert cp.read_bytes() == b"not json"
    finally:
        SET.SETTINGS_FILE = old_file
        SET.AUTOSAVE_BLOCKED = False


@test
def n2_makeup_gain_capped_on_near_silence():
    rng = np.random.default_rng(5)
    x = 10 ** (-75 / 20) * rng.standard_normal((48000 * 6, 2))   # hiss only, about -75 LUFS
    lin = LN.integrated_lufs(x, 48000)
    y, res = master(x, 48000, defaults(profile="rock"))
    out = res["output"]["metrics"]["integrated_lufs"]
    assert out <= lin + 18.5, (lin, out)
    assert res["targets"].get("gain_cap") and any("capped at +18" in n for n in res["notes"]), res["notes"]
    y, res = master(song(48000, 6.0), 48000, defaults(profile="rock"))
    assert not res["targets"].get("gain_cap")


@test
def n7_mp3_margin_and_loudness():
    from mastercraft.audio_io import mp3_available
    from mastercraft.postprocess import export_notes, MP3_TP_MARGIN_DB
    if not mp3_available():
        return
    for prof, mode in (("rock", "loud"), ("metal", "loud"), ("neutral", "streaming")):
        x = song(48000, 10.0, 7)
        s = defaults(profile=prof, mode=mode, format="mp3")
        y, res = master(x, 48000, s)
        p, exp = finalize_export(y, 48000, OUT / f"n7_{prof}.mp3", s["general"], res["targets"]["ceiling_dbtp"])
        c = res["targets"]["ceiling_dbtp"]
        assert exp["decoded_tp_dbtp"] <= c - MP3_TP_MARGIN_DB + 0.02, exp
        miss = exp["decoded_lufs"] - res["targets"]["lufs"]
        notes = export_notes(exp, res["targets"])
        assert abs(miss) <= 0.2 or any("LU from the" in n for n in notes), (exp, notes)
        lost = exp["decoded_lufs"] - res["output"]["metrics"]["integrated_lufs"]   # encode + fix cost
        assert abs(lost) <= 0.2, (lost, exp)   # was 0.15: the denser low-end punch master costs ~0.15 dB


@test
def n4_layout_aware_fold():
    from mastercraft.audio_io import fold_matrix
    g = 0.70710678
    n = 48000
    one = 0.1 * np.ones(n)
    def ch(k, total):
        z = np.zeros((n, total)); z[:, k] = one; return z
    y, w = _sanitize(ch(2, 3))                       # 3.0: centre -> both sides at -3 dB
    assert np.allclose(y[0], [0.1 * g, 0.1 * g]) and "3.0" in w[0], (y[0], w)
    for k, want in ((2, [g, g]), (3, [g, g]), (4, [g, 0]), (5, [0, g]), (0, [1, 0]), (1, [0, 1])):
        y, w = _sanitize(ch(k, 6))                   # 5.1: L R C LFE Ls Rs
        assert np.allclose(y[0], 0.1 * np.array(want)), (k, y[0])
    m, d, known = fold_matrix(8)                     # 7.1: FL FR FC LFE BL BR SL SR
    assert known and np.allclose(m[6], [g, 0]) and np.allclose(m[7], [0, g]), m
    m, d, known = fold_matrix(6, 0x60F)              # 5.1 (side) from a WAVE_FORMAT_EXTENSIBLE mask
    assert known and np.allclose(m[4], [g, 0]) and "side" in d
    y, w = _sanitize(np.ones((10, 11)) * 0.1)        # unknown layout: equal power
    assert not fold_matrix(11)[2] and "equal-power" in w[0] and np.allclose(y[0, 0], 0.1 * 6 * np.sqrt(2 / 11)), (y[0], w)
    # mask read from a real WAVEX file written by libsndfile
    p = write("n4_51.wav", ch(2, 6), 48000, subtype="PCM_16")
    x, sr, w = load_audio_ex(p)
    assert abs(x[100, 0] - x[100, 1]) < 1e-4 and x[100, 0] > 0.06 and any("5.1" in s for s in w), (x[100], w)


@test
def n1_missing_file_in_batch_reported():
    good = write("n1_good.wav", song(48000, 3.0), 48000)
    st = str(OUT / "cli_settings.json")
    rc, out = cli(str(OUT / "does_not_exist.wav"), str(good), "--analyze-only", "-o", str(OUT / "n1"), "--settings", st, "--quiet")
    assert rc == 1 and "File not found" in out and "does_not_exist.wav" in out and "1 OK, 1 failed" in out, (rc, out)
    rc, out = cli(str(OUT / "does_not_exist.wav"), str(OUT / "nope2.wav"), "--settings", st)
    assert rc == 2 and "does_not_exist.wav" in out, (rc, out)


@test
def n3_report_follows_unique_names():
    from mastercraft.audio_io import report_path_for
    assert report_path_for(Path("a/song_master.wav")).name == "song_master.report.json"
    assert report_path_for(Path("a/song_master.mp3")).name == "song_master_mp3.report.json"
    d = OUT / "n3"; shutil.rmtree(d, ignore_errors=True); d.mkdir()
    (d / "song_master.report.json").write_text("{}")      # orphan report from an earlier run
    assert unique_output_path(d / "song_master.wav", set(), avoid_existing=True).name == "song_master_2.wav"
    good = write("n3_in.wav", song(48000, 4.0), 48000)
    st = OUT / "n3_settings.json"; st.write_text(json.dumps(SET.defaults()))
    for fmt in ("wav24", "mp3"):
        rc, out = cli(str(good), "-o", str(d), "--settings", str(st), "--quiet", "-f", fmt)
        assert rc == 0, out
    reps = sorted(p.name for p in d.glob("n3_in*.report.json"))
    assert reps == ["n3_in_master.report.json", "n3_in_master_mp3.report.json"], reps
    assert json.loads((d / "n3_in_master_mp3.report.json").read_text())["export"].get("decoded_lufs") is not None


@test
def n5_load_error_text_and_quiet_stderr():
    bad = OUT / "random_bytes.wav"; bad.write_bytes(np.random.default_rng(1).bytes(8192))
    try:
        load_audio_ex(bad)
        raise AssertionError("no error")
    except AudioLoadError as e:
        assert "does not exist" not in str(e) and "unrecognised file header" in str(e), e
    fake = OUT / "garbage.mp3"; fake.write_bytes(b"ID3\x03\x00\x00\x00\x00\x00\x00" + np.random.default_rng(2).bytes(20000))
    st = str(OUT / "cli_settings.json")
    for f in (bad, fake):
        rc, out = cli(str(f), "--analyze-only", "-o", str(OUT / "n5"), "--settings", st)
        assert rc == 3 and "FAILED" in out and "mpg123" not in out and "vcpkg" not in out and "MPEG header" not in out, out


@test
def n6_no_runtime_warning_on_hiss():
    import warnings
    rng = np.random.default_rng(9)
    x = 0.05 * rng.standard_normal((48000 * 4, 2))
    with warnings.catch_warnings():
        warnings.simplefilter("error", RuntimeWarning)
        h = P.detect_hits(x, 48000); m = P.hit_metrics(x, 48000, h)
        s = P.summary(m); P.compare(m, m, h, 48000)
    assert "NaN" not in json.dumps(s)


@test
def n8_no_punch_warning_on_short_files_and_stage_advice():
    import mastercraft.engine as E
    x = song(48000, 2.0)
    for prof in ("rock", "edm", "metal"):
        y, res = master(x, 48000, defaults(profile=prof, mode="loud"))
        assert not any(n.startswith("WARNING") and "punch" in n.lower() for n in res["notes"]), res["notes"]
    assert set(E._STAGE_ADVICE) >= {"limiter", "static_eq", "glue", "multiband", "saturation"}
    assert E._STAGE_ADVICE["limiter"] != E._STAGE_ADVICE["glue"]


@test
def l8_tab_moves_focus_in_panel_and_queue():
    from PySide6.QtWidgets import QApplication, QSlider, QComboBox
    from mastercraft.gui.main_window import MainWindow
    cp = OUT / "l8_settings.json"; cp.write_text(json.dumps(SET.defaults()))
    old_file, SET.SETTINGS_FILE = SET.SETTINGS_FILE, cp
    try:
        app = QApplication.instance() or QApplication([])
        SET.load()
        w = MainWindow(); w.show(); app.processEvents()
        n = {"t": 0}
        w.player.toggle = lambda: n.__setitem__("t", n["t"] + 1)
        for wid in (w.panel.findChild(QSlider), w.panel.findChild(QComboBox), w.queue):
            wid.setFocus(); app.processEvents()
            assert QApplication.focusWidget() is wid, wid
            w._tab_key(); app.processEvents()
            assert QApplication.focusWidget() is not wid and n["t"] == 0, (wid, QApplication.focusWidget())
        w.btn_play.setFocus(); app.processEvents()
        w._tab_key()
        assert n["t"] == 1          # outside panel/queue Tab is still A/B
        w.close(); app.processEvents()
    finally:
        SET.SETTINGS_FILE = old_file


# ================================================================ Manual EQ (0.2.0)
@test
def eq_flat_is_transparent_and_chunk_invariant():
    from mastercraft import manual_eq as M
    x = song(48000, 4.0) * 0.6
    y = M.render(x, 48000, M.default_state(), 0.0, -1.0)
    assert np.max(np.abs(y - x)) < 1e-9, np.max(np.abs(y - x))
    st = M.default_state(); st["bands"][2].update(gain=9.0); st["bands"][6].update(gain=6.0); st["bands"][0].update(type="low_cut", freq=40.0)
    a = M.render(x, 48000, st, 3.0, -1.0)
    pr = M.Processor(48000, 2, -1.0); pr.set_state(st, 3.0, snap=True); pr.reset()
    xp = np.vstack([x, np.zeros((pr.D, 2))]); out, i = [], 0
    for b in (1, 7, 333, 480, 1024, 4096) * 200:
        if i >= len(xp):
            break
        out.append(pr.process(xp[i:i + b])); i += b
    b2 = np.vstack(out)[pr.D:pr.D + len(x)]
    assert np.max(np.abs(a - b2)) == 0.0
    assert 20 * np.log10(LN.true_peak(a, 48000, 4)) <= -1.0 + 0.02


@test
def eq_hold_loudness_and_presets():
    from mastercraft import manual_eq as M
    x = song(48000, 8.0)
    hold = LN.integrated_lufs(x, 48000)
    st = M.apply_macro(M.default_state(), "Brightness / Air")
    assert st["bands"][6]["gain"] == 2.5 and st["bands"][6]["type"] == "high_shelf"
    st["bands"][1]["gain"] = 6.0
    y, info = M.apply(x, 48000, st, hold, -1.0)
    assert abs(info["lufs"] - hold) <= 0.1 and info["true_peak_dbtp"] <= -0.98, info
    p = M.save_preset(st, OUT / "eq_preset.json")
    assert M.load_preset(p) == M.clean_state(st)
    assert M.clean_state({"bands": [{"type": "bogus", "gain": 99, "freq": -5, "q": "x"}]})["bands"][0] == \
        {"type": "low_shelf", "freq": 20.0, "gain": 18.0, "q": 0.707, "on": True}


@test
def eq_cli_preset():
    from mastercraft import manual_eq as M
    st = M.default_state(); st["bands"][4]["gain"] = 4.0
    pp = M.save_preset(st, OUT / "cli_eq.json")
    good = write("eq_cli_in.wav", song(48000, 5.0), 48000)
    st_file = OUT / "eq_cli_settings.json"; st_file.write_text(json.dumps(SET.defaults()))
    d = OUT / "eq_cli"; shutil.rmtree(d, ignore_errors=True)
    rc, out = cli(str(good), "-o", str(d), "--settings", str(st_file), "--eq-preset", str(pp))
    assert rc == 0 and "Manual EQ preset applied" in out, out
    r = json.loads((d / "eq_cli_in_master.report.json").read_text())
    assert abs(r["manual_eq"]["lufs"] - r["output"]["metrics"]["integrated_lufs"]) <= 0.1, r["manual_eq"]
    # QA v0.2.0 #5: out: line and report are post-EQ (the auto-chain values are kept separately)
    assert "metrics_before_manual_eq" in r["output"]
    y, _ = sf.read(str(d / "eq_cli_in_master.wav"), always_2d=True)
    assert abs(LN.integrated_lufs(y, 48000) - r["output"]["metrics"]["integrated_lufs"]) <= 0.05
    tp_file = 20 * np.log10(LN.true_peak(y, 48000, 4))
    assert abs(tp_file - r["output"]["metrics"]["true_peak_dbtp"]) <= 0.1, (tp_file, r["output"]["metrics"])
    line = next(l for l in out.splitlines() if l.strip().startswith("out:"))
    assert f"{r['output']['metrics']['integrated_lufs']:6.2f}" in line, (line, r["output"]["metrics"])


def _bell(x, sr, f0, gdb, q=0.9):
    from scipy.signal import lfilter
    A = 10 ** (gdb / 40); w = 2 * np.pi * f0 / sr; al = np.sin(w) / (2 * q)
    b = [1 + al * A, -2 * np.cos(w), 1 - al * A]; a = [1 + al / A, -2 * np.cos(w), 1 - al / A]
    return lfilter(np.array(b) / a[0], np.array(a) / a[0], x, axis=0)


@test
def m8_mud_detection_tracks_bell_frequency():
    f = QA_IN / "rc_excerpt40.wav"
    if not f.exists():
        return
    x, sr, _ = load_audio_ex(f)
    prof = get_profile("rock")
    def mud(y):
        rep = analyze(y, sr, prof, "", fast=True)
        return next(i for i in rep["issues"] if i["key"] == "mud")
    clean = mud(x)
    assert clean["severity"] < 30, clean
    for f0 in (200, 400, 500):
        m = mud(_bell(x, sr, f0, 9.0))
        fc = m["details"]["center_hz"]
        assert m["severity"] >= 40 and f0 / 1.45 <= fc <= f0 * 1.45, (f0, m["severity"], fc)


# ================================================================ v0.2.1 (QA v0.2.0)
def _overshoot_mod():
    import importlib.util
    spec = importlib.util.spec_from_file_location("overshoot_test", ROOT / "test" / "eq" / "overshoot_test.py")
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m); return m


@test
def eq_transitions_never_overshoot():
    """Every QA trigger (A/B, preset, undo, macro, type change, bypass, Play after editing while
    stopped, exact-gain update): output never more than 1 dB above the louder steady state."""
    from mastercraft import manual_eq as M
    OS = _overshoot_mod()
    x = song(48000, 4.0, seed=4)
    res = OS.run(M, x, 48000, M.LoudnessModel(x, 48000), True)
    worst = max(res, key=lambda r: r["overshoot_db"])
    assert worst["overshoot_db"] <= 1.0 and all(r["ms_above_1dB"] == 0 for r in res), worst
    # the hard property behind it: a crossfade sample lies between the two chains' samples
    pr = M.Processor(48000, 2, 0.0); pr.set_state(M.default_state(), 0.0, snap=True); pr.reset()
    st = M.default_state(); [b.update(gain=18.0) for b in st["bands"]]
    pr.process(x[:4800]); pr.set_state(st, 6.0); pr.process(x[4800:9600])
    assert pr.transitions == 1
    # v0.2.2: warm-up / fade scale with the slowest pole (tau ~ Q/(pi f)); low-freq high-Q cases are in OS.TRIGGERS
    lc = M.default_state(); lc["bands"][0].update(type="low_cut", freq=40.0, q=18.0)
    tau = M.Chain(lc, 0.0, 48000, 2).tau
    assert 0.5 * 18 / (math.pi * 40) < tau < 2 * 18 / (math.pi * 40), tau
    names = {(r["from"], r["to"]) for r in res}
    assert ("flat", "lowcut 40 Q18") in names and ("flat", "OUT +12") in names


@test
def eq_silent_master_is_finite_and_json_valid():
    from mastercraft import manual_eq as M
    from mastercraft.cli import dump_json
    x = np.zeros((48000 * 3, 2))
    lm = M.LoudnessModel(x, 48000)
    assert lm.silent and lm.delta_db(M.default_state()) == 0.0
    st = M.default_state(); st["bands"][3]["gain"] = 12.0
    r = M.solve_gain_ex(x, 48000, st, float("-inf"), -1.0, lm)
    assert r["gain_db"] == 0.0 and r["note"] and "silent" in r["note"], r
    y, info = M.apply(x, 48000, st, float("-inf"), -1.0)
    assert np.isfinite(y).all() and np.max(np.abs(y)) == 0.0
    p = OUT / "silent_eq.json"
    dump_json({"manual_eq": info, "nan": float("nan"), "inf": np.float64("inf"), "arr": np.array([1.0, np.nan])}, p)
    d = json.loads(p.read_text(), parse_constant=lambda c: (_ for _ in ()).throw(ValueError(c)))
    assert d["nan"] is None and d["inf"] is None and d["arr"] == [1.0, None]
    pr = M.Processor(48000, 2, -1.0); pr.set_state(st, float("inf"), snap=True); pr.reset()
    assert np.isfinite(pr.process(x[:4800])).all() and abs(pr.gain_db) <= M.HOLD_CAP_DB
    xx = x[:4800].copy(); xx[100] = np.nan
    assert np.isfinite(pr.process(xx)).all()


@test
def eq_panel_silent_song_offline():
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    from mastercraft.gui.eq_panel import ManualEQPanel
    pan = ManualEQPanel()
    pan.load_song(np.zeros((48000 * 2, 2), np.float32), 48000, None, float("-inf"), -1.0, {})
    st = pan.st; st["bands"][2]["gain"] = 9.0; pan._changed()
    assert "silent" in pan.info.text().lower(), pan.info.text()
    pan.stream.start_at(0)
    b = pan.stream.readData(4096 * 8)
    assert np.isfinite(np.frombuffer(b, np.float32)).all()


@test
def eq_bom_bools_and_macros():
    from mastercraft import manual_eq as M
    p = OUT / "bom_preset.json"
    d = M.default_state(); d["bands"][2].update(gain=4.0, on="false"); d["bypass"] = "0"; d["hold_loudness"] = "off"
    p.write_bytes(b"\xef\xbb\xbf" + json.dumps(d).encode())
    st = M.load_preset(p)
    assert st["bands"][2]["on"] is False and st["bypass"] is False and st["hold_loudness"] is False and st["bands"][2]["gain"] == 4.0
    assert M.clean_state({"bands": [{"on": "yes"}, {"on": 0}, {"on": "garbage"}]})["bands"][:3] and \
        [b["on"] for b in M.clean_state({"bands": [{"on": "yes"}, {"on": 0}, {"on": "garbage"}]})["bands"][:3]] == [True, False, True]
    s = SET.defaults(); sp = OUT / "bom_settings.json"
    sp.write_bytes(b"\xef\xbb\xbf" + json.dumps(s).encode())
    assert SET.load_file(sp)["general"]["profile"] == s["general"]["profile"]
    k = list(M.MACROS)[0]
    a1 = M.apply_macro(M.default_state(), k); a2 = M.apply_macro(a1, k)
    assert a1 == a2 and M.macro_active(a2, k)


@test
def eq_solver_iterates_and_notes():
    from mastercraft import manual_eq as M
    x = song(48000, 6.0, seed=5)
    hold = LN.integrated_lufs(x, 48000)
    st = M.default_state(); [b.update(gain=18.0) for b in st["bands"]]
    r = M.solve_gain_ex(x, 48000, st, hold, -1.0)
    assert r["converged"] or (r["note"] and "converge" in r["note"]), r
    if r["converged"]:
        assert abs(r["lufs"] - hold) <= 0.1, r
    mono = song(48000, 6.0, seed=6)[:, :1] * np.ones((1, 2))
    st2 = M.default_state(); st2["bands"][5]["gain"] = 12.0; st2["bands"][1]["gain"] = 9.0
    r2 = M.solve_gain_ex(mono, 48000, st2, LN.integrated_lufs(mono, 48000) + 4.0, -1.0)    # loud: guard-limited
    assert r2["iterations"] >= 1 and (r2["converged"] or r2["note"]), r2


@test
def eq_master_cache_on_disk():
    from mastercraft.gui import worker as W
    from mastercraft import paths as PA
    y = song(48000, 2.0).astype(np.float32)
    p = W.cache_master(y, 991)
    assert Path(p).exists() and Path(p).parent == PA.EQ_CACHE_DIR
    assert np.array_equal(W.load_cached_master(p), y)
    p2 = W.cache_master(y, 991)
    assert not Path(p).exists() and Path(p2).exists()      # superseded file removed
    W.cleanup_masters()
    assert not Path(p2).exists()
    assert PA.DATA_DIR == OUT / "data_dir" and PA.EQ_PRESETS_DIR.parent == PA.DATA_DIR    # env override (tests)
    r = subprocess.run([PY, "-c", "from mastercraft import paths; print(paths.DATA_DIR)"], cwd=str(ROOT), capture_output=True,
                       text=True, env={k: v for k, v in os.environ.items() if k != "MASTERCRAFT_DATA_DIR"})
    dflt = Path(r.stdout.strip())
    assert dflt == PA.APP_DIR.parent / (PA.APP_DIR.name + "-data") and PA.APP_DIR not in dflt.parents, dflt


@test
def eq_preset_dir_fallback():
    """v0.2.2: presets folder chain (env, sibling data folder, app folder, %APPDATA% last and only
    with the user's OK). Only paths under test\\ are used; nothing is written to the system drive."""
    from mastercraft import paths as PA
    base = OUT / "preset_fallback"; base.mkdir(parents=True, exist_ok=True)
    blocker = base / "not_a_dir.txt"; blocker.write_text("x")          # a file: mkdir under it fails
    bad = blocker / "eq_presets"; good = base / "app_user_data" / "eq_presets"; last = base / "fake_appdata" / "eq_presets"
    shutil.rmtree(last.parent, ignore_errors=True)   # the last case below creates it: keep the test re-runnable
    shutil.rmtree(good.parent, ignore_errors=True)
    d, note = PA.resolve_preset_dir([("env", bad, False), ("app", good, False), ("appdata", last, True)])
    assert d == good and "not writable" in note and str(bad) in note and str(good) in note, note
    d, note = PA.resolve_preset_dir([("env", good, False)])
    assert d == good and note == ""
    asked = []
    d, note = PA.resolve_preset_dir([("env", bad, False), ("appdata", last, True)], lambda p: asked.append(p) or False)
    assert d is None and asked == [last] and "No writable" in note and not last.exists()
    d, note = PA.resolve_preset_dir([("env", bad, False), ("appdata", last, True)], lambda p: True)
    assert d == last and "fallback" in note
    # default chain order (no probing): env first, APPDATA last and flagged
    c = PA.preset_dir_candidates()
    assert c[0][1] == PA.EQ_PRESETS_DIR and c[-1][2] == bool(os.environ.get("APPDATA"))
    assert [x[2] for x in c[:-1]] == [False] * (len(c) - 1)
    # panel: clear notice in the info line, no dialogs with ask=
    from PySide6.QtWidgets import QApplication
    app = QApplication.instance() or QApplication([])
    from mastercraft.gui.eq_panel import ManualEQPanel
    pan = ManualEQPanel()
    assert pan.preset_dir([("env", bad, False), ("app", good, False)], ask=lambda p: False) == good
    assert "Using fallback" in pan.info.text()


# ================================================================ v0.3 (low-end punch, tilt, blends)
@test
def x1_low_punch_adds_attack_and_keeps_level():
    """The low-end punch stage must raise a kick's attack-vs-tail and leave the band's level alone."""
    from mastercraft.dsp import lowend as LE
    from mastercraft.dsp.filters import band_zero_phase
    sr, bpm = 48000, 120.0
    n = int(8.0 * sr); t = np.arange(n) / sr
    x = 0.18 * np.sin(2 * np.pi * 110 * t)                      # steady bass bed
    beat = 60.0 / bpm
    for k in range(int(8.0 / beat)):
        i = int(k * beat * sr); m = min(n - i, int(0.4 * sr)); tt = np.arange(m) / sr
        x[i:i + m] += 0.7 * np.sin(2 * np.pi * 58 * tt) * np.exp(-tt / 0.06)
    x = np.stack([x, x], 1); x /= np.max(np.abs(x)) / 0.7

    def slap(lo=45.0, hi=170.0):
        b = band_zero_phase(x if slap.y is None else slap.y, lo, hi, sr).mean(axis=1)
        v = []
        for i in range(0, len(b) - int(0.45 * sr), int(0.5 * sr)):
            seg = b[i:i + int(0.2 * sr)]
            pk = np.max(np.abs(seg[: int(0.015 * sr)]))
            sus = np.sqrt(np.mean(seg[int(0.03 * sr): int(0.12 * sr)] ** 2)) + 1e-12
            v.append(20 * np.log10(pk / sus + 1e-12))
        return float(np.median(v))
    slap.y = None
    a = slap()
    slap.y, info = LE.transient_shape(x, sr, 45.0, 170.0, 3.0, 2.0)
    b = slap()
    assert b - a >= 1.2, (a, b, info)                      # a real attack/tail change, not a gain change
    assert info["attack_applied_db"] >= 2.0 and info["sustain_applied_db"] >= 1.5, info
    # the stage is a passthrough where the masks are zero (no level drift on the bed alone)
    bed = np.stack([0.1 * np.sin(2 * np.pi * 110 * t)] * 2, 1)
    yb, _ = LE.transient_shape(bed, sr, 45.0, 170.0, 3.0, 2.0)
    assert abs(20 * np.log10(np.sqrt(np.mean(yb ** 2)) / np.sqrt(np.mean(bed ** 2)))) < 1.5


@test
def x2_low_mid_guard_keeps_the_kick_body():
    """A deep mud cut must not hollow 100-160 Hz: the guard trims the low cuts and the cascade
    ends inside the floors (the old plans took ~2.9 dB out of 160 Hz while lifting the sub)."""
    from mastercraft import engine as E
    sr = 48000
    f1 = [("bell", 163.4, -3.05, 1.05, "mud score 9"), ("low_shelf", 90.0, 1.5, 0.7, "bass deficit")]
    assert E._guard_low_mid(list(f1), sr)[1], "the old RuneCore plan must trip the guard"
    f2 = [("bell", 160.0, -5.0, 1.0, "mud"), ("bell", 105.0, 1.5, 0.8, "deficit")]
    out, actions, resp = E._guard_low_mid(list(f2), sr)
    assert actions and all(0.5 <= a["scale"] <= 1.0 for a in actions), actions
    assert resp["125"] >= -2.05 and resp["160"] >= -2.65, resp
    assert abs(out[0][2]) < 5.0, out                                     # the cut was trimmed
    assert all(len(f) > 4 and f[4] for f in out), out                    # reasons survive the guard


@test
def x3_tilt_first_and_span_cap():
    """A sloped spectrum is fixed with two shelves before the bells, and the overall correction
    never swings more than 6 dB peak-to-peak (research2 1.4 / 3.1 step 7)."""
    from mastercraft import engine as E
    from mastercraft.profiles import get_profile
    x = song(48000, 10.0, 11)
    rep = analyze(x, 48000, get_profile("neutral"), "", fast=True)
    # force a strong tilt on the octave curve
    dev = np.array(rep["_internal"]["dev"], float)
    dev += np.linspace(-3.0, 3.0, dev.size)
    rep["_internal"]["dev"] = dev
    pl = E.plan(rep, get_profile("neutral"), defaults(profile="neutral", mode="streaming_safe"))
    st = next((s for s in pl["steps"] if s["module"] == "static_eq"), None)
    assert st, [s["module"] for s in pl["steps"]]
    assert abs(st["tilt"]["slope_db_per_oct"]) > 0.75 and st["tilt"]["applied_db"] != 0.0, st["tilt"]
    types = [f["type"] for f in st["filters"]]
    assert "low_shelf" in types and "high_shelf" in types, types
    assert all(f["why"] for f in st["filters"]), st["filters"]
    assert st["correction_span_db"] <= 6.05, st["correction_span_db"]


@test
def x4_profile_blends_and_aliases():
    from mastercraft import profiles as PR
    assert len(PR.BUILTIN_ORDER) >= 16 and set(PR.BUILTIN_ORDER) <= set(PR.BUILTIN), PR.BUILTIN_ORDER
    assert PR.resolve_key("alt rock") == "alt_rock" and PR.resolve_key("game") == "game_ost"
    items = PR.parse_combo("metal:0.6+edm:0.4")
    assert abs(sum(w for _, w in items) - 1.0) < 1e-9 and [k for k, _ in items] == ["metal", "edm"], items
    p = PR.get_profile("metal:0.6+edm:0.4")
    assert p["key"].startswith(PR.COMBO_PREFIX) and p["display_name"].startswith("Blend: ")
    metal, edm = PR.get_profile("metal"), PR.get_profile("edm")
    assert min(metal["tonal_db"][1], edm["tonal_db"][1]) - 0.1 <= p["tonal_db"][1] <= max(metal["tonal_db"][1], edm["tonal_db"][1]) + 0.1
    assert p["punch"]["max_low_shelf_boost_db"] <= metal["punch"]["max_low_shelf_boost_db"] + 1e-9   # conservative
    assert p["lufs"]["loud"] <= -8.5 and p["lufs"]["loud"] >= -10.0
    # a curve that IS one profile gets no blend suggestion; a curve between two gets them back
    assert PR.suggest_blend(np.array(metal["tonal_db"], float)) == ([], None)
    mid = 0.6 * np.array(metal["tonal_db"], float) + 0.4 * np.array(edm["tonal_db"], float)
    spec, res = PR.suggest_blend(mid)
    assert spec and {(k, round(w, 1)) for k, w in spec} == {("metal", 0.6), ("edm", 0.4)}, (spec, res)
    try:
        PR.parse_combo("metal:0.6")
        assert False, "a single-profile combo must be rejected"
    except KeyError:
        pass


@test
def x5_low_punch_stage_in_plan_and_chain():
    """The stage is planned with a reason, runs in the chain, and stays within its caps."""
    from mastercraft import engine as E
    x = song(48000, 10.0, 5)
    s = defaults(profile="metal", mode="loud")
    y, res = master(x, 48000, s)
    st = next((q for q in res["plan"]["steps"] if q["module"] == "low_punch"), None)
    assert st, [q["module"] for q in res["plan"]["steps"]]
    assert 0 < st["attack_db"] <= 3.0 + 1e-9 and 0 <= st["sub_duck_db"] <= 2.5 + 1e-9, st
    log = next((l for l in res["log"] if l["module"] == "low_punch"), None)
    assert log and log["info"]["attack_applied_db"] >= 0.0, log
    assert np.all(np.isfinite(y)) and np.max(np.abs(y)) <= 1.0


# ================================================================ v0.3 reference matching
@test
def y1_reference_self_match_is_transparent():
    """Matching a song to itself must be numerically transparent (the null test for the matcher)."""
    from mastercraft import reference as RF
    x = song(48000, 8.0, 9)
    ref = RF.target(x, 48000, "self")
    gain = RF.match_gain_db((ref["freqs"], ref["db"]), (ref["freqs"], ref["db"]))
    assert float(np.max(np.abs(gain))) <= 0.05, float(np.max(np.abs(gain)))
    y, info = RF.apply(x, 48000, ref["freqs"], gain)
    assert np.max(np.abs(y - x)) < 1e-6, np.max(np.abs(y - x))          # nothing audible happens
    bands = [125, 500, 2000, 8000]
    assert all(abs(v) <= 0.05 for v in RF.residual_bands(y, 48000, ref, bands).values())


@test
def y2_reference_match_moves_the_master_onto_the_reference():
    """A dark input mastered against a brighter reference ends up close to it, without wrecking punch
    and without the profile's own tonal EQ fighting the match."""
    from mastercraft import reference as RF
    from mastercraft.dsp import filters as F
    x = song(48000, 10.0, 12)
    dark = F.apply_sos(x, F.shelf(1200.0, -4.0, 48000, "high", 0.7))     # dull "mix"
    ref_src = F.apply_sos(x, F.shelf(1200.0, +2.0, 48000, "high", 0.7))  # brighter reference
    ref = RF.target(ref_src, 48000, "bright.wav")
    bands = [63, 125, 250, 500, 1000, 2000, 4000, 8000]
    before = RF.residual_bands(dark, 48000, ref, bands)
    s = defaults(profile="neutral", mode="streaming_safe")
    s["safety"]["punch_check"] = False          # this test is about tone, not punch
    y, res = master(dark, 48000, s, reference=ref)
    after = res["reference"]["residual_after"]
    rms = lambda d: float(np.sqrt(np.mean([v ** 2 for v in d.values()])))   # noqa: E731
    assert rms(after) < 0.6 * rms(before), (before, after)
    mods = [st["module"] for st in res["plan"]["steps"]]
    assert "reference_match" in mods, mods
    se = next((st for st in res["plan"]["steps"] if st["module"] == "static_eq"), {})
    assert all("band" not in (f.get("why") or "") for f in se.get("filters", [])), se   # no profile tone EQ
    assert res["profile"].endswith("bright.wav'"), res["profile"]
    assert np.max(np.abs(y)) <= 1.0 and np.all(np.isfinite(y))


@test
def y3_reference_match_is_bounded_and_keeps_the_ceiling():
    """The match never asks for more than max_db, never boosts above the codec cutoff, and the
    realisation stays a small minimum-phase chain (no giant FIR, no high-Q bells)."""
    from mastercraft import reference as RF
    from mastercraft.dsp import filters as F
    x = song(48000, 10.0, 13)
    bright = F.apply_sos(song(48000, 10.0, 14), F.shelf(800.0, +5.0, 48000, "high", 0.6))
    ref = RF.target(bright, 48000, "bright.wav")
    g_all = RF.match_gain_db((ref["freqs"], ref["db"]), (ref["freqs"], np.full(ref["freqs"].size, 40.0)), max_db=6.0)
    assert float(np.max(np.abs(g_all))) <= 6.0 + 1e-9
    assert all(g == 0.0 for f, g in zip(ref["freqs"], g_all) if f < 30.0)          # no sub-boost
    g_cut = RF.match_gain_db((ref["freqs"], ref["db"]), (ref["freqs"], np.full(ref["freqs"].size, 40.0)),
                             max_db=6.0, cutoff_hz=12000.0)
    assert all(g == 0.0 for f, g in zip(ref["freqs"], g_cut) if f > 11800.0), "no boost above the cutoff"
    chain, info = RF.design_chain(ref["freqs"], g_all, 48000)
    assert info["filters"] <= 7 and all(abs(g) <= 6.0 + 1e-9 for _f, g, _q in chain), chain
    assert all(q <= 1.4 + 1e-9 for _f, _g, q in chain), chain
    s = defaults(profile="neutral", mode="streaming_safe")
    y, res = master(x, 48000, s, reference=ref)
    assert np.max(np.abs(y)) <= 1.0 and np.all(np.isfinite(y))
    assert res["output"]["metrics"]["true_peak_dbtp"] <= res["targets"]["ceiling_dbtp"] + 0.02
    miss = res["output"]["metrics"]["integrated_lufs"] - res["targets"]["lufs"]
    assert abs(miss) <= 0.5 or any("LUFS" in n for n in res["notes"]), (miss, res["notes"])


# ================================================================ v0.3 multi-format export
@test
def z1_multi_format_export_writes_distinct_files():
    """One master -> several files, each with its own name, label and export block (no overwriting)."""
    from mastercraft import postprocess as PP
    from mastercraft.audio_io import mp3_available
    y = song(48000, 2.0, 3)
    d = OUT / "multi_fmt"
    shutil.rmtree(d, ignore_errors=True)
    d.mkdir(parents=True, exist_ok=True)
    gen = dict(defaults()["general"])
    want = ["wav24", "wav16", "flac24"] + (["mp3"] if mp3_available() else [])
    gen["formats"] = want
    gen["output_sr"] = 0
    exports = PP.finalize_exports(y, 48000, d, "song", "_master", gen, -1.0)
    names = [Path(p).name for p, _e in exports]
    expect = {"wav24": "song_master.wav", "wav16": "song_master_16bit.wav",
              "flac24": "song_master.flac", "mp3": "song_master.mp3"}
    assert names == [expect[k] for k in want], names
    assert len({e["path"] for _p, e in exports}) == len(want)
    for p, e in exports:
        assert Path(p).exists() and Path(p).stat().st_size > 1000, p
        assert e["label"] and e["format"] in want, e
    # legacy single-format key keeps working, with the tag on an unambiguous name
    e1 = PP.finalize_exports(y, 48000, d, "one", "", {"format": "wav16", "output_sr": 0}, -1.0)
    assert Path(e1[0][0]).name == "one_16bit.wav", e1[0][0]
    # the format list is validated: unknown entries drop out, order follows the table
    assert PP.export_formats({"formats": ["mp3", "wav24", "nope"]}) == ["wav24", "mp3"]
    assert PP.export_formats({"formats": []}) == ["wav24"]
    assert PP.export_formats({"format": "flac16"}) == ["flac16"]


@test
def z2_cli_multi_format_flags():
    """-f takes a comma list, writes one file per format with a report each, and rejects typos."""
    from mastercraft.audio_io import mp3_available
    p = write("mf_song.wav", song(48000, 2.0, 5), 48000)
    d = OUT / "multi_cli"
    shutil.rmtree(d, ignore_errors=True)
    d.mkdir(parents=True, exist_ok=True)
    fmts = "wav24,flac24" + (",mp3" if mp3_available() else "")
    rc, out = cli(str(p), "-p", "neutral", "-f", fmts, "-o", str(d), "--no-safety")
    assert rc == 0, out
    for name in ("mf_song_master.wav", "mf_song_master.flac") + (("mf_song_master.mp3",) if "mp3" in fmts else ()):
        assert (d / name).exists(), (name, [x.name for x in d.iterdir()], out)
    reps = {x.name for x in d.glob("*.report.json")}
    assert "mf_song_master.report.json" in reps and "mf_song_master_flac.report.json" in reps, reps
    if "mp3" in fmts:
        assert "mf_song_master_mp3.report.json" in reps, reps
    rc2, out2 = cli(str(p), "-f", "bogus", "-o", str(d))
    assert rc2 == 2 and "unknown format" in out2, (rc2, out2[-400:])


if __name__ == "__main__":
    only = sys.argv[1:]
    bad = [r for r in RESULTS if r[1] == "FAIL"]
    (OUT / "results.json").write_text(json.dumps(RESULTS, indent=2))
    print(f"\n{len(RESULTS) - len(bad)}/{len(RESULTS)} passed")
    sys.exit(1 if bad else 0)