"""Bounded float WAV pipe sink preserving original decoder rate and channels."""

import struct

from .errors import ProcessingError


class FloatWavSink:
    def __init__(self, output, config):
        self.output, self.config = output, config
        self.header = bytearray()
        self.started = False
        self.riff = False
        self.fmt = None
        self.maximum = self.written = 0
        self.align = 0

    def consume(self, block):
        if self.started:
            self._pcm(block)
            return
        self.header.extend(block)
        if not self.riff:
            if len(self.header) < 12:
                return
            if self.header[:4] != b"RIFF" or self.header[8:12] != b"WAVE":
                raise ProcessingError("Invalid decoder stream")
            del self.header[:12]
            self.riff = True
        while len(self.header) >= 8:
            tag, size = struct.unpack_from("<4sI", self.header)
            if tag == b"data":
                if self.fmt is None:
                    raise ProcessingError("Decoder format missing")
                self.output.write(b"RIFF\0\0\0\0WAVEfmt " + struct.pack("<I", 16) + self.fmt + b"data\0\0\0\0")
                self.started = True
                self._pcm(self.header[8:])
                self.header.clear()
                return
            if size > 4096:
                raise ProcessingError("Decoder header limit")
            extent = 8 + size + size % 2
            if len(self.header) < extent:
                return
            if tag == b"fmt ":
                if self.fmt is not None or size < 16:
                    raise ProcessingError("Decoder format invalid")
                raw = self.header[8:8 + size]
                encoding, channels, rate, _, align, bits = struct.unpack_from("<HHIIHH", raw)
                if encoding == 0xFFFE and size >= 40:
                    encoding = struct.unpack_from("<I", raw, 24)[0]
                if encoding != 3 or bits != 32 or channels not in (1, 2) or not 8000 <= rate <= 384000 or align != channels * 4:
                    raise ProcessingError("Unsupported decoder geometry")
                self.align = align
                self.maximum = min(self.config.max_decoded_bytes // align, rate * self.config.max_audio_seconds) * align
                self.fmt = struct.pack("<HHIIHH", 3, channels, rate, rate * align, align, 32)
            del self.header[:extent]

    def _pcm(self, block):
        if self.written + len(block) > self.maximum:
            raise ProcessingError("Decoded audio limit")
        self.output.write(block)
        self.written += len(block)

    def finish(self):
        if not self.started or not self.written or self.written % self.align:
            raise ProcessingError("Incomplete decoded audio")
        self.output.seek(4)
        self.output.write(struct.pack("<I", self.written + 36))
        self.output.seek(40)
        self.output.write(struct.pack("<I", self.written))
