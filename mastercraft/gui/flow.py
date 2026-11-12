"""Small flow layout (wraps widgets onto new rows when the window is narrow).

Based on the classic Qt FlowLayout example; used so the Manual EQ toolbar and
similar button rows never force a wide minimum window (v0.2.2 QA: window size).
"""
from PySide6.QtCore import Qt, QRect, QSize, QPoint
from PySide6.QtWidgets import QLayout, QSizePolicy


class FlowLayout(QLayout):
    def __init__(self, parent=None, margin=0, hspacing=6, vspacing=4):
        super().__init__(parent)
        self._items = []
        self._h = hspacing
        self._v = vspacing
        self.setContentsMargins(margin, margin, margin, margin)

    def addItem(self, item):
        self._items.append(item)

    def count(self):
        return len(self._items)

    def itemAt(self, i):
        return self._items[i] if 0 <= i < len(self._items) else None

    def takeAt(self, i):
        return self._items.pop(i) if 0 <= i < len(self._items) else None

    def expandingDirections(self):
        return Qt.Orientations(0)

    def hasHeightForWidth(self):
        return True

    def heightForWidth(self, width):
        return self._do(QRect(0, 0, width, 0), True)

    def setGeometry(self, rect):
        super().setGeometry(rect)
        self._do(rect, False)

    def sizeHint(self):
        return self.minimumSize()

    def minimumSize(self):
        s = QSize()
        for it in self._items:
            if not it.isEmpty():
                s = s.expandedTo(it.minimumSize().expandedTo(it.sizeHint()))
        m = self.contentsMargins()
        return s + QSize(m.left() + m.right(), m.top() + m.bottom())

    def _do(self, rect, test):
        m = self.contentsMargins()
        r = rect.adjusted(m.left(), m.top(), -m.right(), -m.bottom())
        x, y, line = r.x(), r.y(), 0
        for it in self._items:
            if it.isEmpty():
                continue
            sz = it.sizeHint().expandedTo(it.minimumSize())
            nx = x + sz.width() + self._h
            if nx - self._h > r.right() + 1 and line > 0:
                x = r.x(); y += line + self._v; nx = x + sz.width() + self._h; line = 0
            if not test:
                it.setGeometry(QRect(QPoint(x, y), sz))
            x = nx
            line = max(line, sz.height())
        return y + line - rect.y() + m.bottom()