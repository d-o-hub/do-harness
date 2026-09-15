#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;

use crate::config::{Config, HooksConfig, SensorSpec};

mod advisory;
mod blocked;
mod filters;
mod retry;
mod severity;

fn config_with(specs: &[(&str, &[&str])]) -> Config {
    Config {
        language: None,
        hooks: HooksConfig {
            pre_commit: vec![],
            pre_push: vec![],
        },
        signal_sets: BTreeMap::new(),
        sensors: specs
            .iter()
            .map(|(name, argv)| SensorSpec {
                name: (*name).to_owned(),
                argv: argv.iter().map(|a| (*a).to_owned()).collect(),
                retry: None,
                timeout: None,
                severity: None,
                allow_failure: false,
                transient_exit_codes: vec![],
                artifacts: Vec::new(),
                coverage_inputs: Vec::new(),
                when_changed: vec![],
            })
            .collect(),
        jobs: None,
    }
}
