"""Public errors contain fixed messages, never exception text or private paths."""


class ApiError(Exception):
    def __init__(self, status, message):
        super().__init__(message)
        self.status = status
        self.message = message


class ProcessingError(Exception):
    """An internal processing failure with a deliberately generic public result."""
