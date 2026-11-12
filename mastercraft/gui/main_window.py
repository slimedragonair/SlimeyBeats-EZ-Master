"""Master-Craft main window."""
import copy
from pathlib import Path

import numpy as np
from PySide6.QtCore import Qt, QThread, Signal, QTimer
from PySide6.QtGui import QAction, QKeySequence
from PySide6.QtWidgets import (QMainWindow, QWidget, QSplitter, QVBoxLayout, QHBoxLayout, QPushButton, QTabWidget,
                               QTextBrowser, QProgressBar, QTableWidgetItem, QFileDialog, QLabel, QSlider, QMessageBox,
                               QInputDialog, QStatusBar, QScrollArea, QToolButton, QSizePolicy, QApplication, QStyle)

from .. import settings as SET, profiles as PR, __version__
from ..audio_io import AUDIO_EXTS
from ..profiles import target_curve_at
from .widgets import SpectrumView, WaveformView, QueueTable
from .settings_panel import SettingsPanel
from .worker import Worker, Job
from .player import ABPlayer
from .eq_panel import ManualEQPanel
from . import report_html

DARK = """
QWidget{background:#1f2228;color:#dde;font-family:'Segoe UI';font-size:10pt}
QGroupBox{border:1px solid #3a3f4a;border-radius:4px;margin-top:10px;padding-top:6px}
QGroupBox::title{subcontrol-origin:margin;left:8px;color:#9bd}
QPushButton{background:#2f3540;border:1px solid #475063;border-radius:4px;padding:5px 10px}
QPushButton:hover{background:#3a4352} QPushButton:checked{background:#2b6cb0}
QPushButton#primary{background:#2b6cb0;font-weight:bold}
QTableWidget{background:#191b20;gridline-color:#2a2e36} QHeaderView::section{background:#2a2e36;border:none;padding:4px}
QLineEdit,QComboBox,QDoubleSpinBox,QSpinBox{background:#262a31;border:1px solid #3d4452;padding:2px}
QTabBar::tab{background:#2a2e36;padding:6px 14px} QTabBar::tab:selected{background:#3a4352}
QProgressBar{border:1px solid #3d4452;text-align:center} QProgressBar::chunk{background:#2b6cb0}
"""


class MainWindow(QMainWindow):
    start_jobs = Signal(list)

    def __init__(self):
        super().__init__()
        self.setWindowTitle(f"Master-Craft {__version__} — automatic mastering for Suno songs")
        # v0.2.2: open at 1480x920 or the available screen area, whichever is smaller
        scr = QApplication.primaryScreen()
        ag = scr.availableGeometry() if scr is not None else None
        if ag is not None and ag.width() > 0:
            self.resize(min(1480, int(ag.width() * 0.96)), min(920, int(ag.height() * 0.92)))
        else:
            self.resize(1480, 920)
        self.setStyleSheet(DARK)
        self.settings = SET.load()
        self.results = {}       # row -> payload
        self.busy = False

        # ---- left: queue
        self.queue = QueueTable()
        self.queue.files_dropped.connect(self.add_paths)
        self.queue.itemSelectionChanged.connect(self.show_selected)
        btn_add = QPushButton("Add files…"); btn_add.clicked.connect(self.pick_files)
        btn_rm = QPushButton("Remove"); btn_rm.clicked.connect(self.remove_selected)
        btn_clr = QPushButton("Clear"); btn_clr.clicked.connect(self.clear_queue)
        self.btn_an = QPushButton("Analyze"); self.btn_an.clicked.connect(lambda: self.run(analyze_only=True))
        self.btn_go = QPushButton("Master & export"); self.btn_go.setObjectName("primary"); self.btn_go.clicked.connect(self.run)
        self.btn_go.setToolTip("Master every queued song and write the ticked export formats (Settings → Export)")
        self.btn_cancel = QPushButton("Cancel"); self.btn_cancel.setEnabled(False); self.btn_cancel.clicked.connect(self.cancel)
        self.btn_open = QPushButton("Open output"); self.btn_open.setEnabled(False)
        self.btn_open.setToolTip("Open the folder of the selected master (all formats are written there)")
        self.btn_open.clicked.connect(self.open_output)
        self.total = QProgressBar(); self.total.setFormat("Batch %p%")
        qb = QHBoxLayout()
        for b in (btn_add, btn_rm, btn_clr):
            qb.addWidget(b)
        qb.addStretch(1)
        for b in (self.btn_an, self.btn_open, self.btn_go, self.btn_cancel):
            qb.addWidget(b)
        self.queue.setMinimumHeight(60)
        left = QWidget(); lv = QVBoxLayout(left); lv.addLayout(qb); lv.addWidget(self.queue, 1); lv.addWidget(self.total)

        # ---- centre: tabs
        self.tabs = QTabWidget()
        self.report = QTextBrowser(); self.report.setOpenExternalLinks(False)
        self.report.setHtml("<p style='color:#99a'>Drop songs on the queue, then press <b>Analyze</b> or <b>Master all</b>.</p>")
        self.spec = SpectrumView()
        self.wave = WaveformView()
        vis = QWidget(); vv = QVBoxLayout(vis); vv.addWidget(QLabel("Spectrum (1/6-octave LTAS, dB)")); vv.addWidget(self.spec, 3)
        vv.addWidget(QLabel("Waveform (top: before, bottom: after) — click to seek")); vv.addWidget(self.wave, 2)
        self.tabs.addTab(self.report, "Report")
        self.tabs.addTab(vis, "Spectrum && Waveform")
        self.eq = ManualEQPanel()
        # v0.2.2: the EQ tab scrolls vertically on short screens (width is fitted, never h-scroll)
        self.eq_page = QScrollArea(); self.eq_page.setWidgetResizable(True); self.eq_page.setWidget(self.eq)
        self.eq_page.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff); self.eq_page.setFrameShape(QScrollArea.NoFrame)
        vb = max(self.eq_page.verticalScrollBar().sizeHint().width(), self.eq_page.style().pixelMetric(QStyle.PM_ScrollBarExtent), 20)
        self.eq_page.setMinimumWidth(self.eq.minimumSizeHint().width() + vb + 2)
        self.eq_page.setMinimumHeight(180)
        self.tabs.addTab(self.eq_page, "Manual EQ")

        # ---- transport
        self.player = ABPlayer(self)
        self.btn_play = QPushButton("▶ Play"); self.btn_play.clicked.connect(self.play_pause)
        self.btn_stop = QPushButton("■"); self.btn_stop.clicked.connect(self.player.stop)
        self.btn_a = QPushButton("A: Before"); self.btn_a.setCheckable(True)
        self.btn_b = QPushButton("B: After"); self.btn_b.setCheckable(True); self.btn_b.setChecked(True)
        self.btn_a.clicked.connect(lambda: self.player.set_active("A"))
        self.btn_b.clicked.connect(lambda: self.player.set_active("B"))
        self.player.state.connect(self._ab_state)
        self.vol = QSlider(Qt.Horizontal); self.vol.setRange(0, 100); self.vol.setValue(80); self.vol.setMaximumWidth(120); self.vol.setMinimumWidth(50)
        self.vol.valueChanged.connect(lambda v: (self.player.set_volume(v / 100), self.eq.set_volume(v / 100)))
        self.eq.playing.connect(lambda on: (self.player.pause(), self.btn_play.setText("? Play")) if on else None)
        self.pos_lbl = QLabel("0:00")
        self.match_lbl = QLabel(""); self.match_lbl.setStyleSheet("color:#8ad")
        self.match_lbl.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)   # v0.2.2: never widens the window
        self.btn_play.setToolTip("Play / pause (Space). Tab switches A/B")
        self.player.position.connect(self._pos)
        self.wave.seek.connect(self.player.seek)
        tb = QHBoxLayout()
        for w in (self.btn_play, self.btn_stop, self.btn_a, self.btn_b):
            tb.addWidget(w)
        tb.addWidget(QLabel("Vol")); tb.addWidget(self.vol); tb.addWidget(self.pos_lbl); tb.addWidget(self.match_lbl, 1)
        centre = QWidget(); cv = QVBoxLayout(centre); cv.addWidget(self.tabs, 1); cv.addLayout(tb)

        # ---- right: settings
        self.panel = SettingsPanel(self.settings)
        self.panel.changed.connect(self._settings_changed)
        self.panel.btn_save_prof.clicked.connect(self.save_profile)
        self.panel.btn_load_prof.clicked.connect(self.import_profile)
        self.panel.btn_blend_prof.clicked.connect(self.blend_profile)
        self.panel.btn_del_prof.clicked.connect(self.delete_profile)

        sp = QSplitter()
        lsp = QSplitter(Qt.Vertical); lsp.addWidget(left); lsp.addWidget(centre); lsp.setSizes([300, 620])
        sp.addWidget(lsp); sp.addWidget(self.panel); sp.setSizes([1060, 420])
        # v0.2.2: queue and settings panes are collapsible (drag the handle or use the status-bar toggles)
        lsp.setCollapsible(0, True); lsp.setCollapsible(1, False)
        sp.setCollapsible(0, False); sp.setCollapsible(1, True)
        self.split, self.lsplit, self.queue_pane = sp, lsp, left
        self.setCentralWidget(sp)
        self.setStatusBar(QStatusBar())
        self.statusBar().showMessage("Ready")
        self._settings_banner = QLabel("")
        self._settings_banner.setStyleSheet("color:#e67e22")
        self.statusBar().addPermanentWidget(self._settings_banner)
        self.btn_show_queue = QToolButton(); self.btn_show_queue.setText("Queue"); self.btn_show_queue.setCheckable(True)
        self.btn_show_queue.setChecked(True); self.btn_show_queue.setToolTip("Show / hide the queue pane")
        self.btn_show_queue.toggled.connect(self.queue_pane.setVisible)
        self.btn_show_settings = QToolButton(); self.btn_show_settings.setText("Settings"); self.btn_show_settings.setCheckable(True)
        self.btn_show_settings.setChecked(True); self.btn_show_settings.setToolTip("Show / hide the settings panel")
        self.btn_show_settings.toggled.connect(self.panel.setVisible)
        self.statusBar().addPermanentWidget(self.btn_show_queue); self.statusBar().addPermanentWidget(self.btn_show_settings)
        a = QAction(self); a.setShortcut(QKeySequence("Ctrl+S")); a.triggered.connect(self.save_settings_explicit)
        self.addAction(a)
        if SET.LOAD_ERROR:   # QA N9: corrupt settings.json -> warn, keep it, no autosave over it
            self._settings_banner.setText("Settings file unreadable: autosave is off (Ctrl+S saves)")
            self._settings_banner.setToolTip(SET.LOAD_ERROR)
            QTimer.singleShot(0, self._warn_settings)

        for key, fn in (("Space", self.play_pause), ("Tab", self._tab_key), ("Ctrl+O", self.pick_files)):
            a = QAction(self); a.setShortcut(QKeySequence(key)); a.setShortcutContext(Qt.ApplicationShortcut)
            a.triggered.connect(fn); self.addAction(a)
        self.player.finished.connect(lambda: self.btn_play.setText("▶ Play"))
        from .worker import purge_stale_previews
        purge_stale_previews()

        # ---- worker thread
        self.thread = QThread(self)
        self.worker = Worker()
        self.worker.moveToThread(self.thread)
        self.start_jobs.connect(self.worker.run_jobs)
        self.worker.progress.connect(self._progress)
        self.worker.done.connect(self._done)
        self.worker.failed.connect(self._failed)
        self.worker.idle.connect(self._idle)
        self.thread.start()
        self._save_timer = QTimer(self); self._save_timer.setSingleShot(True); self._save_timer.setInterval(600)
        self._save_timer.timeout.connect(lambda: SET.save(self.settings))

    # ---------- queue
    def pick_files(self):
        files, _ = QFileDialog.getOpenFileNames(self, "Add songs", "", "Audio (*.wav *.flac *.mp3 *.ogg *.aif *.aiff *.m4a);;All (*.*)")
        self.add_paths(files)

    def add_paths(self, paths):
        files = []
        for p in paths:
            P = Path(p)
            if P.is_dir():
                files += [str(f) for f in sorted(P.rglob("*")) if f.suffix.lower() in AUDIO_EXTS]
            elif P.suffix.lower() in AUDIO_EXTS:
                files.append(str(P))
        existing = {self.queue.item(r, 0).data(Qt.UserRole) for r in range(self.queue.rowCount())}
        for f in files:
            if f in existing:
                continue
            r = self.queue.rowCount()
            self.queue.insertRow(r)
            it = QTableWidgetItem(Path(f).name); it.setData(Qt.UserRole, f); it.setToolTip(f)
            self.queue.setItem(r, 0, it)
            self.queue.setItem(r, 1, QTableWidgetItem("queued"))
            pb = QProgressBar(); pb.setRange(0, 100); pb.setValue(0); pb.setMaximumHeight(16)
            self.queue.setCellWidget(r, 2, pb)
            for c in range(3, 7):
                self.queue.setItem(r, c, QTableWidgetItem(""))
        if files:
            self.statusBar().showMessage(f"{len(files)} file(s) added")
            if self.queue.currentRow() < 0:
                self.queue.selectRow(0)

    def remove_selected(self):
        if self.busy:
            return
        rows = sorted({i.row() for i in self.queue.selectedIndexes()}, reverse=True)
        for r in rows:
            self.queue.removeRow(r)
        self.results = {}
        self.player.load(None, None)

    def clear_queue(self):
        if self.busy:
            return
        self.queue.setRowCount(0)
        self.results = {}
        self.player.load(None, None)

    # ---------- run
    def run(self, analyze_only=False):
        if self.busy or self.queue.rowCount() == 0:
            return
        self.settings = self.panel.settings()
        SET.save(self.settings)
        s = copy.deepcopy(self.settings)
        prof = PR.get_profile(s["general"]["profile"])
        if prof.get("settings"):
            SET._merge(s, {"modules": prof["settings"].get("modules", {})})
        jobs = [Job(r, self.queue.item(r, 0).data(Qt.UserRole), copy.deepcopy(s), analyze_only)
                for r in range(self.queue.rowCount())]
        for j in jobs:
            self.queue.item(j.row, 1).setText("waiting")
            self.queue.cellWidget(j.row, 2).setValue(0)
        self.player.stop()
        self.player.load(None, None)
        self._njobs, self._ndone = len(jobs), 0
        self.total.setValue(0)
        self.busy = True
        self._set_busy(True)
        self.start_jobs.emit(jobs)

    def cancel(self):
        self.worker.cancel()
        self.statusBar().showMessage("Cancelling after current step…")

    def _set_busy(self, b):
        self.btn_go.setEnabled(not b); self.btn_an.setEnabled(not b); self.btn_cancel.setEnabled(b)

    def _progress(self, row, pct, msg):
        if row < self.queue.rowCount():
            self.queue.cellWidget(row, 2).setValue(pct)
            self.queue.item(row, 1).setText(msg)
            self.total.setValue(int(100 * (self._ndone + pct / 100) / max(self._njobs, 1)))

    def _done(self, row, payload):
        self._ndone += 1
        self.results[row] = payload
        inp, out = payload.get("input"), payload.get("output")
        paths = payload.get("out_paths") or ([payload["out_path"]] if payload.get("out_path") else [])
        status = "mastered" if out else "analyzed"
        self.queue.item(row, 1).setText(status + (f" · {len(paths)} files" if len(paths) > 1 else ""))
        self.queue.item(row, 1).setToolTip("\n".join(paths))
        self.btn_open.setEnabled(any(p.get("out_path") for p in self.results.values()))
        self.queue.item(row, 3).setText(f"{inp['metrics']['integrated_lufs']:.1f}")
        if out:
            self.queue.item(row, 4).setText(f"{out['metrics']['integrated_lufs']:.1f}")
            self.queue.item(row, 5).setText(f"{out['metrics']['true_peak_dbtp']:+.1f}")
        top = inp["issues"][0] if inp["issues"] else None
        self.queue.item(row, 6).setText(f"{top['title']} ({top['severity']:.0f})" if top else "")
        if self.queue.currentRow() == row or self.queue.currentRow() < 0:
            self.queue.selectRow(row)
            self.show_selected()

    def open_output(self):
        """Reveal the folder of the selected (or last) master in Explorer."""
        from PySide6.QtGui import QDesktopServices
        from PySide6.QtCore import QUrl
        r = self.queue.currentRow()
        order = [r] + [k for k in range(len(self.results)) if k != r]
        for k in order:
            p = self.results.get(k) or {}
            if p.get("out_path"):
                d = Path(p["out_path"]).parent
                if d.exists():
                    QDesktopServices.openUrl(QUrl.fromLocalFile(str(d)))
                    self.statusBar().showMessage(f"Opened {d}")
                    return
        self.statusBar().showMessage("Nothing to open yet: master a song first")

    def _failed(self, row, msg):
        self._ndone += 1
        if row < self.queue.rowCount():
            self.queue.item(row, 1).setText("failed" if msg != "Cancelled" else "cancelled")
            self.queue.item(row, 1).setToolTip(msg)
        if msg != "Cancelled":
            self.statusBar().showMessage(f"Failed: {msg.splitlines()[0]}")

    def _idle(self):
        self.busy = False
        self._set_busy(False)
        self.total.setValue(100)
        self.statusBar().showMessage("Batch finished")

    # ---------- display
    def show_selected(self):
        r = self.queue.currentRow()
        p = self.results.get(r)
        if not p:
            return
        self.report.setHtml(report_html.render(p))
        inp, out = p.get("input"), p.get("output")
        tgt = None
        if inp and inp.get("tonal"):
            prof = PR.get_profile(self.settings["general"]["profile"])
            fr = np.asarray(inp["spectrum"]["freqs"])
            # octave-band target -> per-bin density shape (-10log10 f), aligned at 1 kHz to the input curve
            dens = target_curve_at(prof, fr) - 10 * np.log10(fr / 1000.0)
            ref = np.interp(1000.0, fr, np.asarray(inp["spectrum"]["db"]))
            tgt = (fr, dens - np.interp(1000.0, fr, dens) + ref + inp["tonal"].get("offset_db", 0.0))
        self.spec.set_data(inp.get("spectrum") if inp else None, out.get("spectrum") if out else None, tgt)
        self.wave.set_data(p.get("env_before"), p.get("env_after"), p.get("duration", 0))
        self.player.load(p.get("preview_before"), p.get("preview_after"))
        mp = p.get("y_master_path")
        if mp and Path(mp).exists():
            # lazy: only the selected row's master is in RAM (QA v0.2.0 #4); the panel drops the previous one
            if self.eq.song_key != mp:
                from .worker import load_cached_master
                self.eq.clear()
                self.eq.load_song(load_cached_master(mp), p["sr"], p.get("out_path"),
                                  out["metrics"]["integrated_lufs"], p["targets"]["ceiling_dbtp"], p.get("general"), key=mp)
        else:
            self.eq.clear()
        self._ab_state(self.player.active)
        md = p.get("match_db")
        self.match_lbl.setText(f"Loudness-matched A/B (before {md['before']:+.1f} dB, after {md['after']:+.1f} dB)" if md
                               else "Before only (master to enable A/B)")

    def play_pause(self):
        if self.tabs.currentWidget() is self.eq_page and self.eq.isEnabled():
            self.eq.play_pause()
            return
        self.eq.pause()
        if self.player.is_playing():
            self.player.pause(); self.btn_play.setText("▶ Play")
        else:
            self.player.play(); self.btn_play.setText("❚❚ Pause")

    def _ab_state(self, which):
        self.btn_a.setChecked(which == "A"); self.btn_b.setChecked(which == "B")

    def _pos(self, sec):
        self.pos_lbl.setText(f"{int(sec // 60)}:{int(sec % 60):02d}")
        self.wave.set_position(sec)

    # ---------- settings / profiles
    def _settings_changed(self):
        self.settings = self.panel.settings()
        self._save_timer.start()

    def save_profile(self):
        name, ok = QInputDialog.getText(self, "Save custom profile", "Profile name:")
        if not ok or not name.strip():
            return
        s = self.panel.settings()
        prof = PR.get_profile(s["general"]["profile"])
        base = prof["key"] if not prof["key"].startswith("custom:") else prof.get("base", "neutral")
        p = PR.save_custom_profile(name.strip(), prof, settings={"modules": s["modules"]}, base=base)
        key = "custom:" + p.stem
        self.panel.s["general"]["profile"] = key
        self.panel.refresh_profiles(key)
        self.panel.load_into_ui()
        self.statusBar().showMessage(f"Saved profile to {p}")

    def import_profile(self):
        f, _ = QFileDialog.getOpenFileName(self, "Import profile JSON", "", "JSON (*.json)")
        if not f:
            return
        import json
        try:
            d = json.loads(Path(f).read_text(encoding="utf-8-sig"))
            name = d.get("display_name") or Path(f).stem
            base = d.get("base", "neutral")
            prof = PR.get_profile(base)
            PR._deep_update(prof, d)
            p = PR.save_custom_profile(name, prof, settings=d.get("settings"), base=base)
            self.panel.refresh_profiles("custom:" + p.stem)
            self.panel._on_change()
        except Exception as e:
            QMessageBox.warning(self, "Import failed", str(e))

    def blend_profile(self):
        """Blend two built-in profiles and save the result as a custom profile.

        Most real tracks sit between two curves (synth metal = Metal + EDM, retro instrumental =
        Lo-fi + Game), so a weighted blend is a first-class preset instead of a hand edit."""
        from PySide6.QtWidgets import (QDialog, QFormLayout, QComboBox, QDoubleSpinBox, QDialogButtonBox,
                                       QLineEdit, QVBoxLayout, QLabel)
        base = [(k, d) for k, d, custom in PR.list_profiles() if not custom]
        if len(base) < 2:
            QMessageBox.information(self, "Blend", "Blending needs at least two built-in profiles.")
            return
        dlg = QDialog(self)
        dlg.setWindowTitle("Blend two genre profiles")
        lay = QVBoxLayout(dlg)
        form = QFormLayout()
        a = QComboBox(); b = QComboBox()
        for k, d in base:
            a.addItem(d, k); b.addItem(d, k)
        b.setCurrentIndex(min(1, b.count() - 1))
        w = QDoubleSpinBox(); w.setRange(10, 90); w.setSingleStep(5); w.setDecimals(0)
        w.setValue(60); w.setSuffix(" % first profile")
        name = QLineEdit(); name.setPlaceholderText("e.g. Synth Metal")
        info = QLabel(); info.setWordWrap(True); info.setStyleSheet("color:#8ad;")
        form.addRow("Profile A", a); form.addRow("Profile B", b); form.addRow("Weight", w)
        form.addRow("Name", name); form.addRow("", info)
        lay.addLayout(form)
        buttons = QDialogButtonBox(QDialogButtonBox.Ok | QDialogButtonBox.Cancel)
        buttons.accepted.connect(dlg.accept); buttons.rejected.connect(dlg.reject)
        lay.addWidget(buttons)

        def _upd():
            try:
                spec = f"{a.currentData()}:{w.value() / 100:.2f}+{b.currentData()}:{1 - w.value() / 100:.2f}"
                pr = PR.get_profile(spec)
                t = pr["tonal_db"]
                info.setText(f"{pr['display_name']}  ->  -{abs(pr['lufs']['loud']):.1f} LUFS loud / "
                             f"-{abs(pr['lufs']['streaming_safe']):.1f} streaming-safe; "
                             f"target {t[0]:+.1f}/{t[2]:+.1f}/{t[6]:+.1f}/{t[8]:+.1f} dB "
                             f"(31.5/125/2k/8k Hz vs 1 kHz)")
                if not name.text().strip():
                    name.setPlaceholderText(f"e.g. {pr['display_name'].replace('Blend: ', '')}")
            except Exception as e:      # noqa: bad weight or unknown key: show it, don't crash the dialog
                info.setText(f"cannot build that blend: {e}")
        for c in (a, b):
            c.currentIndexChanged.connect(_upd)
        w.valueChanged.connect(_upd)
        _upd()
        if dlg.exec() != QDialog.Accepted:
            return
        nm = name.text().strip() or f"{a.currentText()} + {b.currentText()}"
        spec = f"{a.currentData()}:{w.value() / 100:.2f}+{b.currentData()}:{1 - w.value() / 100:.2f}"
        try:
            path, pr = PR.save_blend_profile(nm, spec)
        except Exception as e:
            QMessageBox.warning(self, "Blend failed", str(e))
            return
        key = "custom:" + path.stem
        self.panel.s["general"]["profile"] = key
        self.panel.refresh_profiles(key)
        self.panel.load_into_ui()
        self.statusBar().showMessage(f"Saved blend '{nm}' ({pr['display_name']}) to {path}", 6000)

    def delete_profile(self):
        key = self.panel.profile.currentData()
        if not key or not key.startswith("custom:"):
            QMessageBox.information(self, "Profiles", "Built-in profiles can't be deleted.")
            return
        PR.delete_custom_profile(key)
        self.panel.s["general"]["profile"] = "neutral"
        self.panel.refresh_profiles("neutral")
        self.panel.load_into_ui()

    def _warn_settings(self):
        box = QMessageBox(QMessageBox.Warning, "Settings file could not be read", SET.LOAD_ERROR, parent=self)
        save_btn = box.addButton("Save current settings now", QMessageBox.AcceptRole)
        box.addButton("Keep the file as it is", QMessageBox.RejectRole)
        box.setModal(False)
        box.buttonClicked.connect(lambda b: self.save_settings_explicit() if b is save_btn else None)
        box.show()
        self._settings_box = box

    def save_settings_explicit(self):
        self.settings = self.panel.settings()
        if SET.save(self.settings, explicit=True):
            self._settings_banner.setText(""); self._settings_banner.setToolTip("")
            self.statusBar().showMessage("Settings saved", 3000)

    def _tab_moves_focus(self, fw):
        """QA L8: Tab moves focus as usual inside text/spin fields and anywhere in the settings
        panel or the queue (slider, combos, table), so keyboard navigation is never trapped. Elsewhere
        (transport buttons, waveform, no focus) Tab is the A/B toggle."""
        from PySide6.QtWidgets import QLineEdit, QAbstractSpinBox, QTextEdit, QPlainTextEdit
        if fw is None:
            return False
        if isinstance(fw, (QLineEdit, QAbstractSpinBox, QTextEdit, QPlainTextEdit)):
            return True
        w = fw
        while w is not None:
            if w is self.panel or w is getattr(self, "queue", None) or w is getattr(self, "eq", None):
                return True
            w = w.parentWidget()
        return False

    def _tab_key(self):
        from PySide6.QtWidgets import QApplication
        fw = QApplication.focusWidget()
        if self._tab_moves_focus(fw):
            self.focusNextChild()
            return
        self.player.toggle()

    def closeEvent(self, e):
        self.eq.stop()
        self.eq.save_sidecar()
        SET.save(self.panel.settings())
        self.worker.cancel()
        self.player.stop()
        self.player.load(None, None)  # release the preview files so they can be deleted
        self.thread.quit()
        self.thread.wait(3000)
        from .worker import cleanup_previews, cleanup_masters
        cleanup_previews()
        self.eq.clear()
        cleanup_masters()
        super().closeEvent(e)
