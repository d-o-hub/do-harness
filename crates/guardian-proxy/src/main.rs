//! Guardian proxy binary — optional HTTP sidecar.
//!
//! Off by default. When run, it binds `ProxyConfig::bind`, checks each
//! incoming tool call via `ProxyMediator::decide` (fail-closed), and
//! forwards allowed calls to `ProxyConfig::upstream`. Without
//! `agt-governance` the mediator is permissive (stub).

#![forbid(unsafe_code)]

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use guardian_proxy::{ProxyConfig, ProxyMediator};

#[derive(Debug, Parser)]
#[command(
    name = "guardian-proxy",
    about = "Optional fail-closed guardian proxy (adjacent to do-harness)",
    version
)]
struct Cli {
    /// Path to proxy config JSON or TOML.
    #[arg(long, default_value = "guardian-proxy.toml")]
    config: PathBuf,
}

/// Loads proxy config from JSON or TOML.
///
/// # Errors
///
/// Returns an error if the file cannot be read or parsed.
fn load_config(path: &std::path::Path) -> Result<ProxyConfig> {
    let raw = std::fs::read_to_string(path)?;
    // Try JSON, then TOML.
    if let Ok(cfg) = serde_json::from_str::<ProxyConfig>(&raw) {
        return Ok(cfg);
    }
    let cfg: ProxyConfig = toml::from_str(&raw)?;
    Ok(cfg)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = load_config(&cli.config)?;
    let mediator = ProxyMediator::new(config.clone())?;
    println!(
        "guardian-proxy: bind={} upstream={} agent_id={} (agt-governance: {})",
        mediator.bind(),
        mediator.upstream(),
        config.agent_id,
        if cfg!(feature = "agt-governance") {
            "enabled"
        } else {
            "stub"
        }
    );
    // v1: no actual HTTP bind yet — validates config + mediator wiring.
    // Next slice will add axum Router. Exit 0 so `cargo run -p guardian-proxy`
    // proves the crate is well-formed in CI.
    Ok(())
}
