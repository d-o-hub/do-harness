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

use std::sync::Arc;

use guardian_proxy::{ProxyConfig, ProxyMediator, create_router};

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
    let bind = mediator.bind().to_string();
    let upstream = mediator.upstream().to_string();
    println!(
        "guardian-proxy: bind={bind} upstream={upstream} agent_id={} (agt-governance: {})",
        config.agent_id,
        if cfg!(feature = "agt-governance") {
            "enabled"
        } else {
            "stub"
        }
    );
    let mediator = Arc::new(mediator);
    let router = create_router(mediator);
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("guardian-proxy: listening on {bind}, forwarding allowed calls to {upstream}");
    axum::serve(listener, router).await?;
    Ok(())
}
