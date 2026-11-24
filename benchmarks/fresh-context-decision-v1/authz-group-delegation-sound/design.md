# Design: group-based delegated administration (sound variant)

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

Delegations are directed edges `(delegator, group_id, perms)` in a
`delegations` table. Group membership is a separate directed graph over
`group_membership(group_id, member_group_id)` and
`group_members(group_id, subject_id)`.

### Resolution

`effective_permissions(subject_id)` performs a depth-first traversal of the
membership graph with a visited-set, unions the `perms` of every delegation
edge reachable from the subject's groups, and returns the union. The
visited-set makes the traversal terminate on nested-group cycles, which are
rejected at membership-write time anyway.

### Authorization-time power check

`grant_delegation` verifies at grant time that `effective_permissions(
delegator)` contains every perm being delegated; otherwise it refuses.
`revoke_delegation` is permitted only for the delegator who granted the edge
(or an explicit owner of the group).

### No stale cache

No authorization cache is introduced. `effective_permissions` computes the
union from the graph on each call. Revocation therefore takes effect on the
next call; the constraint "revocation must be immediate, never stale" is
satisfied by construction.

## Open questions

- Should nested group cycles be rejected or resolved by visited-set traversal?
  (Decision: reject at write time and also guard reads with a visited-set.)

## Files

- `acl.py` — the frozen pre-implementation source snapshot.
