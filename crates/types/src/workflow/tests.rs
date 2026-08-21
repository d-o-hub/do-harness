//! Tests for the workflow commands, events, and board projection.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use crate::event::ProjectionError;
use crate::workflow::*;

/// A fresh board starts with every counter at zero.
#[test]
fn task_board_starts_empty() {
    assert_eq!(TaskBoard::new().to_counts(), (0, 0, 0, 0));
}

/// Folding a stream reflects each task's latest state exactly once.
#[test]
fn task_board_folds_events() {
    let mut board = TaskBoard::new();
    board
        .apply(&WorkflowEvent::TaskAdded(TaskAdded {
            id: 1,
            title: "a".to_owned(),
            method: None,
        }))
        .unwrap();
    board
        .apply(&WorkflowEvent::TaskAdded(TaskAdded {
            id: 2,
            title: "b".to_owned(),
            method: None,
        }))
        .unwrap();
    board
        .apply(&WorkflowEvent::TaskAdvanced(TaskAdvanced {
            id: 2,
            subtask_index: 1,
        }))
        .unwrap();
    board
        .apply(&WorkflowEvent::TaskCompleted(TaskCompleted { id: 1 }))
        .unwrap();
    board
        .apply(&WorkflowEvent::TaskFailed(TaskFailed { id: 3 }))
        .unwrap();

    // Task 1 done, task 2 in progress, task 3 failed; no pending remain.
    assert_eq!(board.to_counts(), (0, 1, 1, 1));
    assert_eq!(board.pending(), 0);
    assert_eq!(board.in_progress(), 1);
    assert_eq!(board.done(), 1);
    assert_eq!(board.failed(), 1);
}

/// Replaying the same stream is idempotent: duplicate events never
/// double-count a task.
#[test]
fn task_board_replay_is_idempotent() {
    let stream = [
        WorkflowEvent::TaskAdded(TaskAdded {
            id: 1,
            title: "a".to_owned(),
            method: None,
        }),
        WorkflowEvent::TaskAdvanced(TaskAdvanced {
            id: 1,
            subtask_index: 1,
        }),
        WorkflowEvent::TaskCompleted(TaskCompleted { id: 1 }),
    ];
    let mut once = TaskBoard::new();
    for event in &stream {
        once.apply(event).unwrap();
    }
    let mut twice = TaskBoard::new();
    for event in stream.iter().chain(stream.iter()) {
        twice.apply(event).unwrap();
    }
    assert_eq!(once, twice);
    assert_eq!(twice.to_counts(), (0, 0, 1, 0));
}

/// Fold order across different tasks does not change the final board.
#[test]
fn task_board_is_order_independent_across_tasks() {
    let added = |id| {
        WorkflowEvent::TaskAdded(TaskAdded {
            id,
            title: "t".to_owned(),
            method: None,
        })
    };
    let forward = [added(1), added(2), added(3)];
    let backward = [added(3), added(2), added(1)];
    let mut a = TaskBoard::new();
    let mut b = TaskBoard::new();
    for event in &forward {
        a.apply(event).unwrap();
    }
    for event in &backward {
        b.apply(event).unwrap();
    }
    assert_eq!(a.to_counts(), (3, 0, 0, 0));
    assert_eq!(a, b);
}

/// Commands expose their stable names.
#[test]
fn command_names() {
    assert_eq!(
        AddTask {
            title: "t".to_owned(),
            method: None,
            parent_id: None,
            precondition: None,
        }
        .name(),
        "AddTask"
    );
    assert_eq!(AdvanceTask { task_id: 1 }.name(), "AdvanceTask");
    assert_eq!(CompleteTask { task_id: 1 }.name(), "CompleteTask");
    assert_eq!(FailTask { task_id: 1 }.name(), "FailTask");
}

/// The wrapper event delegates its name to the inner event.
#[test]
fn wrapped_event_delegates_name() {
    let event = WorkflowEvent::TaskAdded(TaskAdded {
        id: 1,
        title: "t".to_owned(),
        method: None,
    });
    assert_eq!(event.name(), "TaskAdded");
}

/// Concrete events round-trip through JSON.
#[test]
fn events_roundtrip_json() {
    let event = WorkflowEvent::TaskAdvanced(TaskAdvanced {
        id: 7,
        subtask_index: 2,
    });
    let json = serde_json::to_string(&event).unwrap();
    let parsed: WorkflowEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, event);
}

/// Stale payloads carrying unknown fields are rejected at the wrapper
/// level: `deny_unknown_fields` must hold even through the internally
/// tagged enum's buffered deserialization.
#[test]
fn unknown_fields_are_rejected_through_wrapper() {
    let stale = r#"{"kind":"TaskCompleted","id":1,"bogus":true}"#;
    assert!(serde_json::from_str::<WorkflowEvent>(stale).is_err());
    let stale_nested = r#"{"kind":"TaskAdded","id":1,"title":"t","method":null,"extra":1}"#;
    assert!(serde_json::from_str::<WorkflowEvent>(stale_nested).is_err());
    // The tag itself is part of the contract too.
    let untagged = r#"{"id":1}"#;
    assert!(serde_json::from_str::<WorkflowEvent>(untagged).is_err());
}

/// A projection rejects an event outside its stream with `UnsupportedEvent`.
#[test]
fn projection_rejects_unmatched_event() {
    /// Counter restricted to `TaskAdded` events only.
    #[derive(Debug, Default)]
    struct AddedCount {
        n: i64,
    }

    impl Projection for AddedCount {
        type Event = WorkflowEvent;

        fn apply(&mut self, event: &Self::Event) -> Result<(), ProjectionError> {
            match event {
                Self::Event::TaskAdded(_) => {
                    self.n += 1;
                    Ok(())
                }
                other => Err(ProjectionError::UnsupportedEvent(other.name())),
            }
        }
    }

    let mut board = AddedCount::default();
    board
        .apply(&WorkflowEvent::TaskAdded(TaskAdded {
            id: 1,
            title: "t".to_owned(),
            method: None,
        }))
        .unwrap();
    let err = board
        .apply(&WorkflowEvent::TaskAdvanced(TaskAdvanced {
            id: 1,
            subtask_index: 1,
        }))
        .unwrap_err();
    assert!(matches!(
        err,
        ProjectionError::UnsupportedEvent("TaskAdvanced")
    ));
    assert_eq!(
        err.to_string(),
        "event TaskAdvanced cannot be applied to this projection"
    );
}
