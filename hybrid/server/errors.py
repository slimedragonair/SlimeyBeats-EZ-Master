"""Public errors contain fixed messages, never exception text or private paths."""


class ApiError(Exception):
    def __init__(self, status, message, retry_after=None):
        super().__init__(message)
        self.status = status
        self.message = message
        self.retry_after = retry_after


class ProcessingError(Exception):
    """An internal processing failure with a deliberately generic public result."""
