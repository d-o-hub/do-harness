---
name: event-modeler
description: >
  Design deterministic, type-safe event-sourced slices following Event Modeling
  paradigms (Commands, Events, Projections). Use when defining domain commands,
  events, handlers, or projections, or when writing Given-When-Then contracts.
license: MIT
metadata:
  version: "0.1.0"
  tags: event-modeling eventsourcing commands projections
---

# Event Modeler Skill

## Purpose
Design deterministic, type-safe event-sourced slices following Event Modeling
paradigms, with a hard 500 LOC per file ceiling.

## Schema Guidelines

### 1. State-Change Slice (Write Side)
`Command -> Handler -> Event(s)`:
- Define the command (intent) with typed fields.
- Define the events (facts) it emits; events are immutable and serializable.
- Keep invalid states unrepresentable: typed enums and strict deserialization.

### 2. State-View Slice (Read Side)
`Event(s) -> Projection -> Query`:
- Define a projection that folds events into a view.
- Handlers must reject events that do not belong to the stream.

## Given-When-Then Contracts
- **Given**: initial event history.
- **When**: a command is dispatched.
- **Then**: specific events are emitted and read models updated.
- Verify the fixture fails for the expected reason (red) before implementing
  (green).

## Gotchas
- Events are facts — never mutate or retract them in place.
- Prefer small slice modules; decompose before reaching 450 LOC.