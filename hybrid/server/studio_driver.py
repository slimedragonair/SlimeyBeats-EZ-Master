"""Fixed native worker invocation, private JSON and strictly contained output paths."""

import json
import os
from pathlib import Path
import stat

from .errors import ProcessingError
from .native_settings import write_private_json
from .storage import is_link

MAX_RESPONSE_BYTES = 16 * 1024 * 1024


def unique(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("Duplicate native field")
        result[key] = value
    return result


def reject_constant(value):
    raise ValueError("Nonfinite native value")


def safe_file(path, root):
    path = Path(path)
    # Rust canonicalize uses extended Win32 paths; normalize that spelling only,
    # then apply the same containment/link checks as ordinary local paths.
    if os.name == "nt" and str(path).startswith("\\\\?\\"):
        path = Path(str(path)[4:])
    if not path.is_absolute():
        raise ProcessingError("Relative native output")
    root = root.resolve(strict=True)
    try:
        relative = path.relative_to(root)
    except ValueError as error:
        raise ProcessingError("Output escaped staging") from error
    current = root
    for component in relative.parts:
        if component in (".", ".."):
            raise ProcessingError("Unsafe native output")
        current = current / component
        if is_link(current):
            raise ProcessingError("Linked native output")
    details = path.stat()
    if not stat.S_ISREG(details.st_mode) or details.st_nlink != 1 or details.st_size <= 0:
        raise ProcessingError("Invalid native output")
    if not path.resolve(strict=True).is_relative_to(root):
        raise ProcessingError("Output escaped staging")
    return path


class StudioDriver:
    def __init__(self, config, runner):
        self.config, self.runner = config, runner

    def invoke(self, command, request):
        request_path, response_path = command.stage / "request.json", command.stage / "out" / "response.json"
        write_private_json(request_path, request)
        try:
            self.runner.run([self.config.studio_cli, "--request", request_path,
                             "--response", response_path, "--threads", str(self.config.v2_threads)], command,
                            self.config.worker_timeout_seconds)
            safe_file(response_path, command.stage)
            if response_path.stat().st_size > MAX_RESPONSE_BYTES:
                raise ProcessingError("Native response limit")
            try:
                result = json.loads(response_path.read_text(encoding="utf-8"), object_pairs_hook=unique,
                                    parse_constant=reject_constant)
            except (ValueError, UnicodeError, RecursionError) as error:
                raise ProcessingError("Invalid native response") from error
            if (not isinstance(result, dict) or result.get("api_version") != 2
                    or result.get("operation") != request["operation"]):
                raise ProcessingError("Native response contract")
            return result
        finally:
            if not command.unreaped:
                request_path.unlink(missing_ok=True)
                response_path.unlink(missing_ok=True)
