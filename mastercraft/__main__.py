"""`python -m mastercraft` launches the GUI; `python -m mastercraft cli ...` runs the CLI."""
import sys

if len(sys.argv) > 1 and sys.argv[1] == "cli":
    from .cli import main
    sys.exit(main(sys.argv[2:]))
from .gui.app import main
sys.exit(main())
