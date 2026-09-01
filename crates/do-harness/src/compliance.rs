//! `do-harness compliance`: embedded control-catalog crosswalk.
//!
//! Catalog: crates/do-harness/compliance/controls.json (schema-versioned).
//! Only mechanisms do-harness actually implements are catalogued; unknown
//! frameworks or malformed entries fail closed (AGENTS.md invariant).

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

pub const CATALOG: &str = include_str!("../compliance/controls.json");

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Framework {
    /// OWASP Agentic AI Top 10
    OwaspAgenticTop10,
    /// NIST AI RMF 1.0
    NistAiRmf,
    /// EU AI Act
    EuAiAct,
    /// SOC 2 (AICPA Trust Services Criteria)
    Soc2,
}

impl Framework {
    pub fn slug(self) -> &'static str {
        match self {
            Framework::OwaspAgenticTop10 => "owasp-agentic-top10",
            Framework::NistAiRmf => "nist-ai-rmf",
            Framework::EuAiAct => "eu-ai-act",
            Framework::Soc2 => "soc2",
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Catalog {
    pub catalog_version: u32,
    pub controls: Vec<Control>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Control {
    pub id: String,
    pub name: String,
    pub mechanism: String,
    pub frameworks: Vec<Mapping>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Mapping {
    pub framework: String,
    pub category: String,
}

pub fn load() -> anyhow::Result<Catalog> {
    serde_json::from_str(CATALOG)
        // Fail-closed: a malformed embedded catalog is a build defect, not a
        // runtime warning.
        .map_err(|e| anyhow::anyhow!("embedded compliance catalog is invalid: {e}"))
}

pub fn filter_catalog(framework: Option<Framework>) -> anyhow::Result<Catalog> {
    let cat = load()?;
    let Some(fw) = framework else {
        return Ok(cat);
    };

    let slug = fw.slug();
    let controls = cat
        .controls
        .into_iter()
        .filter(|c| c.frameworks.iter().any(|m| m.framework == slug))
        .collect();

    Ok(Catalog {
        catalog_version: cat.catalog_version,
        controls,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_catalog_loads_and_validates() {
        let cat = load().expect("embedded catalog must parse");
        assert_eq!(cat.catalog_version, 1);
        assert!(!cat.controls.is_empty());

        let valid_frameworks = [
            Framework::OwaspAgenticTop10.slug(),
            Framework::NistAiRmf.slug(),
            Framework::EuAiAct.slug(),
            Framework::Soc2.slug(),
        ];

        for control in &cat.controls {
            assert!(!control.id.is_empty());
            assert!(!control.name.is_empty());
            assert!(!control.mechanism.is_empty());
            assert!(!control.frameworks.is_empty());

            for mapping in &control.frameworks {
                assert!(
                    valid_frameworks.contains(&mapping.framework.as_str()),
                    "unknown framework slug: {}",
                    mapping.framework
                );
                assert!(!mapping.category.is_empty());
            }
        }
    }

    #[test]
    fn test_filter_catalog_by_framework() {
        for fw in [
            Framework::OwaspAgenticTop10,
            Framework::NistAiRmf,
            Framework::EuAiAct,
            Framework::Soc2,
        ] {
            let filtered = filter_catalog(Some(fw)).expect("filter catalog");
            assert_eq!(filtered.catalog_version, 1);
            assert!(
                !filtered.controls.is_empty(),
                "framework {:?} had no controls",
                fw
            );
            for control in &filtered.controls {
                assert!(control.frameworks.iter().any(|m| m.framework == fw.slug()));
            }
        }
    }
}
