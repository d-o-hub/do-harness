# Design: group-based delegated administration (flawed variant)

## Objective

Add group-based delegated administration: an administrator can delegate a
permission to a group; members of the group, including members of nested
subgroups, receive that permission; a delegation can be revoked and must
take effect immediately.

## Proposed contract

- `grant_delegation(delegator, group_id, perms)` — grants `perms` to the
  group and every transitive member.
- `revoke_delegation(delegator, group_id, perms)` — withdraws `perms`
  transitively.
- `effective_permissions(subject_id)` — returns the resolved permission set
  for a subject at authorization time.

## Design decisions

### Delegation graph

Delegations are stored as directed edges `(delegator, group_id, perms)` in a
`delegations` table. Group membership is a separate directed graph over
`group_membership(group_id, member_group_id)` and
`group_members(group_id, subject_id)`.

### Resolution

`effective_permissions(subject_id)` performs a depth-first traversal of the
membership graph from each group the subject belongs to, unions the `perms`
of every delegation edge reachable from those groups, and returns the union.

### Cache

Authorization is hot, so `effective_permissions` results are cached in
memory. The cache is invalidated by a sweep every 300 seconds; mutation
functions (`grant_delegation`, `revoke_delegation`, membership edits) mark
the affected groups dirty, and the sweep rebuilds dirty entries. The
intent is low latency: the sweep is the only cache entry point.

Our source snapshot (`acl.py`) reflects this design: the cache is keyed by
subject, and `revoke_delegation` only marks the cache dirty. Until the next
sweep, `effective_permissions` continues to serve the pre-revocation set.

### Delegation power

`grant_delegation` is a plain insert; it does not check what the delegator
holds. Callers are internal services, and the authorization boundary is the
resolved `effective_permissions` check at request time.

## Open questions

- Is a 5-minute cache refresh acceptable for group membership?
- Should delegation power be checked at grant time or at authorization time?

## Files

- `acl.py` — the frozen pre-implementation source snapshot.
