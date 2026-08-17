#!/usr/bin/env bash
# event-modeler walkthrough: generates and type-checks a state-change slice
# mirroring Command -> Event -> Projection, leaving the modeled file as residue
# that the graded assertions inspect.
set -euo pipefail
root="${DO_HARNESS_ROOT:?DO_HARNESS_ROOT required}"
cat > "$root/model.rs" << 'RUST'
pub trait Command {
    fn name(&self) -> &'static str;
}
pub trait DomainEvent {}
pub trait Projection {
    type Event: DomainEvent;
    fn apply(&mut self, event: &Self::Event);
}

pub struct CreateUser { pub email: String }
impl Command for CreateUser {
    fn name(&self) -> &'static str { "CreateUser" }
}
pub struct UserCreated { pub email: String }
impl DomainEvent for UserCreated {}

#[derive(Default)]
pub struct UserSummaryViews { pub total: usize }
impl Projection for UserSummaryViews {
    type Event = UserCreated;
    fn apply(&mut self, _event: &Self::Event) {
        self.total += 1;
    }
}

#[cfg(test)]
mod t {
    use super::*;
    #[test]
    fn model_folds_events() {
        let mut v = UserSummaryViews::default();
        v.apply(&UserCreated { email: "a@b.c".into() });
        assert_eq!(v.total, 1);
    }
}
RUST
rustc --edition 2021 --test -o "$root/model_artifact" "$root/model.rs"
rm -f "$root/model_artifact"
