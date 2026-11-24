"""Frozen pre-implementation snapshot for the flawed delegation design."""

import time

_SWEEP_INTERVAL_S = 300
_delegations = []  # (delegator, group_id, frozenset(perms))
_group_members = {}  # group_id -> set(subject_id)
_group_membership = {}  # group_id -> set(member_group_id)
_cache = {}  # subject_id -> (expires_at, frozenset(perms))


def grant_delegation(delegator, group_id, perms):
    # NOTE: no check that delegator currently holds `perms`.
    _delegations.append((delegator, group_id, frozenset(perms)))
    _cache.clear()


def revoke_delegation(delegator, group_id, perms):
    _delegations[:] = [
        (d, g, p) for d, g, p in _delegations
        if not (d == delegator and g == group_id and perms <= p)
    ]
    _cache.clear()  # only marks dirty; entries still served until sweep


def _reachable_groups(group_id, seen=None):
    seen = seen or set()
    if group_id in seen:
        return seen
    seen.add(group_id)
    for member in _group_membership.get(group_id, ()):
        _reachable_groups(member, seen)
    return seen


def _effective(subject_id):
    groups = {g for g, members in _group_members.items() if subject_id in members}
    reachable = set()
    for group_id in groups:
        reachable |= _reachable_groups(group_id)
    perms = set()
    for delegator, group_id, granted in _delegations:
        if group_id in reachable:
            perms |= granted
    return frozenset(perms)


def effective_permissions(subject_id):
    entry = _cache.get(subject_id)
    now = time.monotonic()
    if entry and entry[0] > now:
        return entry[1]
    perms = _effective(subject_id)
    _cache[subject_id] = (now + _SWEEP_INTERVAL_S, perms)
    return perms
