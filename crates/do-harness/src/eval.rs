//! Skill-eval runner for `do-harness eval`.

#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod eval_tests;
#[cfg(test)]
mod fixture_tests;
#[cfg(test)]
mod tests;

macro_rules! diag {
    ($format:expr, $($arg:tt)*) => {
        if $format == $crate::report::Format::Json {
            eprintln!($($arg)*);
        } else {
            println!($($arg)*);
        }
    };
}
pub(super) use diag;

mod agent;
mod bless;
mod fixture;
mod gate;
mod grading;
mod opts;
mod orchestrator;

pub use opts::EvalOpts;
pub use orchestrator::run_eval;
