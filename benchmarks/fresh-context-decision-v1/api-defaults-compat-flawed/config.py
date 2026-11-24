"""Frozen pre-implementation snapshot for the flawed API-compat design."""

_DEFAULT_TIMEOUT_S = 5


class Connection:
    def __init__(self, host, port, timeout_s):
        self.host = host
        self.port = port
        self.timeout_s = timeout_s


def connect(host, port, timeout=_DEFAULT_TIMEOUT_S):
    # The legacy timeout_seconds alias was removed in this release.
    return Connection(host, port, timeout)
