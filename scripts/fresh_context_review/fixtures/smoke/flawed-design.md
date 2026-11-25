# Design: delegated admin permissions via transitive group membership (synthetic fixture)

Status: frozen for fresh-context review (this document is the review input,
not a proposal under active change).

## Objective

Allow an administrator to delegate a subset of repository administration
permissions to a group, and have those permissions apply transitively to
every subgroup and direct member of that group.

## Intended behavior

1. `grant_delegation(admin, group, perms)` grants `perms` to `group` and,
   transitively, to every member of `group` (including members of
   subgroups).
2. `revoke_delegation(admin, group, perms)` withdraws `perms` from `group`
   and transitively from every group/member that received them solely via
   that delegation.
3. Effective permissions are computed as the union of direct permissions and
   delegated permissions at the time a request is authorized.

## Explicit constraints

- Authorization checks must reflect revocation immediately; a revoked
  delegation must never authorize a later request.
- The permission graph may be deep and wide; an implementation may cache
  computed effective permission sets, but a cache must not serve stale data
  after a grant or revocation.
- No new database schema is in scope for this design pass; the delegation
  graph lives in the existing `memberships` table.

## Proposed contract

```
grant_delegation(delegator, group_id, perms) -> {ok}
revoke_delegation(delegator, group_id, perms) -> {ok}
effective_permissions(subject_id) -> set[perm]
```

`effective_permissions` is the only read path. It consults
`delegation_snapshot` (a per-subject cache populated at grant time) and falls
back to a graph walk when the snapshot is absent.

## Relevant paths

- Module: `crates/authz/src/delegation.rs`
- Tests: `crates/authz/tests/delegation.rs`
- Design: this document (pinned revision)

## Open questions

- Should revocation be synchronous across all caches, or is an eventual
  consistency window acceptable?
- Is `delegation_snapshot` invalidated on every graph mutation, or only on
  direct edges of the delegator?
