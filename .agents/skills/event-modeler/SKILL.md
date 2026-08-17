---
name: event-modeler
description: >
  Design deterministic, type-safe event-sourced slices following Event Modeling
  paradigms (Commands, Events, Projections) with a hard 500 LOC per file
  ceiling. Use when defining domain commands, events, handlers, or projections,
  writing Given-When-Then contracts, or modeling state-change versus state-view
  slices.
license: MIT
metadata:
  version: "1.1"
  tags: event-modeling eventsourcing commands projections serde thiserror
---

# Event Modeler Skill

## Purpose
Design deterministic, type-safe event-sourced slices following Event Modeling paradigms (Commands, Events, Projections) with a hard constraint of 500 LOC per file.

## Schema Guidelines

### 1. State-Change Slice (Write Side)
```rust
// In src/domain/<slice_name>/commands.rs
#[derive(Debug, Clone, PartialEq)]
pub struct CreateUserCommand {
    pub user_id: String,
    pub email: String,
}

// In src/domain/<slice_name>/events.rs
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum UserEvent {
    UserCreated { user_id: String, email: String, created_at: i64 },
}
```

### 2. State-View Slice (Read Side)
```rust
// In src/domain/<slice_name>/projections.rs
pub struct UserSummaryView {
    pub total_users: usize,
    pub last_registered_user: Option<String>,
}

impl UserSummaryView {
    pub fn apply(&mut self, event: &UserEvent) {
        match event {
            UserEvent::UserCreated { user_id, .. } => {
                self.total_users += 1;
                self.last_registered_user = Some(user_id.clone());
            }
        }
    }
}
```

## Typed Errors
Define typed error enums with `thiserror` before writing handlers:

```rust
// In src/domain/<slice_name>/errors.rs
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("user {user_id} already exists")]
    AlreadyExists { user_id: String },
    #[error("invalid email: {0}")]
    InvalidEmail(String),
}
```

## Quality Checklist
- Command handlers must be pure functions or stateless orchestrators.
- All domain events must implement `serde::Serialize`/`serde::Deserialize` for libSQL persistence; add `#[serde(deny_unknown_fields)]` to reject stale or unexpected payloads.
- Errors: `thiserror` enums in libraries; `anyhow` only in binaries. No `unwrap()` in library code.
- `#![forbid(unsafe_code)]` at crate roots.
- Maximum file size: 500 LOC. Split files across `commands.rs`, `events.rs`, `handlers.rs`, and `projections.rs` if needed.

## Gotchas
- Events are immutable facts: never mutate an emitted event after the fact; correct via a new event.
- Keep event payloads flat and serde-compatible — nested types complicate libSQL persistence and versioning.