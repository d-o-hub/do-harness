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

use guardian_proxy::{AuditLog, ProxyConfig, ProxyMediator, create_router_degraded};

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
    /// Verify a hash-chained audit log and exit (fail-closed: tamper exits non-zero).
    #[arg(long, value_name = "PATH")]
    verify_audit: Option<PathBuf>,
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
    if let Some(path) = &cli.verify_audit {
        AuditLog::verify(path)?;
        println!("guardian-proxy: audit log {} verified", path.display());
        return Ok(());
    }
    let config = load_config(&cli.config)?;
    let bind = config.bind.clone();
    let router = match ProxyMediator::new(config.clone()) {
        Ok(mediator) => {
            println!(
                "guardian-proxy: bind={bind} upstream={} agent_id={} (agt-governance: {})",
                config.upstream,
                config.agent_id,
                if cfg!(feature = "agt-governance") {
                    "enabled"
                } else {
                    "stub"
                }
            );
            let mediator = Arc::new(mediator);
            let mut state = match &config.audit_log {
                Some(path) => {
                    let audit = AuditLog::open(path)?;
                    println!("guardian-proxy: audit log at {path}");
                    guardian_proxy::AppState::with_audit(mediator, audit)
                }
                None => guardian_proxy::AppState::new(mediator),
            };
            state.metrics_token.clone_from(&config.metrics_token);
            guardian_proxy::create_router_with_state(state)
        }
        Err(err) => {
            // Fail closed but stay observable: /health turns 503 and every
            // tool call is denied until the configuration is fixed.
            eprintln!("guardian-proxy: FAIL-CLOSED: mediator init failed: {err}");
            create_router_degraded(err.to_string())
        }
    };
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("guardian-proxy: listening on {bind}");
    axum::serve(listener, router).await?;
    Ok(())
}
