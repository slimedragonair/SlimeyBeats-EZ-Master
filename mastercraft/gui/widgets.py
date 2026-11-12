"""Custom QPainter widgets: spectrum, waveform, drop-enabled queue table."""
import math

import numpy as np
from PySide6.QtCore import Qt, Signal, QPointF, QRectF
from PySide6.QtGui import QPainter, QPen, QColor, QPainterPath, QFont, QBrush
from PySide6.QtWidgets import QWidget, QTableWidget, QAbstractItemView, QHeaderView

BG = QColor(24, 26, 31)
GRID = QColor(55, 60, 70)
TXT = QColor(170, 175, 185)
C_BEFORE = QColor(240, 150, 60)
C_AFTER = QColor(80, 190, 255)
C_TARGET = QColor(140, 220, 120)


class SpectrumView(QWidget):
    """Log-frequency spectrum: before, after and genre target curve."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(110)
        self.curves = {}
        self.target = None

    def set_data(self, before=None, after=None, target=None):
        self.curves = {}
        if before:
            self.curves["Before"] = (np.asarray(before["freqs"]), np.asarray(before["db"]), C_BEFORE)
        if after:
            self.curves["After"] = (np.asarray(after["freqs"]), np.asarray(after["db"]), C_AFTER)
        self.target = target
        self.update()

    def paintEvent(self, ev):
        p = QPainter(self)
        p.setRenderHint(QPainter.Antialiasing)
        r = self.rect().adjusted(44, 10, -10, -24)
        p.fillRect(self.rect(), BG)
        f0, f1 = 20.0, 20000.0
        allv = np.concatenate([c[1] for c in self.curves.values()]) if self.curves else np.array([-90.0, 0.0])
        top = math.ceil((np.max(allv) + 3) / 10) * 10
        bot = top - 80
        fx = lambda f: r.left() + r.width() * (math.log10(max(f, f0)) - math.log10(f0)) / (math.log10(f1) - math.log10(f0))
        fy = lambda d: r.top() + r.height() * (top - d) / (top - bot)
        p.setFont(QFont("Segoe UI", 8))
        p.setPen(QPen(GRID, 1))
        for f in (20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000):
            x = fx(f)
            p.setPen(QPen(GRID, 1)); p.drawLine(QPointF(x, r.top()), QPointF(x, r.bottom()))
            p.setPen(TXT); p.drawText(QPointF(x - 10, r.bottom() + 15), f"{f // 1000}k" if f >= 1000 else str(f))
        for d in range(int(bot), int(top) + 1, 10):
            y = fy(d)
            p.setPen(QPen(GRID, 1)); p.drawLine(QPointF(r.left(), y), QPointF(r.right(), y))
            p.setPen(TXT); p.drawText(QPointF(4, y + 4), f"{d}")
        for name, (fr, db, col) in self.curves.items():
            path = QPainterPath()
            for i, (f, d) in enumerate(zip(fr, db)):
                pt = QPointF(fx(f), fy(max(min(d, top), bot)))
                path.moveTo(pt) if i == 0 else path.lineTo(pt)
            p.setPen(QPen(col, 2)); p.drawPath(path)
        if self.target is not None and self.curves:
            fr, db = self.target
            path = QPainterPath()
            for i, (f, d) in enumerate(zip(fr, db)):
                pt = QPointF(fx(f), fy(max(min(d, top), bot)))
                path.moveTo(pt) if i == 0 else path.lineTo(pt)
            pen = QPen(C_TARGET, 1.5, Qt.DashLine)
            p.setPen(pen); p.drawPath(path)
        # legend
        lx = r.right() - 210
        items = [(n, c[2]) for n, c in self.curves.items()] + ([("Genre target", C_TARGET)] if self.target is not None else [])
        for i, (n, c) in enumerate(items):
            p.setPen(QPen(c, 3)); p.drawLine(QPointF(lx + 70 * i, r.top() + 8), QPointF(lx + 70 * i + 14, r.top() + 8))
            p.setPen(TXT); p.drawText(QPointF(lx + 70 * i + 18, r.top() + 12), n)
        if not self.curves:
            p.setPen(TXT); p.drawText(r, Qt.AlignCenter, "Spectrum appears after analysis")
        p.end()


class WaveformView(QWidget):
    """Min/max envelope of before (orange) and after (blue); click to seek."""
    seek = Signal(float)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setMinimumHeight(70)
        self.env = {}
        self.pos = None
        self.duration = 0.0

    def set_data(self, before=None, after=None, duration=0.0):
        self.env = {}
        if before is not None:
            self.env["Before"] = (np.asarray(before), C_BEFORE)
        if after is not None:
            self.env["After"] = (np.asarray(after), C_AFTER)
        self.duration = duration
        self.update()

    def set_position(self, sec):
        self.pos = sec
        self.update()

    def mousePressEvent(self, e):
        if self.duration > 0:
            frac = min(max((e.position().x() - 8) / max(self.width() - 16, 1), 0), 1)
            self.seek.emit(frac * self.duration)

    def paintEvent(self, ev):
        p = QPainter(self)
        p.fillRect(self.rect(), BG)
        r = self.rect().adjusted(8, 6, -8, -6)
        names = list(self.env)
        if not names:
            p.setPen(TXT); p.drawText(r, Qt.AlignCenter, "Waveform appears after analysis (drop files on the queue)")
            p.end(); return
        lane_h = r.height() / len(names)
        for li, nm in enumerate(names):
            e, col = self.env[nm]
            mid = r.top() + lane_h * (li + 0.5)
            half = lane_h * 0.45
            n = e.shape[0]
            p.setPen(QPen(col, 1))
            w = r.width()
            for px in range(int(w)):
                i = int(px * n / w)
                lo, hi = e[i]
                p.drawLine(QPointF(r.left() + px, mid - hi * half), QPointF(r.left() + px, mid - lo * half))
            p.setPen(QPen(GRID, 1, Qt.DashLine))
            for lvl in (1.0, -1.0):
                p.drawLine(QPointF(r.left(), mid - lvl * half), QPointF(r.right(), mid - lvl * half))
            p.setPen(TXT); p.drawText(QPointF(r.left() + 4, mid - half + 12), nm)
        if self.pos is not None and self.duration > 0:
            x = r.left() + r.width() * self.pos / self.duration
            p.setPen(QPen(QColor(255, 255, 255), 1)); p.drawLine(QPointF(x, r.top()), QPointF(x, r.bottom()))
        p.end()


class QueueTable(QTableWidget):
    """Batch queue with drag & drop of files/folders."""
    files_dropped = Signal(list)
    COLS = ["File", "Status", "Progress", "In LUFS", "Out LUFS", "Out TP", "Top issue"]

    def __init__(self, parent=None):
        super().__init__(0, len(self.COLS), parent)
        self.setHorizontalHeaderLabels(self.COLS)
        self.setAcceptDrops(True)
        self.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.verticalHeader().setVisible(False)
        h = self.horizontalHeader()
        h.setSectionResizeMode(0, QHeaderView.Stretch)
        for i in range(1, len(self.COLS)):
            h.setSectionResizeMode(i, QHeaderView.ResizeToContents)
        self.viewport().setAcceptDrops(True)
        self.setDragDropMode(QAbstractItemView.DropOnly)

    def dragEnterEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dragMoveEvent(self, e):
        if e.mimeData().hasUrls():
            e.acceptProposedAction()

    def dropEvent(self, e):
        paths = [u.toLocalFile() for u in e.mimeData().urls() if u.isLocalFile()]
        if paths:
            self.files_dropped.emit(paths)
            e.acceptProposedAction()

    def paintEvent(self, ev):
        super().paintEvent(ev)
        if self.rowCount() == 0:
            p = QPainter(self.viewport())
            p.setPen(TXT)
            p.drawText(self.viewport().rect(), Qt.AlignCenter, "Drag & drop songs or folders here\n(WAV, FLAC, MP3, OGG, AIFF)")
            p.end()


def envelope(x, points=1600):
    """Per-bucket min/max of the channel-mean, shape (points, 2)."""
    m = x.mean(axis=1) if x.ndim == 2 else x
    n = m.size
    points = max(1, min(points, n))
    k = n // points
    mm = m[: k * points].reshape(points, k)
    return np.stack([mm.min(axis=1), mm.max(axis=1)], axis=1)
