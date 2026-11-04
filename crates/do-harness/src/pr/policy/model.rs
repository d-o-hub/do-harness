//! Data models and schemas for PR merge policy evaluation.

use serde::{Deserialize, Serialize};

/// Expected PR merge policy loaded from `plans/pr-merge-policy.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrMergePolicy {
    pub schema_version: u32,
    pub ruleset_id: u64,
    pub name: String,
    pub target: String,
    pub source_type: String,
    pub source: String,
    pub enforcement: String,
    pub include_refs: Vec<String>,
    #[serde(default)]
    pub exclude_refs: Vec<String>,
    #[serde(default)]
    pub bypass_actors: Vec<BypassActorConfig>,
    pub current_user_can_bypass: String,
    pub required_status_checks: RequiredStatusChecksPolicy,
    pub pull_request: PullRequestPolicy,
    #[serde(default)]
    pub code_scanning_tools: Vec<CodeScanningToolPolicy>,
    #[serde(default)]
    pub code_quality: Option<CodeQualityPolicy>,
    #[serde(default)]
    pub other_rules: Option<OtherRulesPolicy>,
    #[serde(default)]
    pub findings: Option<FindingsPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BypassActorConfig {
    #[serde(default)]
    pub actor_id: Option<i64>,
    pub actor_type: String,
    pub bypass_mode: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredStatusCheckItem {
    pub context: String,
    #[serde(default)]
    pub integration_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_field_names)]
pub struct RequiredStatusChecksPolicy {
    pub strict_required_status_checks_policy: bool,
    pub do_not_enforce_on_create: bool,
    pub required: Vec<RequiredStatusCheckItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::struct_excessive_bools)]
pub struct PullRequestPolicy {
    pub required_approving_review_count: u32,
    pub required_review_thread_resolution: bool,
    pub dismiss_stale_reviews_on_push: bool,
    #[serde(default)]
    pub required_reviewers: Vec<String>,
    pub require_code_owner_review: bool,
    pub require_last_push_approval: bool,
    pub require_extra_approval_for_unattributed_changes: bool,
    pub allowed_merge_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeScanningToolPolicy {
    pub tool: String,
    pub security_alerts_threshold: String,
    pub alerts_threshold: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeQualityPolicy {
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtherRulesPolicy {
    #[serde(default)]
    pub deletion: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingsPolicy {
    pub policy: String,
    #[serde(default)]
    pub max_new_issues_per_run: Option<u32>,
    #[serde(default)]
    pub sources: Vec<FindingsSourceConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FindingsSourceConfig {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub version_source: Option<String>,
    #[serde(default)]
    pub workflow: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

/// Overall policy decision verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PolicyDecision {
    Pass,
    Drift,
    Unknown,
}

/// Structured policy finding describing a drift or evaluation blocker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyFinding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<u64>,
    pub rule_type: String,
    pub field: String,
    pub expected: String,
    pub actual: String,
    pub remediation: String,
}

/// Evaluated report emitted by `doctor pr-policy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrPolicyReport {
    pub schema_version: u32,
    pub decision: PolicyDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ruleset_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_sha256: Option<String>,
    pub findings: Vec<PolicyFinding>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RulesetSummary {
    pub(crate) id: u64,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct RulesetDetail {
    pub(crate) id: u64,
    #[serde(default)]
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) enforcement: Option<String>,
    #[serde(default)]
    pub(crate) conditions: Option<RulesetConditions>,
    #[serde(default)]
    pub(crate) rules: Vec<RulesetRule>,
    #[serde(default)]
    pub(crate) bypass_actors: Vec<RulesetBypassActor>,
    #[serde(default)]
    pub(crate) current_user_can_bypass: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RulesetConditions {
    #[serde(default)]
    pub(crate) ref_name: Option<RefNameCondition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RefNameCondition {
    #[serde(default)]
    pub(crate) include: Vec<String>,
    #[serde(default)]
    pub(crate) exclude: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RulesetRule {
    #[serde(rename = "type")]
    pub(crate) rule_type: String,
    #[serde(default)]
    pub(crate) parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BranchRule {
    #[serde(rename = "type")]
    pub(crate) rule_type: String,
    #[serde(default)]
    pub(crate) ruleset_id: Option<u64>,
    #[serde(default)]
    pub(crate) ruleset_source: Option<String>,
    #[serde(default)]
    pub(crate) ruleset_source_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub(crate) struct RulesetBypassActor {
    #[serde(default)]
    pub(crate) actor_id: Option<i64>,
    #[serde(default)]
    pub(crate) actor_type: Option<String>,
    #[serde(default)]
    pub(crate) bypass_mode: Option<String>,
}
