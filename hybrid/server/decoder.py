"""Force MP3 demuxing into bounded PCM; the CLI only receives validated WAV."""

import struct
import wave

from .audio_guard import inspect_wav
from .errors import ProcessingError

RATE = 48000
CHANNELS = 2
SAMPLE_BYTES = 2


def prepare_wav(job, config, runner):
    source = job.directory / ("upload." + job.request.extension)
    destination = job.directory / "input.wav"
    if job.request.extension == "wav":
        info = inspect_wav(source, config)
        # Rebuild a minimal RIFF, stripping metadata and unneeded chunks before hound.
        with source.open("rb") as original:
            original.seek(12)
            fmt = None
            payload = None
            while fmt is None or payload is None:
                tag, length = struct.unpack("<4sI", original.read(8))
                start = original.tell()
                if tag == b"fmt ":
                    raw = original.read(length)
                    encoding, channels, rate, byte_rate, align, bits = struct.unpack_from("<HHIIHH", raw)
                    if encoding == 0xFFFE:
                        encoding = struct.unpack_from("<I", raw, 24)[0]
                    fmt = struct.pack("<HHIIHH", encoding, channels, rate, byte_rate, align, bits)
                elif tag == b"data":
                    payload = (start, length)
                original.seek(start + length + (length % 2))
            offset, length = payload
            with destination.open("xb") as output:
                output.write(b"RIFF" + struct.pack("<I", 36 + length + (length % 2)) + b"WAVEfmt ")
                output.write(struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", length))
                original.seek(offset)
                remaining = length
                while remaining:
                    if job.cancel.is_set():
                        raise ProcessingError("Processing cancelled")
                    block = original.read(min(65536, remaining))
                    if not block:
                        raise ProcessingError("Truncated WAV")
                    output.write(block)
                    remaining -= len(block)
                if length % 2:
                    output.write(b"\0")
        inspect_wav(destination, config)
        return info
    if config.ffmpeg is None:
        raise ProcessingError("MP3 decoder is unavailable")
    # Decoded f32 footprint is twice this s16 stream. Detect one extra frame rather
    # than accepting a silently truncated input at the duration/memory boundary.
    frames = min(config.max_decoded_bytes // (CHANNELS * 4), RATE * config.max_audio_seconds)
    max_pcm = frames * CHANNELS * SAMPLE_BYTES
    written = 0
    with wave.open(str(destination), "wb") as output:
        output.setparams((CHANNELS, SAMPLE_BYTES, RATE, 0, "NONE", "not compressed"))

        def consume(block):
            nonlocal written
            if written + len(block) > max_pcm:
                raise ProcessingError("Decoded MP3 exceeds audio bounds")
            output.writeframesraw(block)
            written += len(block)

        command = [config.ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin",
                   "-threads", "1", "-filter_threads", "1", "-f", "mp3",
                   "-protocol_whitelist", "file,pipe", "-i", source,
                   "-map", "0:a:0", "-vn", "-sn", "-dn", "-ac", str(CHANNELS),
                   "-ar", str(RATE), "-c:a", "pcm_s16le", "-threads", "1",
                   "-t", f"{(frames + 1) / RATE:.9f}", "-fs", str(max_pcm + 65536),
                   "-f", "s16le", "pipe:1"]
        runner.run(command, job, config.decode_timeout_seconds, consume)
    if written == 0 or written % (CHANNELS * SAMPLE_BYTES):
        raise ProcessingError("Empty or incomplete MP3 decode")
    return inspect_wav(destination, config)
