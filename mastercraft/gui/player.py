"""Loudness-matched A/B player: two synced QMediaPlayers, switching = muting one."""
from PySide6.QtCore import QObject, QUrl, Signal, QTimer
from PySide6.QtMultimedia import QMediaPlayer, QAudioOutput


class ABPlayer(QObject):
    position = Signal(float)
    state = Signal(str)
    finished = Signal()        # the audible player reached the end

    def __init__(self, parent=None):
        super().__init__(parent)
        self.players, self.outs = {}, {}
        for k in ("A", "B"):
            pl = QMediaPlayer(self)
            out = QAudioOutput(self)
            pl.setAudioOutput(out)
            pl.mediaStatusChanged.connect(lambda st, k=k: self._status(k, st))
            self.players[k], self.outs[k] = pl, out
        self.active = "B"
        self.volume = 0.8
        self._t = QTimer(self)
        self._t.setInterval(50)
        self._t.timeout.connect(self._tick)
        self._apply()

    def load(self, before_path, after_path=None):
        self.stop()
        self.players["A"].setSource(QUrl.fromLocalFile(before_path) if before_path else QUrl())
        self.players["B"].setSource(QUrl.fromLocalFile(after_path) if after_path else QUrl())
        self.active = "B" if after_path else "A"
        self._apply()

    def has_after(self):
        return not self.players["B"].source().isEmpty()

    def set_active(self, which):
        if which == "B" and not self.has_after():
            which = "A"
        prev = self.active
        self.active = which
        # the newly audible player continues from where the one you were hearing is (QA L8)
        if prev != which and not self.players[prev].source().isEmpty():
            self.players[which].setPosition(self.players[prev].position())
        self._apply()
        self.state.emit(which)

    def toggle(self):
        self.set_active("A" if self.active == "B" else "B")

    def set_volume(self, v):
        self.volume = v
        self._apply()

    def _apply(self):
        for k, o in self.outs.items():
            o.setVolume(self.volume if k == self.active else 0.0)

    def play(self):
        for k, p in self.players.items():
            if not p.source().isEmpty():
                p.play()
        self._t.start()

    def pause(self):
        for p in self.players.values():
            p.pause()
        self._t.stop()

    def stop(self):
        for p in self.players.values():
            p.stop()
        self._t.stop()
        self.position.emit(0.0)

    def seek(self, sec):
        for p in self.players.values():
            if not p.source().isEmpty():
                p.setPosition(int(sec * 1000))
        self.position.emit(sec)

    def is_playing(self):
        return any(p.playbackState() == QMediaPlayer.PlayingState for p in self.players.values())

    def _status(self, k, st):
        if st == QMediaPlayer.EndOfMedia and k == self.active:
            for p in self.players.values():
                p.stop()
            self._t.stop()
            self.finished.emit()

    def _tick(self):
        p = self.players[self.active]
        self.position.emit(p.position() / 1000.0)
        # keep the muted player within 40 ms of the audible one
        o = self.players["B" if self.active == "A" else "A"]
        if not o.source().isEmpty() and abs(o.position() - p.position()) > 40:
            o.setPosition(p.position())
