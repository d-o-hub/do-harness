"""Frozen pre-implementation snapshot for the sound API-compat design."""

import warnings

_DEFAULT_TIMEOUT_S = 30


class Connection:
    def __init__(self, host, port, timeout_s):
        self.host = host
        self.port = port
        self.timeout_s = timeout_s


def connect(host, port, timeout=_DEFAULT_TIMEOUT_S, timeout_seconds=None):
    if timeout_seconds is not None:
        warnings.warn("timeout_seconds is deprecated; use timeout",
                      DeprecationWarning, stacklevel=2)
        if timeout != _DEFAULT_TIMEOUT_S:
            raise TypeError("pass either timeout or timeout_seconds, not both")
        timeout = timeout_seconds
    return Connection(host, port, timeout)
