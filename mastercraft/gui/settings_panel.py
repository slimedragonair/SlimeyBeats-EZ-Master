"""Settings UI: main controls, per-module toggles/amounts and an auto-generated Advanced panel."""
import copy
import json
from pathlib import Path

from PySide6.QtCore import Qt, Signal, QTimer
from PySide6.QtWidgets import (QWidget, QVBoxLayout, QHBoxLayout, QFormLayout, QComboBox, QDoubleSpinBox, QSpinBox,
                               QCheckBox, QSlider, QLabel, QGroupBox, QLineEdit, QPushButton, QFileDialog, QScrollArea,
                               QToolButton, QGridLayout, QStyle, QListWidget, QListWidgetItem)

from ..audio_io import EXPORT_FORMATS, mp3_available, output_name
from .. import profiles as PR
from .. import settings as SET

MODULE_LABELS = {
    "headroom": "Headroom (-3 dB in)", "declip": "Declip", "cleanup": "DC / rumble / polarity", "denoise": "Denoise (only if noisy)",
    "static_eq": "Corrective EQ (tilt + bells)", "resonance": "Resonance notches", "low_end": "Low-end fix",
    "low_punch": "Low-end punch (kick attack + sub duck)", "dynamic_eq": "Dynamic EQ (mud/harsh/fizz)",
    "deesser": "De-esser", "hf_rebuild": "HF rebuild (band-limited)", "multiband": "Multiband compression", "glue": "Glue compressor",
    "saturation": "Saturation", "stereo": "Stereo width + mono bass",
    "clipper": "Soft clipper (pre-limiter, 4x)", "limiter": "True-peak limiter",
}


def _range_for(key, val):
    k = key.lower()
    if k.endswith("_hz") or k in ("fc",):
        return 10.0, 24000.0, 1.0, 0
    if k.endswith("_ms"):
        return 0.1, 3000.0, 1.0, 1
    if "db" in k:
        return -60.0, 60.0, 0.1, 2
    if k in ("ratio",) or k.endswith("_ratio"):
        return 1.0, 20.0, 0.1, 2
    if k in ("amount", "intensity"):
        return 0.0, 1.5, 0.05, 2
    return -1000.0, 100000.0, 0.05, 3


class SettingsPanel(QWidget):
    changed = Signal()

    def __init__(self, settings, parent=None):
        super().__init__(parent)
        self.s = copy.deepcopy(settings)
        self._adv_widgets = {}
        self._mod_widgets = {}
        outer = QVBoxLayout(self)
        outer.setContentsMargins(0, 0, 0, 0)
        sc = QScrollArea(); sc.setWidgetResizable(True)
        sc.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)   # v0.2.2: width is fitted, never h-scroll
        self._sc = sc
        body = QWidget(); lay = QVBoxLayout(body)
        sc.setWidget(body); outer.addWidget(sc)

        # ---- main
        g = QGroupBox("Mastering"); f = QFormLayout(g)
        self.profile = QComboBox()
        prow = QHBoxLayout(); prow.setContentsMargins(0, 0, 0, 0)   # v0.2.2: buttons on their own row (narrow panel)
        self.btn_save_prof = QToolButton(); self.btn_save_prof.setText("Save…"); self.btn_save_prof.setToolTip("Save current settings as a custom profile (JSON)")
        self.btn_load_prof = QToolButton(); self.btn_load_prof.setText("Import…")
        self.btn_del_prof = QToolButton(); self.btn_del_prof.setText("Delete")
        self.btn_blend_prof = QToolButton(); self.btn_blend_prof.setText("Blend…")
        self.btn_blend_prof.setToolTip("Blend two genre profiles (e.g. Metal 60 % + EDM 40 % = synth metal) and save it as a preset")
        for b in (self.btn_save_prof, self.btn_load_prof, self.btn_blend_prof, self.btn_del_prof):
            prow.addWidget(b)
        prow.addStretch(1)
        f.addRow("Genre profile", self.profile)
        f.addRow("", prow)
        self.mode = QComboBox(); self.mode.addItem("Streaming-safe (-14 LUFS class)", "streaming_safe"); self.mode.addItem("Loud (genre loudness)", "loud")
        f.addRow("Output mode", self.mode)
        self.strict = QCheckBox("Spotify-strict (-2 dBTP when loud)"); f.addRow("", self.strict)
        self.lufs_auto = QCheckBox("from profile"); self.lufs = QDoubleSpinBox(); self.lufs.setRange(-30, -4); self.lufs.setSingleStep(0.5); self.lufs.setSuffix(" LUFS")
        r = QHBoxLayout(); r.addWidget(self.lufs); r.addWidget(self.lufs_auto); f.addRow("Target loudness", r)
        self.ceil_auto = QCheckBox("from profile"); self.ceil = QDoubleSpinBox(); self.ceil.setRange(-6, 0); self.ceil.setSingleStep(0.1); self.ceil.setSuffix(" dBTP")
        r = QHBoxLayout(); r.addWidget(self.ceil); r.addWidget(self.ceil_auto); f.addRow("True-peak ceiling", r)
        self.intensity = QSlider(Qt.Horizontal); self.intensity.setRange(0, 150); self.int_lbl = QLabel()
        r = QHBoxLayout(); r.addWidget(self.intensity, 1); r.addWidget(self.int_lbl); f.addRow("Intensity", r)
        self.reference = QLineEdit(); self.reference.setPlaceholderText("none — match a reference track")
        b = QToolButton(); b.setText("…"); b.setToolTip("Choose a reference track to match the tone/width/loudness to")
        b.clicked.connect(self._pick_reference)
        r = QHBoxLayout(); r.addWidget(self.reference, 1); r.addWidget(b); f.addRow("Reference track", r)
        self.targets_lbl = QLabel(); self.targets_lbl.setWordWrap(True); self.targets_lbl.setStyleSheet("color:#8ad;"); f.addRow("", self.targets_lbl)
        lay.addWidget(g)

        # ---- export
        g = QGroupBox("Export"); f = QFormLayout(g)
        # one or more formats at once: each gets its own file (and its own JSON report)
        self.fmts = QListWidget()
        self.fmts.setSelectionMode(QListWidget.NoSelection)
        self.fmts.setToolTip("Every ticked format is written from the same master, in one run")
        for k, (lbl, ext, tag) in EXPORT_FORMATS.items():
            it = QListWidgetItem(lbl)
            it.setData(Qt.UserRole, k)
            it.setFlags(it.flags() | Qt.ItemIsUserCheckable)
            it.setCheckState(Qt.Unchecked)
            if k == "mp3" and not mp3_available():
                it.setFlags(it.flags() & ~Qt.ItemIsEnabled)
                it.setToolTip("MP3 export is not available in this installation")
            self.fmts.addItem(it)
        self.fmts.setFixedHeight(self.fmts.sizeHintForRow(0) * min(self.fmts.count(), 6) + 6)
        f.addRow("Formats (tick one or more)", self.fmts)
        self.osr = QComboBox()
        for v, lbl in ((0, "Keep source"), (44100, "44.1 kHz"), (48000, "48 kHz"), (88200, "88.2 kHz"), (96000, "96 kHz")):
            self.osr.addItem(lbl, v)
        f.addRow("Sample rate (all formats)", self.osr)
        self.out_dir = QLineEdit(); self.out_dir.setPlaceholderText("<app>\\output")
        b = QToolButton(); b.setText("…"); b.clicked.connect(self._pick_dir)
        r = QHBoxLayout(); r.addWidget(self.out_dir, 1); r.addWidget(b); f.addRow("Output folder", r)
        self.suffix = QLineEdit(); self.suffix.setToolTip("Added before the format tag, e.g. _master -> song_master_16bit.wav")
        f.addRow("Name suffix", self.suffix)
        self.fmt_preview = QLabel(); self.fmt_preview.setWordWrap(True); self.fmt_preview.setStyleSheet("color:#8ad;")
        f.addRow("", self.fmt_preview)
        self.report = QCheckBox("Write a JSON report next to each file")
        self.report.setToolTip("One report per written format, so a WAV and an MP3 master keep separate reports")
        f.addRow("", self.report)
        lay.addWidget(g)

        # ---- modules
        g = QGroupBox("Modules (toggle / amount)"); grid = QGridLayout(g)
        for i, (m, lbl) in enumerate(MODULE_LABELS.items()):
            cb = QCheckBox(lbl)
            grid.addWidget(cb, i, 0)
            sl = None
            if "amount" in SET.DEFAULTS["modules"][m]:
                sl = QSlider(Qt.Horizontal); sl.setRange(0, 150); sl.setToolTip("Amount (100% = research defaults)"); sl.setMinimumWidth(48)
                grid.addWidget(sl, i, 1)
            self._mod_widgets[m] = (cb, sl)
        self.safety = QCheckBox("Safety check (back off if worse)")
        self.safety.setToolTip("Re-analyse each output and back off if it measures worse than the input")
        grid.addWidget(self.safety, len(MODULE_LABELS), 0, 1, 2)
        lay.addWidget(g)

        # ---- advanced
        self.adv_toggle = QToolButton(); self.adv_toggle.setText("▸ Advanced"); self.adv_toggle.setCheckable(True)
        self.adv_toggle.setStyleSheet("QToolButton{border:none;font-weight:bold;}")
        lay.addWidget(self.adv_toggle)
        self.adv = QWidget(); av = QVBoxLayout(self.adv); av.setContentsMargins(0, 0, 0, 0)
        for m, params in SET.DEFAULTS["modules"].items():
            box = QGroupBox(MODULE_LABELS.get(m, m)); fl = QFormLayout(box)
            for k, v in params.items():
                if k in ("enabled", "amount"):
                    continue
                w = self._make_adv(k, v)
                fl.addRow(k.replace("_", " "), w)
                self._adv_widgets[(m, k)] = w
            av.addWidget(box)
        box = QGroupBox("Safety"); fl = QFormLayout(box)
        for k, v in SET.DEFAULTS["safety"].items():
            if k == "enabled":
                continue
            w = self._make_adv(k, v); fl.addRow(k.replace("_", " "), w); self._adv_widgets[("__safety", k)] = w
        av.addWidget(box)
        rb = QPushButton("Reset all settings to defaults"); rb.clicked.connect(self.reset_defaults); av.addWidget(rb)
        self.adv.setVisible(False)
        self.adv_toggle.toggled.connect(lambda on: (self.adv.setVisible(on), self.adv_toggle.setText(("▾" if on else "▸") + " Advanced")))
        lay.addWidget(self.adv)
        lay.addStretch(1)
        # v0.2.2 (QA v0.2.1 window size): compact, responsive widths so the panel never needs a horizontal
        # scrollbar: forms wrap long rows, combos size to a short content length, and the panel's minimum
        # width is fitted to the body's real minimum (+ the vertical scrollbar)
        for fl_ in body.findChildren(QFormLayout):
            fl_.setRowWrapPolicy(QFormLayout.WrapLongRows)
        for cb_ in body.findChildren(QComboBox):
            cb_.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
            cb_.setMinimumContentsLength(10)
        self._body = body
        self.adv_toggle.toggled.connect(lambda _on: QTimer.singleShot(0, self.fit_width))
        self.fit_width()

        self.refresh_profiles()
        self.load_into_ui()
        for w in (self.profile, self.mode, self.osr):
            w.currentIndexChanged.connect(self._on_change)
        for w in (self.strict, self.lufs_auto, self.ceil_auto, self.report, self.safety):
            w.toggled.connect(self._on_change)
        for w in (self.lufs, self.ceil):
            w.valueChanged.connect(self._on_change)
        self.intensity.valueChanged.connect(self._on_change)
        self.out_dir.editingFinished.connect(self._on_change)
        self.reference.editingFinished.connect(self._on_change)
        self.suffix.textChanged.connect(lambda _t: self._update_fmt_preview())
        self.suffix.editingFinished.connect(self._on_change)
        self.fmts.itemChanged.connect(self._on_fmt_changed)
        for cb, sl in self._mod_widgets.values():
            cb.toggled.connect(self._on_change)
            if sl:
                sl.valueChanged.connect(self._on_change)
        for w in self._adv_widgets.values():
            for sig in ("valueChanged", "toggled", "editingFinished"):
                if hasattr(w, sig):
                    getattr(w, sig).connect(self._on_change)
                    break

    def _on_fmt_changed(self, *_a):
        """Keep at least one format ticked, then save + update the filename preview."""
        if not self._checked_formats():
            self.fmts.blockSignals(True)
            self._set_checked_formats([self.s["general"].get("format") or "wav24"])
            self.fmts.blockSignals(False)
        self._update_fmt_preview()
        self._on_change()

    def _make_adv(self, k, v):
        if isinstance(v, bool):
            return QCheckBox()
        if isinstance(v, int) and not isinstance(v, bool):
            w = QSpinBox(); w.setRange(0, 100000); return w
        if isinstance(v, float):
            lo, hi, st, dec = _range_for(k, v)
            w = QDoubleSpinBox(); w.setRange(lo, hi); w.setSingleStep(st); w.setDecimals(dec); return w
        w = QLineEdit(); w.setPlaceholderText("auto (from profile/analysis)"); return w

    # -------- profile list
    def showEvent(self, e):   # re-fit once the style/fonts are final
        super().showEvent(e); self.fit_width()

    def fit_width(self):
        """Minimum width = body minimum + vertical scrollbar, so no horizontal scrolling is ever needed."""
        sc = self._sc
        vb = max(sc.verticalScrollBar().sizeHint().width(), sc.style().pixelMetric(QStyle.PM_ScrollBarExtent), 20)
        w = self._body.minimumSizeHint().width() + vb + 2 * sc.frameWidth() + 4
        self.setMinimumWidth(w)
        return w

    def refresh_profiles(self, select=None):
        self.profile.blockSignals(True)
        self.profile.clear()
        for k, disp, custom in PR.list_profiles():
            self.profile.addItem(disp, k)
        key = select or self.s["general"]["profile"]
        i = self.profile.findData(key)
        self.profile.setCurrentIndex(max(i, 0))
        self.profile.blockSignals(False)

    # -------- sync
    def _checked_formats(self):
        out = []
        for i in range(self.fmts.count()):
            it = self.fmts.item(i)
            if it.checkState() == Qt.Checked and (it.flags() & Qt.ItemIsEnabled):
                out.append(it.data(Qt.UserRole))
        return out

    def _set_checked_formats(self, keys):
        keys = list(keys) or ["wav24"]
        for i in range(self.fmts.count()):
            it = self.fmts.item(i)
            it.setCheckState(Qt.Checked if it.data(Qt.UserRole) in keys else Qt.Unchecked)

    def _update_fmt_preview(self):
        """Show exactly which files a run will write (the honest version of a 'format' label)."""
        ks = self._checked_formats()
        suffix = self.suffix.text().strip()
        names = [output_name("song", suffix, k) for k in ks]
        self.fmt_preview.setText(("→ " + "  ·  ".join(names)) if names else
                                 "→ no format selected: tick at least one")

    def load_into_ui(self):
        widgets = [self.profile, self.mode, self.strict, self.lufs, self.lufs_auto, self.ceil, self.ceil_auto, self.intensity,
                   self.reference, self.fmts, self.osr, self.out_dir, self.suffix, self.report, self.safety] + \
                  [w for pair in self._mod_widgets.values() for w in pair if w] + list(self._adv_widgets.values())
        for w in widgets:
            w.blockSignals(True)
        g = self.s["general"]
        i = self.profile.findData(g["profile"]); self.profile.setCurrentIndex(max(i, 0))
        self.mode.setCurrentIndex(max(self.mode.findData(g["mode"]), 0))
        self.strict.setChecked(bool(g["spotify_strict"]))
        self.strict.setEnabled(g["mode"] == "loud")  # QA L2: only meaningful in loud mode
        prof = PR.get_profile(self.profile.currentData())
        tl, tp = SET.resolve_targets(self.s, prof)
        self.lufs_auto.setChecked(g["target_lufs"] is None); self.lufs.setValue(tl); self.lufs.setEnabled(g["target_lufs"] is not None)
        self.ceil_auto.setChecked(g["ceiling_dbtp"] is None); self.ceil.setValue(tp); self.ceil.setEnabled(g["ceiling_dbtp"] is not None)
        self.intensity.setValue(int(round(g["intensity"] * 100))); self.int_lbl.setText(f"{int(round(g['intensity'] * 100))}%")
        self.reference.setText(g.get("reference") or "")
        from ..postprocess import export_formats
        self._set_checked_formats(export_formats(g))
        self.osr.setCurrentIndex(max(self.osr.findData(g["output_sr"]), 0))
        self.out_dir.setText(g["out_dir"]); self.suffix.setText(g["suffix"]); self.report.setChecked(g["write_report"])
        self._update_fmt_preview()
        self.safety.setChecked(self.s["safety"]["enabled"])
        for m, (cb, sl) in self._mod_widgets.items():
            cb.setChecked(self.s["modules"][m]["enabled"])
            if sl:
                sl.setValue(int(round(self.s["modules"][m].get("amount", 1.0) * 100)))
        for (m, k), w in self._adv_widgets.items():
            v = self.s["safety"][k] if m == "__safety" else self.s["modules"][m][k]
            if isinstance(w, QCheckBox):
                w.setChecked(bool(v))
            elif isinstance(w, (QSpinBox, QDoubleSpinBox)):
                w.setValue(v if v is not None else 0)
            else:
                w.setText("" if v is None else json.dumps(v))
        for w in widgets:
            w.blockSignals(False)
        self.targets_lbl.setText(f"→ {tl:.1f} LUFS integrated, ceiling {tp:.1f} dBTP"
                                 + (f"  ·  reference: {Path(g['reference']).name}" if g.get("reference") else ""))

    def read_from_ui(self):
        g = self.s["general"]
        g["profile"] = self.profile.currentData()
        g["mode"] = self.mode.currentData()
        g["spotify_strict"] = self.strict.isChecked()
        # QA L3: a stored value outside the spin box range is shown clamped; don't write the clamped
        # value back unless the user actually changed it
        def _keep(old, w):
            v = round(w.value(), 2)
            if old is not None and round(min(max(float(old), w.minimum()), w.maximum()), 2) == v:
                return old
            return v
        g["target_lufs"] = None if self.lufs_auto.isChecked() else _keep(g.get("target_lufs"), self.lufs)
        g["ceiling_dbtp"] = None if self.ceil_auto.isChecked() else _keep(g.get("ceiling_dbtp"), self.ceil)
        g["intensity"] = self.intensity.value() / 100.0
        checked = self._checked_formats()
        if checked:                      # never save "no output format"
            g["formats"] = checked
            g["format"] = checked[0]     # legacy key, kept in sync for older code/settings readers
        g["output_sr"] = self.osr.currentData()
        g["out_dir"] = self.out_dir.text().strip()
        g["reference"] = self.reference.text().strip()
        g["suffix"] = self.suffix.text().strip()
        g["write_report"] = self.report.isChecked()
        self.s["safety"]["enabled"] = self.safety.isChecked()
        for m, (cb, sl) in self._mod_widgets.items():
            self.s["modules"][m]["enabled"] = cb.isChecked()
            if sl:
                self.s["modules"][m]["amount"] = sl.value() / 100.0
        for (m, k), w in self._adv_widgets.items():
            tgt = self.s["safety"] if m == "__safety" else self.s["modules"][m]
            if isinstance(w, QCheckBox):
                tgt[k] = w.isChecked()
            elif isinstance(w, QSpinBox):
                tgt[k] = int(w.value())
            elif isinstance(w, QDoubleSpinBox):
                tgt[k] = float(w.value())
            else:
                t = w.text().strip()
                try:
                    tgt[k] = None if not t else json.loads(t)
                    w.setStyleSheet(""); w.setToolTip("")
                except ValueError:  # QA L8: keep the previous value instead of silently storing null
                    w.setStyleSheet("border:1px solid #c0392b")
                    w.setToolTip("Invalid JSON; the previous value is kept")
        return copy.deepcopy(self.s)

    def _on_change(self, *a):
        prev_profile = self.s["general"]["profile"]
        self.read_from_ui()
        if self.s["general"]["profile"] != prev_profile:
            prof = PR.get_profile(self.s["general"]["profile"])
            if prof.get("settings"):
                SET._merge(self.s, {"modules": prof["settings"].get("modules", {})})
        self.load_into_ui()
        self.changed.emit()

    def reset_defaults(self):
        self.s = SET.defaults()
        self.load_into_ui()
        SET.save(self.s, explicit=True)   # an explicit reset also lifts a blocked autosave (QA N9)
        self.changed.emit()
        self.changed.emit()

    def _pick_dir(self):
        d = QFileDialog.getExistingDirectory(self, "Output folder", self.out_dir.text())
        if d:
            self.out_dir.setText(d)
            self._on_change()

    def _pick_reference(self):
        f, _ = QFileDialog.getOpenFileName(self, "Reference track", self.reference.text(),
                                           "Audio (*.wav *.flac *.mp3 *.ogg *.aiff);;All files (*.*)")
        if f:
            self.reference.setText(f)
            self._on_change()

    def settings(self):
        return self.read_from_ui()
