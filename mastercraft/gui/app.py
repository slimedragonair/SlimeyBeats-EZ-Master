"""GUI entry point."""
import sys

from ..paths import setup_env


def main(argv=None):
    setup_env()
    from PySide6.QtWidgets import QApplication
    from .main_window import MainWindow
    app = QApplication(sys.argv if argv is None else argv)
    app.setApplicationName("Master-Craft")
    w = MainWindow()
    w.show()
    args = [a for a in (sys.argv[1:] if argv is None else argv[1:]) if not a.startswith("-")]
    if args:
        w.add_paths(args)
    rc = app.exec()
    # tear the window (and its QMediaPlayers) down before QApplication goes away;
    # otherwise Qt Multimedia can fail-fast on interpreter exit (0xC0000409)
    w.deleteLater()
    app.processEvents()
    del w
    return rc


if __name__ == "__main__":
    sys.exit(main())
