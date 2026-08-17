//! Event-sourcing base contracts.
//!
//! These traits encode the two event-modeling slice types as type-level
//! contracts: state change (`Command -> Handler -> Event`) and state view
//! (`Event -> Projection -> Query`).

use std::fmt::Debug;

/// A command issued against the write side of a slice.
///
/// Commands are the input of the `Command -> Handler -> Event` pipeline.
pub trait Command: Debug + Send + Sync {
    /// The unique name of the command (e.g., `CreateUser`).
    fn name(&self) -> &'static str;
}

/// A domain event emitted by a handler.
///
/// Events are immutable facts and must serialize for persistence.
pub trait DomainEvent: Debug + Send + Sync {
    /// The unique name of the event (e.g., `UserCreated`).
    fn name(&self) -> &'static str;
}

/// A read-side projection that folds an event stream into a view.
pub trait Projection {
    /// The event type this projection consumes.
    type Event: DomainEvent;

    /// Applies a single event to the projection state.
    ///
    /// # Errors
    ///
    /// Returns an error if the event cannot be applied to the current state.
    fn apply(&mut self, event: &Self::Event) -> Result<(), ProjectionError>;
}

/// Error returned when a projection cannot apply an event.
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    /// The event does not belong to this projection's stream.
    #[error("event {0} cannot be applied to this projection")]
    UnsupportedEvent(&'static str),
}
