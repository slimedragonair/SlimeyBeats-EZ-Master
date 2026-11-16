"""Forced private decoding, compact previews and bounded waveform generation."""

from array import array
from types import SimpleNamespace
import math
import struct
import sys

from .audio_guard import inspect_wav
from .decoder import prepare_wav
from .errors import ProcessingError
from .project_limits import WAVEFORM_BINS
from .pcm_decoder import FloatWavSink
from .storage import directory_bytes

DEMUXERS = {"mp3": "mp3", "flac": "flac", "m4a": "mov", "aac": "aac", "aiff": "aiff", "aif": "aiff", "ogg": "ogg"}


def decode_upload(upload, destination, command, config, runner):
    if upload.extension == "wav":
        # Reuse the complete RIFF validation and metadata stripping used by v1.
        adapter = SimpleNamespace(directory=destination.parent, request=SimpleNamespace(extension="wav"), cancel=command.cancel)
        info = inspect_wav(upload.path, config)
        used = directory_bytes(command.directory, config.max_job_disk_bytes, config.max_storage_files)
        if used + info.decoded_bytes + 64 > config.max_job_disk_bytes:
            raise ProcessingError("Decoded storage reservation")
        # Share the metadata stripping implementation without copying the entire
        # encoded source first. The new input remains private and immutable.
        prepare_wav(adapter, config, runner, source=upload.path)
        return inspect_wav(destination, config)
    if config.ffmpeg is None or upload.extension not in DEMUXERS:
        raise ProcessingError("Decoder unavailable")
    with destination.open("xb") as output:
        sink = FloatWavSink(output, config)
        arguments = [config.ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin", "-threads", "1",
                     "-filter_threads", "1", "-f", DEMUXERS[upload.extension], "-protocol_whitelist", "file,pipe"]
        if upload.extension == "m4a":
            arguments.extend(["-enable_drefs", "0", "-use_absolute_path", "0"])
        arguments.extend(["-i", upload.path, "-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "-1",
                          "-c:a", "pcm_f32le", "-threads", "1", "-fflags", "+bitexact", "-f", "wav", "pipe:1"])
        # No -ar/-ac conversion and no -t/-fs truncation. The sink observes one
        # byte beyond either decoded bound, then the supervisor kills/reaps.
        runner.run(arguments, command, config.decode_timeout_seconds, sink.consume)
        sink.finish()
    return inspect_wav(destination, config)


def preview(path, destination, command, config, runner):
    runner.run([config.ffmpeg, "-hide_banner", "-loglevel", "error", "-nostdin", "-threads", "1",
                "-filter_threads", "1", "-f", "wav", "-protocol_whitelist", "file,pipe", "-i", path,
                "-map", "0:a:0", "-vn", "-sn", "-dn", "-map_metadata", "-1", "-ac", "2", "-ar", "44100",
                "-c:a", "libmp3lame", "-b:a", str(config.v2_preview_kbps) + "k", "-threads", "1", "-write_xing", "1", "-id3v2_version", "0",
                "-f", "mp3", destination], command, config.worker_timeout_seconds)
    if not destination.is_file() or destination.stat().st_size <= 0:
        raise ProcessingError("Preview unavailable")


def waveform(path, info, command):
    # Native editable assets are f32 WAV. Read at most 64 KiB at a time and
    # aggregate all interleaved channels so right-only audio remains visible.
    bins = min(info.frames, WAVEFORM_BINS)
    peaks = []
    channel_peaks = [0.0] * info.channels
    with path.open("rb") as stream:
        stream.seek(12)
        encoding = bits = None
        while True:
            chunk = stream.read(8)
            if len(chunk) != 8:
                raise ProcessingError("Invalid waveform input")
            tag, size = struct.unpack("<4sI", chunk)
            start = stream.tell()
            if tag == b"fmt ":
                raw = stream.read(size)
                encoding, _, _, _, _, bits = struct.unpack_from("<HHIIHH", raw)
                if encoding == 0xFFFE:
                    encoding = struct.unpack_from("<I", raw, 24)[0]
            if tag == b"data":
                break
            stream.seek(start + size + size % 2)
        if encoding != 3 or bits != 32:
            raise ProcessingError("Expected native float waveform input")
        for index in range(bins):
            count = ((index + 1) * info.frames // bins - index * info.frames // bins) * info.channels
            low, high = math.inf, -math.inf
            while count:
                if command.cancel.is_set():
                    raise ProcessingError("Cancelled")
                samples = min(count, 16384)
                block = stream.read(samples * 4)
                if len(block) != samples * 4:
                    raise ProcessingError("Incomplete waveform")
                values = array("f")
                values.frombytes(block)
                if sys.byteorder != "little":
                    values.byteswap()
                low, high = min(low, min(values)), max(high, max(values))
                for channel in range(info.channels):
                    track = values[channel::info.channels]
                    channel_peaks[channel] = max(channel_peaks[channel], abs(min(track)), abs(max(track)))
                count -= samples
            peaks.append([max(-1, min(1, low)), max(-1, min(1, high))])
    return {"duration_seconds": info.frames / info.sample_rate, "peaks": peaks,
            "channel_peaks": channel_peaks}
