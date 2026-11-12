"""Optional numba acceleration (numba is already present as a librosa dependency).
Falls back to plain Python if unavailable."""
try:
    from numba import njit as _njit

    def njit(fn):
        return _njit(cache=False, fastmath=False)(fn)
    HAVE_NUMBA = True
except Exception:  # pragma: no cover
    HAVE_NUMBA = False

    def njit(fn):
        return fn
