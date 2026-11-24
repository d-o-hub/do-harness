"""Frozen pre-implementation snapshot for the routine trimming control."""


def strip_label(value):
    if value is None:
        return None
    return value.strip()
