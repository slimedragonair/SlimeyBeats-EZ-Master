"""Paced core frame blocks -> one continuous compressed preview, never a song render."""
import array
import math
import sys
import threading
import time

from .arrangement_native import spawn_owned
from .errors import ProcessingError
from .processes import terminate_and_reap
from .arrangement_preview_formats import PREVIEW_RATE, PREVIEW_PRIMING_FRAMES, encoder_arguments, preview_padding

MAX_ENCODE_AHEAD_SECONDS = 0.15


class ArrangementStream:
    def __init__(self, state, descriptor):
        self.state, self.descriptor = state, descriptor
        self.id = descriptor['stream_id']
        self.lock = threading.RLock()
        self.cancel = threading.Event()
        self.started = False
        self.process = self.feeder = None
        self.failed = False

    def start(self):
        with self.lock:
            if self.started or self.cancel.is_set():
                raise ProcessingError('Arrangement stream expired')
            self.started = True
            config = self.state.config
            self.process = spawn_owned(config, self.state.project.directory, encoder_arguments(config, self.descriptor))
            self.feeder = threading.Thread(target=self._feed, name='arrangement-frame-feeder', daemon=True)
            self.feeder.start()
            return self.process.stdout

    def _feed(self):
        rate, channels = self.descriptor['sample_rate'], self.descriptor['channels']
        first = self.descriptor['start_frame']
        remaining = self.descriptor['side_duration_frames'] - first
        block_frames = min(2048, max(128, rate // 50))
        sent, started = 0, time.monotonic()
        try:
            while remaining > 0 and not self.cancel.is_set():
                delay = sent / rate - (time.monotonic() - started) - MAX_ENCODE_AHEAD_SECONDS
                if delay > 0 and self.cancel.wait(delay):
                    break
                if self.state.closed or self.state.project.deleting or self.state.project.deadline <= time.monotonic():
                    break
                block = self.state.native.call({'operation': 'block', 'frames': min(block_frames, remaining)}, timeout=10)
                count, samples = block.get('frames'), block.get('samples')
                if (type(count) is not int or not 0 <= count <= min(block_frames, remaining)
                        or not isinstance(samples, list) or len(samples) != count * channels
                        or not all(type(value) in (int, float) and math.isfinite(value) for value in samples)):
                    raise ProcessingError('Invalid arrangement frame block')
                if not count:
                    raise ProcessingError('Arrangement frame stream ended early')
                if self.cancel.is_set():
                    break
                pcm = array.array('f', samples)
                if sys.byteorder != 'little':
                    pcm.byteswap()
                self._write_pcm(pcm.tobytes())
                sent += count
                remaining -= count
            if (remaining == 0 and sent > 0 and not self.failed and not self.cancel.is_set()
                    and not self.state.closed and not self.state.project.deleting
                    and self.state.project.deadline > time.monotonic()):
                # Codec transport only: do not advance Rust or change source geometry.
                tail, _ = preview_padding(sent, rate)
                if tail:
                    self._write_pcm(bytes(tail * channels * 4))
        except (OSError, ValueError, ProcessingError):
            self.failed = not self.cancel.is_set()
        finally:
            try:
                self.process.stdin.close()
            except (OSError, ValueError):
                pass

    def _write_pcm(self, data):
        if self.process.stdin.write(data) != len(data):
            raise ProcessingError('Incomplete arrangement encoder input')
        self.process.stdin.flush()

    def close(self):
        with self.lock:
            self.cancel.set()
            if self.process is not None:
                terminate_and_reap(self.process)
                for pipe in (self.process.stdin, self.process.stdout):
                    try:
                        pipe.close()
                    except (OSError, ValueError):
                        pass
            if self.feeder is not None and self.feeder is not threading.current_thread():
                self.feeder.join(timeout=12)
                if self.feeder.is_alive():
                    raise ProcessingError('Arrangement stream feeder is still stopping')
