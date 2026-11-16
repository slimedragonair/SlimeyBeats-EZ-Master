"""Bound total request reading, including peers that keep trickling header bytes."""

from http.client import LineTooLong
import time


class DeadlineReader:
    def __init__(self, stream, connection, seconds):
        self.stream = stream
        self.connection = connection
        self.deadline = time.monotonic() + seconds
        self.pending = bytearray()
        self.header_bytes = 0

    def _remaining_time(self):
        remaining = self.deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError("Request read deadline exceeded")
        self.connection.settimeout(remaining)

    def _receive(self, size):
        self._remaining_time()
        # BufferedReader.read/readline can perform many underlying receives, each
        # refreshing a socket timeout. read1 performs at most one, so we recheck
        # the absolute deadline after every receive even when data keeps arriving.
        return self.stream.read1(size)

    def readline(self, size=65537):
        while True:
            self._remaining_time()
            newline = self.pending.find(b"\n")
            count = min(newline + 1, size) if newline >= 0 else min(len(self.pending), size)
            if newline >= 0 or len(self.pending) >= size:
                line = bytes(self.pending[:count])
                del self.pending[:count]
                self.header_bytes += len(line)
                if self.header_bytes > 65536:
                    raise LineTooLong("request headers")
                return line
            block = self._receive(min(8192, size - len(self.pending)))
            if not block:
                line = bytes(self.pending)
                self.pending.clear()
                return line
            self.pending.extend(block)

    def read(self, size):
        self._remaining_time()
        result = bytearray(self.pending[:size])
        del self.pending[:len(result)]
        while len(result) < size:
            block = self._receive(min(65536, size - len(result)))
            if not block:
                break
            result.extend(block)
        return bytes(result)

    def close(self):
        self.stream.close()
