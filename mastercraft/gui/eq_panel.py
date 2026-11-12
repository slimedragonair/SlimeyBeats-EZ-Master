"""Manual EQ tab: FL-Studio-style 7-band parametric EQ on the finished master, with live audition.

Live path: QAudioSink (pull mode) <- EQStream.readData() <- manual_eq.Processor.process() on the
auto master. Every edit builds a new filter chain that is crossfaded in over 20 ms (each output
sample lies between the old and the new EQ, so a transition can never be louder than the louder
of the two steady states). While stopped, edits snap; Play/seek prime the chain on the audio just
before the play position. Export runs the same processor offline.
"""
import copy
import math
import threading
import time
from pathlib import Path

import numpy as np
from PySide6.QtCore import Qt, Signal, QTimer, QPointF, QRectF, QIODevice, QSize
from PySide6.QtGui import QPainter, QPen, QColor, QBrush, QPainterPath, QFont, QIcon, QPixmap
from PySide6.QtWidgets import (QWidget, QHBoxLayout, QVBoxLayout, QLabel, QSlider, QComboBox, QDial, QToolButton,
                               QPushButton, QFileDialog, QSizePolicy, QFrame, QButtonGroup, QMenu)

from .. import manual_eq as M
from .flow import FlowLayout
from .. import loudness as LN

GRID_F = [20, 30, 40, 50, 60, 70, 80, 90, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 2000, 3000, 4000, 5000,
          6000, 7000, 8000, 9000, 10000, 20000]
LABEL_F = [(20, "20"), (50, "50"), (100, "100"), (200, "200"), (500, "500"), (1000, "1k"), (2000, "2k"), (5000, "5k"),
           (10000, "10k")]
AN_TOP, AN_RANGE = -6.0, 84.0        # analyzer: dBFS at the top of the graph, and the span shown
NFFT = 4096
COALESCE_S = 0.6                     # keyboard / wheel / click edits closer than this = one undo step
STRIP_W = 32


def _qc(hexs, a=255):
    c = QColor(hexs); c.setAlpha(a); return c


def _fmt(v, spec="+.2f", none="n/a"):
    try:
        v = float(v)
    except (TypeError, ValueError):
        return none
    return format(v, spec) if math.isfinite(v) else none


def type_icon(t, color="#dfe6ea", w=26, h=14):
    """Small drawn curve for a band type (FL-style icon buttons)."""
    pm = QPixmap(w, h); pm.fill(Qt.transparent)
    p = QPainter(pm); p.setRenderHint(QPainter.Antialiasing)
    p.setPen(QPen(QColor(color), 1.6))
    path = QPainterPath()
    m, hi, lo = h / 2, 2.5, h - 2.5
    if t == "bell":
        path.moveTo(1, m); path.cubicTo(w * 0.35, m, w * 0.38, hi, w / 2, hi); path.cubicTo(w * 0.62, hi, w * 0.65, m, w - 1, m)
    elif t == "low_shelf":
        path.moveTo(1, hi); path.lineTo(w * 0.3, hi); path.cubicTo(w * 0.5, hi, w * 0.5, m + 2, w * 0.7, m + 2); path.lineTo(w - 1, m + 2)
    elif t == "high_shelf":
        path.moveTo(1, m + 2); path.lineTo(w * 0.3, m + 2); path.cubicTo(w * 0.5, m + 2, w * 0.5, hi, w * 0.7, hi); path.lineTo(w - 1, hi)
    elif t == "low_cut":
        path.moveTo(w * 0.18, lo); path.cubicTo(w * 0.35, m - 1, w * 0.4, m - 1, w * 0.55, m - 1); path.lineTo(w - 1, m - 1)
    else:   # high_cut
        path.moveTo(1, m - 1); path.lineTo(w * 0.45, m - 1); path.cubicTo(w * 0.6, m - 1, w * 0.65, m - 1, w * 0.82, lo)
    p.drawPath(path); p.end()
    return QIcon(pm)


# ------------------------------------------------------------------ graph
class EQGraph(QWidget):
    changed = Signal()          # live edit
    editStarted = Signal()      # snapshot for undo
    selected = Signal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumSize(220, 150)   # v0.2.2: shrinks with the window
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.ClickFocus)
        self.st = M.default_state()
        self.sr = 48000
        self.sel = -1
        self._drag = -1
        self.fgrid = np.geomspace(M.F_MIN, M.F_MAX, 400)
        self.an_in = self.an_out = None
        self.an_label = ""
        self._wheel_t = 0.0

    # mapping
    def plot_rect(self):
        return QRectF(8, 38, self.width() - 44, self.height() - 58)

    def f2x(self, f, r):
        return r.left() + r.width() * np.log(f / M.F_MIN) / np.log(M.F_MAX / M.F_MIN)

    def x2f(self, x, r):
        return float(np.clip(M.F_MIN * (M.F_MAX / M.F_MIN) ** ((x - r.left()) / r.width()), M.F_MIN, M.F_MAX))

    def g2y(self, g, r):
        return r.top() + r.height() * (M.GAIN_RANGE - g) / (2 * M.GAIN_RANGE)

    def y2g(self, y, r):
        return float(np.clip(M.GAIN_RANGE - (y - r.top()) / r.height() * 2 * M.GAIN_RANGE, -M.GAIN_RANGE, M.GAIN_RANGE))

    def a2y(self, db, r):
        return r.top() + r.height() * np.clip((AN_TOP - db) / AN_RANGE, 0, 1)

    def node_pos(self, i, r):
        b = self.st["bands"][i]
        g = b["gain"] if b["type"] in ("bell", "low_shelf", "high_shelf") else 0.0
        return QPointF(self.f2x(b["freq"], r), self.g2y(g, r))

    def set_state(self, st, sr=None):
        self.st = st
        if sr:
            self.sr = sr
        self.update()

    def set_analyzer(self, fin, fout, label=""):
        self.an_in, self.an_out, self.an_label = fin, fout, label
        self.update()

    # painting
    def paintEvent(self, e):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        p.fillRect(self.rect(), QColor("#1d242b"))
        r = self.plot_rect()
        p.fillRect(r, QColor("#17222a"))
        f = QFont(self.font()); f.setPointSizeF(7.5); p.setFont(f)
        # note row (C1..C9), like FL's keyboard ruler
        for k, (name, nf) in enumerate(M.NOTES):
            x0 = self.f2x(nf, r)
            x1 = self.f2x(M.NOTES[k + 1][1], r) if k + 1 < len(M.NOTES) else min(self.f2x(nf * 2, r), r.right())
            rr = QRectF(x0 + 1, 3, max(4.0, x1 - x0 - 2), 13)
            p.fillRect(rr, QColor("#27323a" if k % 2 else "#2d3943"))
            p.setPen(QColor("#9fb0bd")); p.drawText(rr.adjusted(3, 0, 0, 0), Qt.AlignLeft | Qt.AlignVCenter, name)
        # region bar
        for name, lo, hi in M.REGIONS:
            x0, x1 = self.f2x(lo, r), self.f2x(hi, r)
            rr = QRectF(x0 + 1, 19, x1 - x0 - 2, 15)
            p.fillRect(rr, QColor("#34434f"))
            p.setPen(QColor("#b9c6d0")); p.drawText(rr, Qt.AlignCenter, name)
        # grid
        for gf in GRID_F:
            x = self.f2x(gf, r)
            p.setPen(QPen(QColor("#2a3842" if gf not in dict(LABEL_F) else "#33454f"), 1))
            p.drawLine(QPointF(x, r.top()), QPointF(x, r.bottom()))
        for g in (-18, -12, -6, 0, 6, 12, 18):
            y = self.g2y(g, r)
            p.setPen(QPen(QColor("#3b4d58" if g == 0 else "#26333c"), 1))
            p.drawLine(QPointF(r.left(), y), QPointF(r.right(), y))
            p.setPen(QColor("#8aa0b0")); p.drawText(QRectF(r.right() + 4, y - 7, 34, 14), Qt.AlignLeft | Qt.AlignVCenter, f"{g:+d}" if g else "0")
        for lf, lab in LABEL_F:
            x = self.f2x(lf, r)
            p.setPen(QColor("#8aa0b0")); p.drawText(QRectF(x + 2, r.bottom() + 3, 40, 14), Qt.AlignLeft, lab)
        if self.an_label:
            p.setPen(QColor("#6f8696"))
            p.drawText(QRectF(r.left() + 6, r.bottom() - 16, r.width() - 12, 14), Qt.AlignLeft | Qt.AlignVCenter,
                       "MANUAL EQ (post-master)   " + self.an_label)
        p.setClipRect(r)
        # analyzer: input (grey fill) and output (cyan line)
        xs = [self.f2x(fq, r) for fq in self.fgrid]
        if self.an_in is not None:
            path = QPainterPath(QPointF(xs[0], r.bottom()))
            for x, v in zip(xs, self.an_in):
                path.lineTo(x, self.a2y(v, r))
            path.lineTo(xs[-1], r.bottom()); path.closeSubpath()
            p.fillPath(path, QBrush(_qc("#7f8c96", 70)))
        if self.an_out is not None:
            path = QPainterPath(QPointF(xs[0], self.a2y(self.an_out[0], r)))
            for x, v in zip(xs, self.an_out):
                path.lineTo(x, self.a2y(v, r))
            p.setPen(QPen(_qc("#5fd3ff", 170), 1.3)); p.drawPath(path)
        # per-band curves and total
        tot = np.zeros(len(self.fgrid))
        byp = self.st["bypass"]
        for i, b in enumerate(self.st["bands"]):
            if not b["on"]:
                continue
            h = M.band_response_db(b, self.fgrid, self.sr)
            tot += h
            if byp:
                continue
            path = QPainterPath(QPointF(xs[0], self.g2y(0, r)))
            for x, v in zip(xs, h):
                path.lineTo(x, self.g2y(np.clip(v, -30, 30), r))
            path.lineTo(xs[-1], self.g2y(0, r)); path.closeSubpath()
            p.fillPath(path, QBrush(_qc(M.COLORS[i], 55 if i == self.sel else 28)))
        path = QPainterPath(QPointF(xs[0], self.g2y(np.clip(tot[0], -30, 30), r)))
        for x, v in zip(xs, tot):
            path.lineTo(x, self.g2y(np.clip(v, -30, 30), r))
        p.setPen(QPen(QColor("#6d7a84") if byp else QColor("#f2f6f8"), 2)); p.drawPath(path)
        p.setClipping(False)
        # nodes
        f.setPointSizeF(8.5); f.setItalic(True); p.setFont(f)
        for i, b in enumerate(self.st["bands"]):
            c = self.node_pos(i, r)
            col = QColor(M.COLORS[i]) if b["on"] and not byp else QColor("#55606a")
            p.setBrush(QBrush(_qc(M.COLORS[i], 70 if i == self.sel else 35)))
            p.setPen(QPen(col, 3 if i == self.sel else 2))
            p.drawEllipse(c, 11, 11)
            p.setPen(col); p.drawText(QRectF(c.x() - 10, c.y() - 10, 20, 20), Qt.AlignCenter, str(i + 1))
        p.end()

    # mouse
    def _hit(self, pos):
        r = self.plot_rect()
        best, bd = -1, 15.0
        for i in range(M.NBANDS):
            c = self.node_pos(i, r)
            d = np.hypot(c.x() - pos.x(), c.y() - pos.y())
            if d < bd:
                best, bd = i, d
        return best

    def mousePressEvent(self, e):
        i = self._hit(e.position())
        if i >= 0:
            self.sel = i
            self._drag = i
            self.editStarted.emit()
            self.selected.emit(i)
            self.update()

    def mouseMoveEvent(self, e):
        if self._drag < 0:
            self.setCursor(Qt.PointingHandCursor if self._hit(e.position()) >= 0 else Qt.ArrowCursor)
            return
        r = self.plot_rect()
        b = self.st["bands"][self._drag]
        b["freq"] = round(self.x2f(e.position().x(), r), 1)
        if b["type"] in ("bell", "low_shelf", "high_shelf"):
            g = self.y2g(e.position().y(), r)
            b["gain"] = round(0.0 if abs(g) < 0.15 else g, 2)
        self.changed.emit()
        self.update()

    def mouseReleaseEvent(self, e):
        self._drag = -1

    def mouseDoubleClickEvent(self, e):
        i = self._hit(e.position())
        if i >= 0:
            self.editStarted.emit()
            self.st["bands"][i] = M.default_band(i)
            self.changed.emit()
            self.update()

    def wheelEvent(self, e):
        i = self._hit(e.position())
        if i < 0:
            i = self.sel
        if i < 0:
            return
        now = time.perf_counter()
        if now - self._wheel_t > COALESCE_S:
            self.editStarted.emit()
        self._wheel_t = now
        b = self.st["bands"][i]
        steps = e.angleDelta().y() / 120.0
        b["q"] = round(float(np.clip(b["q"] * 1.12 ** steps, M.Q_MIN, M.Q_MAX)), 3)
        self.sel = i
        self.selected.emit(i)
        self.changed.emit()
        self.update()
        e.accept()


# ------------------------------------------------------------------ band strip
def _f2d(f):
    return int(round(1000 * np.log(f / M.F_MIN) / np.log(M.F_MAX / M.F_MIN)))


def _d2f(d):
    return float(M.F_MIN * (M.F_MAX / M.F_MIN) ** (d / 1000.0))


def _q2d(q):
    return int(round(1000 * np.log(q / M.Q_MIN) / np.log(M.Q_MAX / M.Q_MIN)))


def _d2q(d):
    return float(M.Q_MIN * (M.Q_MAX / M.Q_MIN) ** (d / 1000.0))


class _Coalesce:
    """Undo-step policy for continuous controls: a drag is one step (sliderPressed); keyboard,
    wheel and click-on-groove changes start a new step only after COALESCE_S of quiet."""

    def __init__(self):
        self.t = -1e9

    def need(self, *sliders):
        now = time.perf_counter()
        dragging = any(s.isSliderDown() for s in sliders)
        new = (not dragging) and (now - self.t > COALESCE_S)
        self.t = now
        return new

    def pressed(self):
        self.t = time.perf_counter()


_STRIP_CSS = ("QFrame#strip{{background:#20282f;border-radius:3px}} QLabel{{background:transparent;font-size:7pt}}"
              "QSlider::handle:vertical{{background:{c};height:12px;margin:0 -4px;border-radius:3px}}"
              "QSlider::groove:vertical{{background:#11171c;width:6px;border-radius:3px}}"
              "QDial{{background:#2b343c}} QToolButton{{padding:0;border:1px solid #3a4650;border-radius:2px;background:#273038}}"
              "QToolButton:checked{{background:{c};color:#111}}")


class BandStrip(QFrame):
    changed = Signal()
    editStarted = Signal()

    def __init__(self, i, parent=None):
        super().__init__(parent)
        self.setObjectName("strip")
        self.i = i
        self.b = M.default_band(i)
        self._mute = False
        self._co = _Coalesce()
        col = M.COLORS[i]
        self.setStyleSheet(_STRIP_CSS.format(c=col))
        self.setFixedWidth(STRIP_W)
        v = QVBoxLayout(self); v.setContentsMargins(2, 3, 2, 3); v.setSpacing(2)
        self.on = QToolButton(); self.on.setCheckable(True); self.on.setChecked(True); self.on.setText(str(i + 1))
        self.on.setFixedSize(STRIP_W - 6, 16); self.on.setToolTip(f"Band {i + 1} on/off")
        self.type = QToolButton(); self.type.setFixedSize(STRIP_W - 6, 18); self.type.setIconSize(QSize(24, 13))
        self.type.setPopupMode(QToolButton.InstantPopup); self.type.setToolTip("Band type")
        menu = QMenu(self.type)
        for t in M.TYPES:
            a = menu.addAction(type_icon(t), M.TYPE_LABELS[t])
            a.triggered.connect(lambda _=False, t=t: self._set_type(t))
        self.type.setMenu(menu)
        self.type.setStyleSheet("QToolButton::menu-indicator{image:none;width:0}")
        self.gain = QSlider(Qt.Vertical); self.gain.setRange(-180, 180); self.gain.setMinimumHeight(80)
        self.gain.setToolTip("Gain (dB)  double-click: 0"); self.gain.setPageStep(10)
        self.glbl = QLabel("0.0"); self.glbl.setAlignment(Qt.AlignCenter)
        self.freq = QDial(); self.freq.setRange(0, 1000); self.freq.setFixedSize(30, 30); self.freq.setToolTip("FREQ")
        self.flbl = QLabel(""); self.flbl.setAlignment(Qt.AlignCenter)
        self.bw = QDial(); self.bw.setRange(0, 1000); self.bw.setFixedSize(30, 30); self.bw.setToolTip("BW (Q)")
        self.qlbl = QLabel(""); self.qlbl.setAlignment(Qt.AlignCenter)
        v.addWidget(self.on, 0, Qt.AlignHCenter); v.addWidget(self.type, 0, Qt.AlignHCenter)
        v.addWidget(self.gain, 1, Qt.AlignHCenter); v.addWidget(self.glbl)
        v.addWidget(self.freq, 0, Qt.AlignHCenter); v.addWidget(self.flbl)
        v.addWidget(self.bw, 0, Qt.AlignHCenter); v.addWidget(self.qlbl)
        for w in (self.gain, self.freq, self.bw):
            w.sliderPressed.connect(self._pressed)
            w.valueChanged.connect(self._ui)
        self.on.toggled.connect(lambda _: (self.editStarted.emit(), self._ui(discrete=True)) if not self._mute else None)
        self.gain.installEventFilter(self)

    def _pressed(self):
        self._co.pressed()
        self.editStarted.emit()

    def _set_type(self, t):
        if t == self.b["type"]:
            return
        self.editStarted.emit()
        self.b["type"] = t
        self._labels()
        self.changed.emit()

    def eventFilter(self, o, e):
        from PySide6.QtCore import QEvent
        if o is self.gain and e.type() == QEvent.MouseButtonDblClick:
            self.editStarted.emit(); self._co.pressed(); self.gain.setValue(0); return True
        return False

    def set_band(self, b):
        self.b = b
        self._mute = True
        self.gain.setValue(int(round(b["gain"] * 10))); self.freq.setValue(_f2d(b["freq"])); self.bw.setValue(_q2d(b["q"]))
        self.on.setChecked(b["on"])
        self._mute = False
        self._labels()

    def _labels(self):
        b = self.b
        self.glbl.setText(f"{b['gain']:+.1f}" if b["type"] in ("bell", "low_shelf", "high_shelf") else "-")
        f = b["freq"]
        self.flbl.setText(f"{f / 1000:.1f}k" if f >= 1000 else f"{f:.0f}")
        self.qlbl.setText(f"{b['q']:.2f}")
        self.gain.setEnabled(b["type"] in ("bell", "low_shelf", "high_shelf"))
        self.type.setIcon(type_icon(b["type"])); self.type.setToolTip(f"Band type: {M.TYPE_LABELS[b['type']]}")

    def _ui(self, *_, discrete=False):
        if self._mute:
            return
        if not discrete and self._co.need(self.gain, self.freq, self.bw):
            self.editStarted.emit()          # keyboard / wheel / click edit: its own undo step (QA v0.2.0 #5)
        b = self.b
        b["gain"] = self.gain.value() / 10.0
        b["freq"] = round(_d2f(self.freq.value()), 1)
        b["q"] = round(_d2q(self.bw.value()), 3)
        b["on"] = self.on.isChecked()
        self._labels()
        self.changed.emit()


class OutStrip(QFrame):
    """Master output fader (dB, after the EQ and loudness hold, before the true-peak guard)."""
    changed = Signal()
    editStarted = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("strip")
        self.setStyleSheet(_STRIP_CSS.format(c="#dfe6ea"))
        self.setFixedWidth(STRIP_W + 6)
        self._mute = False
        self._co = _Coalesce()
        self.st = None
        v = QVBoxLayout(self); v.setContentsMargins(2, 3, 2, 3); v.setSpacing(2)
        tag = QLabel("OUT"); tag.setAlignment(Qt.AlignCenter); tag.setStyleSheet("font-weight:bold;font-size:7pt")
        self.fader = QSlider(Qt.Vertical); self.fader.setRange(int(-M.OUT_RANGE * 10), int(M.OUT_RANGE * 10))
        self.fader.setPageStep(10); self.fader.setMinimumHeight(80)
        self.fader.setToolTip("Master output (dB). With loudness hold on it trims the held level.  double-click: 0")
        self.lbl = QLabel("0.0"); self.lbl.setAlignment(Qt.AlignCenter)
        self.gr = QLabel("GR\n0.0"); self.gr.setAlignment(Qt.AlignCenter); self.gr.setToolTip("True-peak guard gain reduction (dB)")
        v.addWidget(tag); v.addWidget(self.fader, 1, Qt.AlignHCenter); v.addWidget(self.lbl); v.addWidget(self.gr)
        self.fader.sliderPressed.connect(lambda: (self._co.pressed(), self.editStarted.emit()))
        self.fader.valueChanged.connect(self._ui)
        self.fader.installEventFilter(self)

    def eventFilter(self, o, e):
        from PySide6.QtCore import QEvent
        if o is self.fader and e.type() == QEvent.MouseButtonDblClick:
            self.editStarted.emit(); self._co.pressed(); self.fader.setValue(0); return True
        return False

    def set_state(self, st):
        self.st = st
        self._mute = True
        self.fader.setValue(int(round(st.get("out_gain_db", 0.0) * 10)))
        self._mute = False
        self.lbl.setText(f"{st.get('out_gain_db', 0.0):+.1f}")

    def set_gr(self, gr):
        self.gr.setText(f"GR\n{_fmt(gr, '.1f', '0.0')}")

    def _ui(self, *_):
        if self._mute or self.st is None:
            return
        if self._co.need(self.fader):
            self.editStarted.emit()
        self.st["out_gain_db"] = self.fader.value() / 10.0
        self.lbl.setText(f"{self.st['out_gain_db']:+.1f}")
        self.changed.emit()


# ------------------------------------------------------------------ audio stream
class EQStream(QIODevice):
    """Pull-mode source for QAudioSink: renders the next block through the processor on demand."""

    def __init__(self, panel):
        super().__init__()
        self.panel = panel
        self.x = None
        self.pos = 0
        self.ended = False
        self.fade = 0
        self.seek_req = None          # seek handled in the render thread: reset + prime + fade-in
        self.frames_out = 0           # frames handed to the sink so far
        self.last_render_ms = 0.0
        self.tap_in = np.zeros(NFFT); self.tap_out = np.zeros(NFFT)
        self.events = []              # (perf_counter, frames_out) after each read, for latency stats

    def isSequential(self):
        return True

    def bytesAvailable(self):
        return 1 << 20

    def start_at(self, pos):
        """Reset the processor and prime it on the audio before pos (no cold-filter transient)."""
        pr = self.panel.proc
        self.pos = int(np.clip(pos, 0, len(self.x)))
        pr.reset()
        if self.pos > 0:
            pr.prime(self.x[max(0, self.pos - pr.warm_len()):self.pos])
        self.fade = int(0.005 * pr.sr)
        self.ended = False

    def render(self, frames):
        x, pr = self.x, self.panel.proc
        if self.seek_req is not None:
            p, self.seek_req = self.seek_req, None
            self.start_at(p)
        nch = x.shape[1]
        blk = x[self.pos:self.pos + frames]
        if len(blk) < frames:
            blk = np.vstack([blk, np.zeros((frames - len(blk), nch), np.float32)])
            self.ended = True
        t0 = time.perf_counter()
        y = pr.process(blk)
        if self.fade > 0:   # 5 ms fade-in after a seek / start
            k = min(self.fade, frames)
            n0 = int(0.005 * pr.sr) - self.fade
            y[:k] *= (np.arange(n0, n0 + k) / int(0.005 * pr.sr))[:, None]
            self.fade -= k
        self.last_render_ms = (time.perf_counter() - t0) * 1000
        self.pos += frames
        mi, mo = blk.mean(axis=1), y.mean(axis=1)
        n = min(frames, NFFT)
        self.tap_in = np.concatenate([self.tap_in[n:], mi[-n:]]); self.tap_out = np.concatenate([self.tap_out[n:], mo[-n:]])
        return y

    def readData(self, maxlen):
        if self.x is None:
            return bytes()
        nch = self.x.shape[1]
        frames = min(int(maxlen) // (4 * nch), 4096)
        if frames <= 0:
            return bytes()
        y = self.render(frames)
        self.frames_out += frames
        self.events.append((time.perf_counter(), self.frames_out))
        if len(self.events) > 400:
            del self.events[:200]
        y = np.nan_to_num(y, nan=0.0, posinf=0.0, neginf=0.0)
        return np.clip(y, -1, 1).astype(np.float32).tobytes()

    def writeData(self, data):
        return -1


def spectrum_db(x, fgrid, sr):
    """Hann FFT magnitude (dBFS, sine = 0 dB) of the last NFFT samples, reduced to fgrid (max per bin group)."""
    w = np.hanning(len(x))
    X = np.abs(np.fft.rfft(np.nan_to_num(x) * w)) * 2 / w.sum()
    f = np.fft.rfftfreq(len(x), 1 / sr)
    db = 20 * np.log10(np.maximum(X, 1e-7))
    edges = np.sqrt(fgrid[:-1] * fgrid[1:])
    idx = np.searchsorted(f, np.concatenate([[fgrid[0] / 1.01], edges, [fgrid[-1] * 1.01]]))
    out = np.interp(fgrid, f, db)
    for i in range(len(fgrid)):
        a, b = idx[i], idx[i + 1]
        if b > a:
            out[i] = db[a:b].max()
    return out


# ------------------------------------------------------------------ panel
class ManualEQPanel(QWidget):
    playing = Signal(bool)
    status = Signal(str)
    _solved = Signal(int, object)
    _exported = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.slots = {"A": M.default_state(), "B": M.default_state()}
        self.active = "A"
        self.undo, self.redo = [], []
        self.x = None; self.sr = 48000; self.master_path = None; self.hold_lufs = -14.0; self.ceiling = -1.0
        self.general = {}
        self.song_key = None
        self.master_hash = None
        self.saved_sidecar = None     # sidecar made on a different master (not applied, QA v0.2.0 #5)
        self._side_dirty = False
        self.proc = M.Processor(48000, 2)
        self.model = None
        self.version = 0
        self.exact = None             # solve_gain_ex result for the current version
        self._est_gain = 0.0
        self.sink = None
        self.stream = EQStream(self)
        self.stream.open(QIODevice.ReadOnly)
        self.volume = 0.8
        self._static_in = None
        self._exporting = False

        self.graph = EQGraph()
        self.strips = [BandStrip(i) for i in range(M.NBANDS)]
        self.out_strip = OutStrip()
        sv = QHBoxLayout(); sv.setSpacing(2); sv.setContentsMargins(0, 0, 0, 0)
        for s in self.strips + [self.out_strip]:
            sv.addWidget(s)
            s.changed.connect(self._changed)
            s.editStarted.connect(self._snapshot)
        self.graph.changed.connect(self._changed)
        self.graph.editStarted.connect(self._snapshot)

        def btn(text, tip, cb, check=False):
            b = QPushButton(text); b.setToolTip(tip); b.setCheckable(check); b.clicked.connect(cb); return b
        self.btn_play = btn("Play", "Play the master through the manual EQ (live)", self.play_pause)
        self.btn_stop = btn("Stop", "Stop", self.stop)
        self.btn_byp = btn("Bypass", "Global bypass: hear the auto master without the manual EQ", self._bypass, True)
        self.btn_hold = btn("Hold loudness", "Keep the integrated loudness of the auto master (gain follows the EQ)", self._hold, True)
        self.btn_hold.setChecked(True)
        self.btn_a = btn("A", "Compare slot A", lambda: self.set_slot("A"), True)
        self.btn_b = btn("B", "Compare slot B", lambda: self.set_slot("B"), True)
        grp = QButtonGroup(self); grp.addButton(self.btn_a); grp.addButton(self.btn_b); self.btn_a.setChecked(True)
        self.btn_copy = btn("A>B", "Copy the active slot to the other one", self.copy_slot)
        self.btn_undo = btn("Undo", "Undo (Ctrl+Z)", self.do_undo)
        self.btn_redo = btn("Redo", "Redo (Ctrl+Y)", self.do_redo)
        self.btn_reset = btn("Reset", "All bands flat, output 0 dB", self.reset_all)
        self.btn_load = btn("Load...", "Load an EQ preset (JSON)", self.load_preset_dialog)
        self.btn_save = btn("Save...", "Save this EQ as a preset (JSON)", self.save_preset_dialog)
        self.btn_saved = btn("Apply saved EQ", "An EQ was saved for this song, but on a different master file", self.apply_saved_sidecar)
        self.btn_saved.setVisible(False)
        self.macro = QComboBox(); self.macro.addItem("Macros...", None)
        for k in M.MACROS:
            self.macro.addItem(k, k)
        self.macro.activated.connect(self._macro)
        self.macro.setSizeAdjustPolicy(QComboBox.AdjustToMinimumContentsLengthWithIcon)
        self.macro.setMinimumContentsLength(10); self.macro.setToolTip("Macros (absolute settings: applying twice = once)")
        self.btn_export = btn("Export with EQ", "Write <master>_eq with this EQ (same path as the preview)", self.export)
        self.btn_export.setObjectName("primary")
        self.pos = QSlider(Qt.Horizontal); self.pos.setRange(0, 1000); self.pos.sliderMoved.connect(self.seek_frac)
        self.info = QLabel(""); self.info.setStyleSheet("color:#8ad")
        # v0.2.2: one wrapping toolbar (flow layout) instead of two fixed rows, so a narrow window
        # wraps the buttons instead of forcing a wide minimum width
        self.toolbar = QWidget(); fl = FlowLayout(self.toolbar, 0, 4, 4)
        for w in (self.btn_play, self.btn_stop, self.btn_byp, self.btn_hold, self.btn_a, self.btn_b, self.btn_copy,
                  self.btn_undo, self.btn_redo, self.btn_reset, self.btn_load, self.btn_save, self.macro,
                  self.btn_saved, self.btn_export):
            fl.addWidget(w)
        pol = QSizePolicy(QSizePolicy.Preferred, QSizePolicy.Preferred); pol.setHeightForWidth(True)
        self.toolbar.setSizePolicy(pol)
        main = QHBoxLayout(); main.setSpacing(6); main.addWidget(self.graph, 1); main.addLayout(sv)
        v = QVBoxLayout(self); v.setContentsMargins(6, 6, 6, 6)
        v.addWidget(self.toolbar); v.addLayout(main, 1); v.addWidget(self.pos); v.addWidget(self.info)
        self.info.setWordWrap(True)

        self._solve_timer = QTimer(self); self._solve_timer.setSingleShot(True); self._solve_timer.setInterval(500)
        self._solve_timer.timeout.connect(self._solve_async)
        self._side_timer = QTimer(self); self._side_timer.setSingleShot(True); self._side_timer.setInterval(800)
        self._side_timer.timeout.connect(self.save_sidecar)
        self._an_timer = QTimer(self); self._an_timer.setInterval(40); self._an_timer.timeout.connect(self._analyzer_tick)
        self._an_in = self._an_out = None
        self._solved.connect(self._on_solved)
        self._exported.connect(self._on_exported)
        from PySide6.QtGui import QShortcut, QKeySequence
        QShortcut(QKeySequence.Undo, self, self.do_undo); QShortcut(QKeySequence.Redo, self, self.do_redo)
        self._sync_ui()
        self.setEnabled(False)

    # ---------- song
    @property
    def st(self):
        return self.slots[self.active]

    def load_song(self, x, sr, master_path=None, hold_lufs=-14.0, ceiling=-1.0, general=None, key=None):
        self.stop()
        self.x = np.ascontiguousarray(LN.as2d(np.asarray(x, dtype=np.float32)))
        np.nan_to_num(self.x, copy=False, nan=0.0, posinf=0.0, neginf=0.0)
        self.song_key = key
        self.sr = int(sr); self.master_path = master_path
        hl = float(hold_lufs) if hold_lufs is not None else float("nan")
        self.hold_lufs = hl if math.isfinite(hl) else -70.0     # silent master: hold is a no-op (QA v0.2.0 #3)
        self.ceiling = float(ceiling) if ceiling is not None and math.isfinite(float(ceiling)) else -1.0
        self.general = dict(general or {})
        self._refresh_export_label()
        self.proc = M.Processor(self.sr, self.x.shape[1], self.ceiling)
        self.stream.x = self.x; self.stream.pos = 0
        self.model = M.LoudnessModel(self.x, self.sr)
        self._static_spectrum()
        self.undo.clear(); self.redo.clear()
        self.master_hash = None
        if master_path and Path(master_path).exists():
            try:
                self.master_hash = M.file_hash(master_path)
            except OSError:
                self.master_hash = None
        self.slots = {"A": M.default_state(), "B": M.default_state()}; self.active = "A"
        self.saved_sidecar = None
        self._side_dirty = False
        msg = None
        if master_path and M.sidecar_path(master_path).exists():
            try:
                d = M.read_json(M.sidecar_path(master_path))
                if d.get("master_sha1") and d.get("master_sha1") == self.master_hash:
                    self._apply_sidecar_dict(d)
                    msg = "Restored the EQ saved for this master."
                else:   # different (re-mastered) file, or an old sidecar without a hash: never apply silently
                    self.saved_sidecar = d
                    msg = ("A saved EQ exists for this song but was made on a different master file; "
                           "not applied. Use 'Apply saved EQ' to load it anyway.")
            except Exception as e:  # noqa
                msg = f"Saved EQ unreadable, ignored: {type(e).__name__}"
        self.btn_saved.setVisible(self.saved_sidecar is not None)
        self.graph.sr = self.sr
        self.setEnabled(True)
        self._sync_ui()
        self._push(snap=True, user=False)
        if msg:
            self.info.setText(msg + "   " + self.info.text())

    def _apply_sidecar_dict(self, d):
        self.slots = {"A": M.clean_state(d.get("A", d)), "B": M.clean_state(d.get("B", d))}
        self.active = d.get("active", "A") if d.get("active") in ("A", "B") else "A"

    def apply_saved_sidecar(self):
        if not self.saved_sidecar:
            return
        self._snapshot()
        self._apply_sidecar_dict(self.saved_sidecar)
        self.saved_sidecar = None
        self.btn_saved.setVisible(False)
        self._sync_ui(); self._push()

    def clear(self):
        self.stop(); self.x = None; self.stream.x = None; self.model = None; self.song_key = None
        self._static_in = None
        self.setEnabled(False)

    def _static_spectrum(self):
        """Average FFT spectrum of the song (shown when not playing), on the same scale as the live analyzer."""
        m = self.x.mean(axis=1).astype(np.float64)
        hop = max(NFFT, len(m) // 160)
        frames = [m[i:i + NFFT] for i in range(0, len(m) - NFFT, hop)] or [np.pad(m, (0, max(0, NFFT - len(m))))[:NFFT]]
        w = np.hanning(NFFT)
        P = np.mean([np.abs(np.fft.rfft(f * w)) ** 2 for f in frames], axis=0)
        f = np.fft.rfftfreq(NFFT, 1 / self.sr)
        db = 10 * np.log10(np.maximum(P, 1e-14)) + 20 * np.log10(2 / w.sum())
        self._static_in = np.interp(self.graph.fgrid, f, db)

    # ---------- edits
    def _snapshot(self):
        snap = (copy.deepcopy(self.slots), self.active)
        if not self.undo or self.undo[-1] != snap:
            self.undo.append(snap)
            del self.undo[:-200]
        self.redo.clear()
        self._sync_buttons()

    def _changed(self):
        # strips and graph edit the same dicts; bring the other view up to date
        self._sync_ui()
        self._push()

    def _push(self, snap=False, user=True):
        """Send the active slot to the processor. While stopped it snaps (Play primes it);
        while playing or paused the processor crossfades (bounded, never louder than either end)."""
        self.version += 1
        st = self.st
        g = 0.0
        if st["hold_loudness"] and not st["bypass"] and self.model is not None:
            g = M.safe_db(-self.model.delta_db(st))      # instant estimate, refined by _solve_async
        self.exact = None
        self.proc.set_state(st, g, snap=snap or self.sink is None)
        self._est_gain = g
        self._solve_timer.start()
        if user and self.master_path:
            self._side_dirty = True
            self._side_timer.start()
        self._update_info()
        if not self.stream_active():
            self._show_static()

    def _solve_async(self):
        if self.x is None:
            return
        ver, st, x, sr = self.version, copy.deepcopy(self.st), self.x, self.sr
        hold, ceil, model = self.hold_lufs, self.ceiling, self.model

        def work():
            try:
                self._solved.emit(ver, M.solve_gain_ex(x, sr, st, hold, ceil, model))
            except Exception:  # noqa
                pass
        threading.Thread(target=work, daemon=True).start()

    def _on_solved(self, ver, res):
        if ver != self.version or self.x is None:
            return
        self.exact = res
        g = M.safe_db(res.get("gain_db", 0.0))
        self.proc.set_gain_db(g, snap=self.sink is None)
        if not self.stream_active():
            self._show_static()
        self._update_info()

    def hold_text(self):
        st = self.st
        if st["bypass"]:
            return "Bypassed: auto master only"
        if not st["hold_loudness"]:
            return "Loudness hold off: level follows the EQ"
        r = self.exact
        if r:
            if r.get("note") and not r.get("converged", True):
                return f"Loudness hold {_fmt(r['gain_db'])} dB - {r['note']}"
            if r.get("note"):
                return r["note"]
            return f"Loudness hold {_fmt(r['gain_db'])} dB (exact: {_fmt(r.get('lufs'), '.2f')} LUFS, target {_fmt(self.hold_lufs, '.2f')})"
        if self.model is not None and self.model.silent:
            return "The master is silent: loudness hold is off (gain 0 dB)."
        return f"Loudness hold {_fmt(self._est_gain)} dB (estimate, refining...)"

    def _update_info(self):
        gr = self.proc.gain_reduction_db()
        self.out_strip.set_gr(gr)
        og = self.st.get("out_gain_db", 0.0)
        self.info.setText(f"Slot {self.active}  |  {self.hold_text()}  |  out {og:+.1f} dB  |  "
                          f"TP guard {self.ceiling:.1f} dBTP, GR {_fmt(gr, '.1f', '0.0')} dB")

    def _sync_ui(self):
        st = self.st
        self.graph.set_state(st, self.sr)
        for s, b in zip(self.strips, st["bands"]):
            s.set_band(b)
        self.out_strip.set_state(st)
        self.btn_byp.setChecked(st["bypass"]); self.btn_hold.setChecked(st["hold_loudness"])
        self.btn_a.setChecked(self.active == "A"); self.btn_b.setChecked(self.active == "B")
        self.btn_copy.setText("A>B" if self.active == "A" else "B>A")
        self._sync_buttons()

    def _sync_buttons(self):
        self.btn_undo.setEnabled(bool(self.undo)); self.btn_redo.setEnabled(bool(self.redo))

    def _bypass(self):
        self._snapshot(); self.st["bypass"] = self.btn_byp.isChecked(); self._changed()

    def _hold(self):
        self._snapshot(); self.st["hold_loudness"] = self.btn_hold.isChecked(); self._changed()

    def set_slot(self, k):
        if k == self.active:
            return
        self.active = k
        self._sync_ui(); self._push()

    def toggle_ab(self):
        self.set_slot("B" if self.active == "A" else "A")

    def copy_slot(self):
        self._snapshot()
        other = "B" if self.active == "A" else "A"
        self.slots[other] = copy.deepcopy(self.st)
        self.info.setText(f"Copied {self.active} to {other}")

    def do_undo(self):
        if not self.undo:
            return
        self.redo.append((copy.deepcopy(self.slots), self.active))
        self.slots, self.active = self.undo.pop()
        self._sync_ui(); self._push()

    def do_redo(self):
        if not self.redo:
            return
        self.undo.append((copy.deepcopy(self.slots), self.active))
        self.slots, self.active = self.redo.pop()
        self._sync_ui(); self._push()

    def reset_all(self):
        self._snapshot()
        keep = {k: self.st[k] for k in ("hold_loudness",)}
        self.slots[self.active] = M.default_state(); self.slots[self.active].update(keep)
        self._sync_ui(); self._push()

    def _macro(self, idx):
        k = self.macro.itemData(idx)
        self.macro.setCurrentIndex(0)
        if not k:
            return
        if M.macro_active(self.st, k):       # absolute: a second click changes nothing (no stacking)
            self.info.setText(f"Macro '{k}' is already applied.")
            return
        self._snapshot()
        self.slots[self.active] = M.apply_macro(self.st, k)
        self._sync_ui(); self._push()

    def set_state(self, st):
        """Programmatic edit (tests, presets): one undo step."""
        self._snapshot()
        self.slots[self.active] = M.clean_state(st)
        self._sync_ui(); self._push()

    # ---------- presets / sidecar
    def preset_dir(self, candidates=None, ask=None):
        """v0.2.2: first writable folder of the fallback chain (paths.resolve_preset_dir). A clear
        error/notice is shown when the usual folder is not writable; %APPDATA% (system drive) is only
        used after the user agrees. Returns None if nothing is writable."""
        from .. import paths as PA
        from PySide6.QtWidgets import QMessageBox

        def ask_last(d):
            if ask is not None:
                return ask(d)
            return QMessageBox.question(self, "Presets folder", "None of the usual presets folders can be written.\n\n"
                                        f"Use {d} instead? (this is on the system drive)") == QMessageBox.Yes
        d, notice = PA.resolve_preset_dir(candidates, ask_last)
        self.preset_notice = notice
        if notice:
            self.info.setText(notice)
            if d is None and ask is None:
                QMessageBox.warning(self, "Presets folder", notice)
        return d

    def save_preset_dialog(self):
        d = self.preset_dir()
        if d is None:
            return
        f, _ = QFileDialog.getSaveFileName(self, "Save EQ preset", str(d / "my_eq.json"), "EQ preset (*.json)")
        if f:
            try:
                M.save_preset(self.st, f); self.info.setText(f"Preset saved: {f}")
            except OSError as e:
                self.info.setText(f"Could not save preset: {e}")

    def load_preset_dialog(self):
        d = self.preset_dir()
        f, _ = QFileDialog.getOpenFileName(self, "Load EQ preset", str(d) if d is not None else "", "EQ preset (*.json)")
        if f:
            try:
                self.set_state(M.load_preset(f))
            except Exception as e:  # noqa
                self.info.setText(f"Could not load preset: {e}")

    def save_sidecar(self, force=False):
        """Write <master>.eq.json (A/B, active slot, SHA-1 of the master). Only after an edit, so
        opening a song never overwrites an EQ saved for another master."""
        if not self.master_path or not (self._side_dirty or force):
            return None
        import json
        p = M.sidecar_path(self.master_path)
        d = {"version": 2, "master_sha1": self.master_hash, "master": Path(self.master_path).name,
             "active": self.active, "A": M.clean_state(self.slots["A"]), "B": M.clean_state(self.slots["B"])}
        tmp = p.with_suffix(".tmp")
        try:
            tmp.write_text(json.dumps(d, indent=2, allow_nan=False), encoding="utf-8"); tmp.replace(p)
        except (OSError, ValueError):
            return None
        self._side_dirty = False
        return p

    # ---------- transport
    def stream_active(self):
        return self.sink is not None and self.sink.state().name in ("ActiveState", "IdleState")

    def _make_sink(self):
        from PySide6.QtMultimedia import QAudioFormat, QAudioSink, QMediaDevices
        fmt = QAudioFormat()
        fmt.setSampleRate(self.sr); fmt.setChannelCount(self.x.shape[1]); fmt.setSampleFormat(QAudioFormat.Float)
        dev = QMediaDevices.defaultAudioOutput()
        if not dev.isFormatSupported(fmt):
            self.info.setText(f"The audio device does not take {self.sr} Hz float; trying anyway.")
        sink = QAudioSink(dev, fmt, self)
        sink.setBufferSize(int(self.sr * 0.05) * 4 * self.x.shape[1])    # ~50 ms
        sink.setVolume(self.volume)
        return sink

    def play_pause(self):
        if self.x is None:
            return
        if self.sink is not None and self.sink.state().name == "ActiveState":
            self.sink.suspend(); self._an_timer.stop(); self.btn_play.setText("Play"); self.playing.emit(False); return
        if self.sink is not None and self.sink.state().name == "SuspendedState":
            # edits made while paused were queued as crossfades: they play out bounded on resume
            self.sink.resume(); self._an_timer.start(); self.btn_play.setText("Pause"); self.playing.emit(True); return
        pos = self.stream.pos
        if self.stream.ended or pos >= len(self.x):
            pos = 0
        self.stream.seek_req = None
        self.stream.start_at(pos)          # fresh start: processor at the current EQ, primed, 5 ms fade-in
        self.sink = self._make_sink()
        self.sink.start(self.stream)
        self._an_timer.start()
        self.btn_play.setText("Pause")
        self.playing.emit(True)

    def pause(self):
        if self.sink is not None and self.sink.state().name == "ActiveState":
            self.play_pause()

    def stop(self):
        if self.sink is not None:
            self.sink.stop(); self.sink.deleteLater(); self.sink = None
        self._an_timer.stop()
        self.stream.pos = 0; self.stream.ended = False; self.stream.seek_req = None
        self.btn_play.setText("Play")
        self.playing.emit(False)
        if self.x is not None:
            self._show_static()

    def set_volume(self, v):
        self.volume = v
        if self.sink is not None:
            self.sink.setVolume(v)

    def seek_frac(self, v):
        if self.x is None:
            return
        p = int(v / 1000 * len(self.x))
        if self.sink is None:
            self.stream.pos = p
        else:
            self.stream.seek_req = p       # applied in the render call (same thread as process())

    # ---------- analyzer
    def _show_static(self):
        if self._static_in is None:
            return
        st = self.st
        g = 0.0 if st["bypass"] else self.proc.gain_db + st.get("out_gain_db", 0.0)
        out = self._static_in + M.response_db(st, self.graph.fgrid, self.sr) + (g if math.isfinite(g) else 0.0)
        self.graph.set_analyzer(self._static_in, out, "analyzer: song average (grey) / with EQ (cyan)")

    def _analyzer_tick(self):
        s = self.stream
        if s.x is None:
            return
        fi, fo = spectrum_db(s.tap_in, self.graph.fgrid, self.sr), spectrum_db(s.tap_out, self.graph.fgrid, self.sr)
        if self._an_in is None or len(self._an_in) != len(fi):
            self._an_in, self._an_out = fi, fo
        else:   # fast attack, ~300 ms decay
            self._an_in = np.maximum(fi, self._an_in - 1.2); self._an_out = np.maximum(fo, self._an_out - 1.2)
        self.graph.set_analyzer(self._an_in, self._an_out, f"live: in (grey) / out (cyan)   render {s.last_render_ms:.1f} ms")
        if len(self.x):
            if not self.pos.isSliderDown():
                self.pos.setValue(int(1000 * min(s.pos, len(self.x)) / len(self.x)))
        if s.ended and self.sink is not None and self.sink.state().name == "IdleState":
            self.stop()
        self._update_info()

    # ---------- export
    def _export_formats(self):
        from ..postprocess import export_formats
        try:
            return export_formats(self.general or {})
        except Exception:
            return ["wav24"]

    def _refresh_export_label(self):
        """Say what the button will write: one file per ticked format."""
        from ..audio_io import format_label
        ks = self._export_formats()
        lbl = "Export with EQ" + (f" ({len(ks)} files)" if len(ks) > 1 else "")
        self.btn_export.setText(lbl)
        self.btn_export.setToolTip("Write the EQ'd master as " + ", ".join(format_label(k) for k in ks)
                                  + f" (name: {Path(self.master_path).stem if self.master_path else 'song'}_eq<tag><ext>)")

    def export(self):
        if self._exporting:
            return
        if self.x is None or not self.master_path:
            self.info.setText("Master a song first (the EQ works on the finished master).")
            return
        from ..audio_io import report_path_for
        from ..postprocess import finalize_exports, export_notes
        from ..cli import dump_json
        mp = Path(self.master_path)
        st, x, sr, hold, ceil, gen = copy.deepcopy(self.st), self.x, self.sr, self.hold_lufs, self.ceiling, dict(self.general)
        self._exporting = True
        self.btn_export.setEnabled(False); self.btn_export.setText("Exporting…")
        self.info.setText("Exporting the EQ'd master…")

        def work():
            try:
                y, info = M.apply(x.astype(np.float64), sr, st, hold, ceil)
                exports = finalize_exports(y, sr, mp.parent, mp.stem + "_eq", "", gen, ceil)
                info["exports"] = [e for _p, e in exports]
                info["export"] = exports[0][1]
                info["notes"] = info.get("notes", []) + [n for _p, e in exports
                                                         for n in export_notes(e, {"lufs": hold, "ceiling_dbtp": ceil})]
                base = {k: v for k, v in info.items() if k != "exports"}
                for p_, e in exports:                     # one report per written file
                    dump_json({"manual_eq": {**base, "export": e}, "source_master": str(mp)}, report_path_for(p_))
                note = ("  " + " ".join(info["notes"])) if not info.get("converged", True) else ""
                names = ", ".join(p.name for p, _e in exports)
                self._exported.emit(f"Exported {names} ({_fmt(info['lufs'], '.2f')} LUFS, "
                                    f"TP {_fmt(info['true_peak_dbtp'])} dBTP){note}")
            except Exception as e:  # noqa
                self._exported.emit(f"Export failed: {type(e).__name__}: {e}")
        threading.Thread(target=work, daemon=True).start()

    def _on_exported(self, m):
        self._exporting = False
        self.btn_export.setEnabled(True); self._refresh_export_label()
        self.info.setText(m); self.status.emit(m)