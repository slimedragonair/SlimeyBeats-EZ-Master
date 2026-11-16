"""Operator v2 thread control must never alter v1's four-thread contract."""

from dataclasses import replace
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from hybrid.server.config import Config, from_arguments
from hybrid.server.processes import ProcessRunner
from hybrid.server.project_limits import processing_config
from hybrid.server.studio_driver import StudioDriver
from hybrid.tests.support import TOKEN


class ProjectThreadTests(unittest.TestCase):
    def config(self, directory, **settings):
        return Config(cli=Path(sys.executable), studio_cli=Path(sys.executable), token=TOKEN,
                      data_dir=Path(directory), **settings)

    def test_default_bounds_and_operator_flag(self):
        with tempfile.TemporaryDirectory() as directory:
            default = self.config(directory)
            self.assertEqual(default.v2_threads, 4)
            for count in (1, 8, 32):
                self.assertEqual(processing_config(replace(default, v2_threads=count)).worker_threads, count)
            for count in (0, 33, -1, True, 8.0):
                with self.subTest(count=count), self.assertRaises(ValueError):
                    replace(default, v2_threads=count)
            with patch.dict(os.environ, {"SLIMEYBEATS_HYBRID_TOKEN": TOKEN, "SLIMEYBEATS_HYBRID_ORIGINS": ""}):
                configured = from_arguments(["--cli", sys.executable, "--ffmpeg", sys.executable,
                                             "--data-dir", directory, "--v2-threads", "8"])
            self.assertEqual(configured.v2_threads, 8)
            self.assertEqual(configured.capabilities()["api_version"], 1)

    def test_native_driver_uses_only_operator_selected_thread_argument(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            stage = root / "command"
            stage.mkdir()
            config = processing_config(self.config(root, v2_threads=8))
            command = SimpleNamespace(stage=stage, unreaped=False)
            request = {"api_version": 2, "operation": "analyze", "project_root": str(root),
                       "out_dir": str(stage / "out"), "settings": {}}
            observed = []
            class Runner:
                def run(self, arguments, job, timeout):
                    observed.extend(arguments)
                    response = Path(arguments[arguments.index("--response") + 1])
                    response.parent.mkdir()
                    response.write_text(json.dumps({"api_version": 2, "operation": "analyze", "assets": []}))
            StudioDriver(config, Runner()).invoke(command, request)
            self.assertEqual(observed[0], config.studio_cli)
            self.assertEqual(observed[observed.index("--threads") + 1], "8")
            self.assertFalse((stage / "request.json").exists())
            self.assertFalse((stage / "out" / "response.json").exists())

    def test_supervised_v1_environment_stays_four_and_v2_uses_operator_threads(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            config = self.config(root, v2_threads=8)
            code = "import json,os;print(json.dumps([os.environ['RAYON_NUM_THREADS'],os.environ['OMP_NUM_THREADS']]))"
            for actual, expected in ((config, ["4", "4"]), (processing_config(config), ["8", "8"])):
                output = bytearray()
                job = SimpleNamespace(directory=root, cancel=threading.Event(), unreaped=False)
                ProcessRunner(actual).run([sys.executable, "-c", code], job, 5, output.extend)
                self.assertEqual(json.loads(output), expected)
                self.assertFalse(job.unreaped)
