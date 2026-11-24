"""Frozen pre-implementation snapshot for the sound delegation design."""

_delegations = []  # (delegator, group_id, frozenset(perms))
_group_members = {}  # group_id -> set(subject_id)
_group_membership = {}  # group_id -> set(member_group_id)


def grant_delegation(delegator, group_id, perms):
    if not perms <= effective_permissions(delegator):
        raise PermissionError("delegator does not hold every delegated perm")
    _delegations.append((delegator, group_id, frozenset(perms)))


def revoke_delegation(delegator, group_id, perms):
    _delegations[:] = [
        (d, g, p) for d, g, p in _delegations
        if not (d == delegator and g == group_id and perms <= p)
    ]


def _reachable_groups(group_id, seen=None):
    seen = seen or set()
    if group_id in seen:
        return seen
    seen.add(group_id)
    for member in _group_membership.get(group_id, ()):
        _reachable_groups(member, seen)
    return seen


def effective_permissions(subject_id):
    groups = {g for g, members in _group_members.items() if subject_id in members}
    reachable = set()
    for group_id in groups:
        reachable |= _reachable_groups(group_id)
    perms = set()
    for delegator, group_id, granted in _delegations:
        if group_id in reachable:
            perms |= granted
    return frozenset(perms)
