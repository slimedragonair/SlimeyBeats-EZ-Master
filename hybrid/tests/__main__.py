"""Run all regressions, requiring an explicit real native executable for smoke QA."""

import argparse
import os
from pathlib import Path
import unittest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cli", type=Path, required=True, help="Real packaged native CLI; no simulated mastering")
    parser.add_argument("--ffmpeg", type=Path)
    parser.add_argument("--studio-cli", type=Path, help="Optional real v2 Studio worker for full remote-project integration")
    args = parser.parse_args()
    os.environ["HYBRID_TEST_CLI"] = str(args.cli.resolve(strict=True))
    if args.ffmpeg:
        os.environ["HYBRID_TEST_FFMPEG"] = str(args.ffmpeg.resolve(strict=True))
    if args.studio_cli:
        os.environ["HYBRID_TEST_STUDIO_CLI"] = str(args.studio_cli.resolve(strict=True))
    suite = unittest.defaultTestLoader.discover(str(Path(__file__).parent), top_level_dir=str(Path(__file__).parents[2]))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
