"""Validate complete RIFF WAVs without loading their declared sample payload."""

from dataclasses import dataclass
import math
import struct

from .errors import ProcessingError


@dataclass(frozen=True)
class WavInfo:
    channels: int
    sample_rate: int
    frames: int
    decoded_bytes: int


def inspect_wav(path, config):
    size = path.stat().st_size
    with path.open("rb") as stream:
        header = stream.read(12)
        if len(header) != 12 or header[:4] != b"RIFF" or header[8:] != b"WAVE":
            raise ProcessingError("Only complete RIFF PCM/float WAV is supported")
        riff_end = struct.unpack_from("<I", header, 4)[0] + 8
        if riff_end != size:
            raise ProcessingError("Invalid WAV size")
        fmt = None
        data = None
        chunks = 0
        while stream.tell() < riff_end:
            chunks += 1
            if chunks > 128:
                raise ProcessingError("Too many WAV chunks")
            chunk = stream.read(8)
            if len(chunk) != 8:
                raise ProcessingError("Truncated WAV chunk")
            tag, length = struct.unpack("<4sI", chunk)
            start = stream.tell()
            if start + length + (length % 2) > riff_end:
                raise ProcessingError("WAV chunk exceeds file size")
            if tag == b"fmt ":
                if fmt is not None or not 16 <= length <= 64:
                    raise ProcessingError("Invalid WAV format chunk")
                raw = stream.read(length)
                encoding, channels, rate, byte_rate, align, bits = struct.unpack_from("<HHIIHH", raw)
                if encoding == 0xFFFE:
                    if length < 40 or struct.unpack_from("<H", raw, 16)[0] < 22:
                        raise ProcessingError("Invalid extensible WAV")
                    valid_bits = struct.unpack_from("<H", raw, 18)[0]
                    guid = raw[24:40]
                    if guid[4:] != bytes.fromhex("00001000800000aa00389b71") or valid_bits != bits:
                        raise ProcessingError("Unsupported extensible WAV")
                    encoding = struct.unpack_from("<I", guid)[0]
                if (encoding not in (1, 3) or channels not in (1, 2)
                        or not 8000 <= rate <= 384000
                        or (encoding == 1 and bits not in (8, 16, 24, 32))
                        or (encoding == 3 and bits != 32)
                        or align != channels * (bits // 8) or byte_rate != rate * align):
                    raise ProcessingError("Unsupported PCM/float WAV layout")
                fmt = (encoding, channels, rate, align)
            elif tag == b"data":
                if data is not None:
                    raise ProcessingError("Multiple WAV data chunks are unsupported")
                data = (start, length)
            stream.seek(start + length + (length % 2))
        if fmt is None or data is None:
            raise ProcessingError("WAV needs format and data chunks")
        encoding, channels, rate, align = fmt
        start, length = data
        frames = length // align
        decoded_bytes = frames * channels * 4
        if (length == 0 or length % align or decoded_bytes > config.max_decoded_bytes
                or frames > rate * config.max_audio_seconds):
            raise ProcessingError("Decoded WAV exceeds frame or memory bounds")
        if encoding == 3:
            stream.seek(start)
            remaining = length
            while remaining:
                block = stream.read(min(65536, remaining))
                if not block or len(block) % 4 or any(not math.isfinite(x[0]) for x in struct.iter_unpack("<f", block)):
                    raise ProcessingError("Nonfinite or truncated float WAV")
                remaining -= len(block)
    return WavInfo(channels, rate, frames, decoded_bytes)
