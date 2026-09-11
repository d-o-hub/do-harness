//! Skill-eval runner for `do-harness eval`.

#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod tests;

mod bless;
mod gate;
mod grading;
mod orchestrator;

pub use orchestrator::run_eval;
