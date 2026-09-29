//! Arguments for `do-harness learn`.
//!
//! Split from `cli.rs` to keep that file under the modularity cap.

use clap::Args;

use crate::learn;
use crate::report::Format;

/// Arguments for `learn --draft`.
#[derive(Debug, Args)]
pub struct LearnArgs {
    /// Draft the steering actions without applying anything (required: `learn` writes nothing).
    #[arg(long, required = true)]
    pub draft: bool,
    /// Window in days the fires are counted over.
    #[arg(long, value_name = "DAYS", default_value_t = learn::DEFAULT_WINDOW_DAYS)]
    pub days: i64,
    /// Fires a sensor needs to appear in the draft.
    #[arg(long, value_name = "N", default_value_t = learn::DEFAULT_MIN_FIRES)]
    pub min_fires: u32,
    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Text)]
    pub format: Format,
}
