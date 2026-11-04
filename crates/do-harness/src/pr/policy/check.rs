//! Validation functions comparing active rulesets against expected policy.

use std::collections::BTreeSet;

use super::model::{BranchRule, PolicyFinding, PrMergePolicy, RulesetDetail};

pub(crate) fn check_metadata(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    if detail.name != policy.name {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "metadata".to_owned(),
            field: "name".to_owned(),
            expected: policy.name.clone(),
            actual: detail.name.clone(),
            remediation: format!("Update ruleset name to '{}'", policy.name),
        });
    }
    if detail.target.as_deref() != Some(&policy.target) {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "metadata".to_owned(),
            field: "target".to_owned(),
            expected: policy.target.clone(),
            actual: detail.target.clone().unwrap_or_default(),
            remediation: format!("Update ruleset target to '{}'", policy.target),
        });
    }
    if detail.enforcement.as_deref() != Some(&policy.enforcement) {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "metadata".to_owned(),
            field: "enforcement".to_owned(),
            expected: policy.enforcement.clone(),
            actual: detail.enforcement.clone().unwrap_or_default(),
            remediation: format!("Set ruleset enforcement to '{}'", policy.enforcement),
        });
    }
}

pub(crate) fn check_conditions(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    let empty_vec = Vec::new();
    let ref_name = detail.conditions.as_ref().and_then(|c| c.ref_name.as_ref());
    let actual_include = ref_name.map_or(&empty_vec, |r| &r.include);
    let actual_exclude = ref_name.map_or(&empty_vec, |r| &r.exclude);

    if actual_include != &policy.include_refs {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "conditions".to_owned(),
            field: "include_refs".to_owned(),
            expected: format!("{:?}", policy.include_refs),
            actual: format!("{actual_include:?}"),
            remediation: format!("Set include_refs to {:?}", policy.include_refs),
        });
    }
    if actual_exclude != &policy.exclude_refs {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "conditions".to_owned(),
            field: "exclude_refs".to_owned(),
            expected: format!("{:?}", policy.exclude_refs),
            actual: format!("{actual_exclude:?}"),
            remediation: format!("Set exclude_refs to {:?}", policy.exclude_refs),
        });
    }
}

pub(crate) fn check_bypass(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    let has_non_never_bypass = detail
        .bypass_actors
        .iter()
        .any(|b| b.bypass_mode.as_deref() != Some("never"));
    if policy.bypass_actors.is_empty() && has_non_never_bypass {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "bypass_actors".to_owned(),
            field: "bypass_actors".to_owned(),
            expected: "[]".to_owned(),
            actual: format!(
                "{:?}",
                detail
                    .bypass_actors
                    .iter()
                    .map(|b| (&b.actor_type, &b.bypass_mode))
                    .collect::<Vec<_>>()
            ),
            remediation: "Remove all bypass actors from the ruleset".to_owned(),
        });
    }
    if detail.current_user_can_bypass.as_deref() != Some(&policy.current_user_can_bypass) {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "bypass".to_owned(),
            field: "current_user_can_bypass".to_owned(),
            expected: policy.current_user_can_bypass.clone(),
            actual: detail.current_user_can_bypass.clone().unwrap_or_default(),
            remediation: format!("Ensure bypass mode is '{}'", policy.current_user_can_bypass),
        });
    }
}

pub(crate) fn check_rules(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    check_status_checks(policy, detail, findings);
    check_pull_request(policy, detail, findings);
    check_code_scanning(policy, detail, findings);
    check_code_quality(policy, detail, findings);
    check_other_rules(policy, detail, findings);
}

fn check_status_checks(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    let rule = detail
        .rules
        .iter()
        .find(|r| r.rule_type == "required_status_checks");
    let Some(rule) = rule else {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "required_status_checks".to_owned(),
            field: "rule".to_owned(),
            expected: "required_status_checks rule present".to_owned(),
            actual: "rule missing".to_owned(),
            remediation: "Add required_status_checks rule to ruleset".to_owned(),
        });
        return;
    };

    let params = rule.parameters.as_ref();
    let strict = params
        .and_then(|p| p.get("strict_required_status_checks_policy"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let do_not_enforce = params
        .and_then(|p| p.get("do_not_enforce_on_create"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if strict
        != policy
            .required_status_checks
            .strict_required_status_checks_policy
    {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "required_status_checks".to_owned(),
            field: "strict_required_status_checks_policy".to_owned(),
            expected: policy
                .required_status_checks
                .strict_required_status_checks_policy
                .to_string(),
            actual: strict.to_string(),
            remediation: "Enable strict_required_status_checks_policy in ruleset".to_owned(),
        });
    }
    if do_not_enforce != policy.required_status_checks.do_not_enforce_on_create {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "required_status_checks".to_owned(),
            field: "do_not_enforce_on_create".to_owned(),
            expected: policy
                .required_status_checks
                .do_not_enforce_on_create
                .to_string(),
            actual: do_not_enforce.to_string(),
            remediation: "Configure do_not_enforce_on_create in ruleset".to_owned(),
        });
    }

    let actual_checks = params
        .and_then(|p| p.get("required_status_checks"))
        .and_then(|v| v.as_array());
    let mut actual_contexts = BTreeSet::new();
    if let Some(checks) = actual_checks {
        for c in checks {
            if let Some(ctx) = c.get("context").and_then(|v| v.as_str()) {
                actual_contexts.insert(ctx.to_owned());
            }
        }
    }

    for exp in &policy.required_status_checks.required {
        if !actual_contexts.contains(&exp.context) {
            findings.push(PolicyFinding {
                rule_id: Some(policy.ruleset_id),
                rule_type: "required_status_checks".to_owned(),
                field: format!("required[{}]", exp.context),
                expected: format!("context '{}' required", exp.context),
                actual: "context missing from ruleset".to_owned(),
                remediation: format!("Add required status check '{}' to ruleset", exp.context),
            });
        }
    }

    for act in &actual_contexts {
        if !policy
            .required_status_checks
            .required
            .iter()
            .any(|e| &e.context == act)
        {
            findings.push(PolicyFinding {
                rule_id: Some(policy.ruleset_id),
                rule_type: "required_status_checks".to_owned(),
                field: format!("unexpected_check[{act}]"),
                expected: "not in policy required list".to_owned(),
                actual: format!("context '{act}' present in ruleset"),
                remediation: format!("Remove obsolete required check '{act}' from ruleset"),
            });
        }
    }
}

fn check_pull_request(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    let rule = detail.rules.iter().find(|r| r.rule_type == "pull_request");
    let Some(rule) = rule else {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "pull_request".to_owned(),
            field: "rule".to_owned(),
            expected: "pull_request rule present".to_owned(),
            actual: "rule missing".to_owned(),
            remediation: "Add pull_request rule to ruleset".to_owned(),
        });
        return;
    };

    let params = rule.parameters.as_ref();
    let thread_res = params
        .and_then(|p| p.get("required_review_thread_resolution"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if thread_res != policy.pull_request.required_review_thread_resolution {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "pull_request".to_owned(),
            field: "required_review_thread_resolution".to_owned(),
            expected: policy
                .pull_request
                .required_review_thread_resolution
                .to_string(),
            actual: thread_res.to_string(),
            remediation: "Enable required_review_thread_resolution in ruleset".to_owned(),
        });
    }

    let approvals = params
        .and_then(|p| p.get("required_approving_review_count"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(0);
    if approvals != policy.pull_request.required_approving_review_count {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "pull_request".to_owned(),
            field: "required_approving_review_count".to_owned(),
            expected: policy
                .pull_request
                .required_approving_review_count
                .to_string(),
            actual: approvals.to_string(),
            remediation: format!(
                "Set required_approving_review_count to {}",
                policy.pull_request.required_approving_review_count
            ),
        });
    }
}

fn check_code_scanning(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    if policy.code_scanning_tools.is_empty() {
        return;
    }
    let rule = detail.rules.iter().find(|r| r.rule_type == "code_scanning");
    let Some(rule) = rule else {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "code_scanning".to_owned(),
            field: "rule".to_owned(),
            expected: "code_scanning rule present".to_owned(),
            actual: "rule missing".to_owned(),
            remediation: "Add code_scanning rule to ruleset".to_owned(),
        });
        return;
    };

    let tools = rule
        .parameters
        .as_ref()
        .and_then(|p| p.get("code_scanning_tools"))
        .and_then(|v| v.as_array());
    for exp in &policy.code_scanning_tools {
        let found = tools.and_then(|arr| {
            arr.iter()
                .find(|t| t.get("tool").and_then(|v| v.as_str()) == Some(&exp.tool))
        });
        if found.is_none() {
            findings.push(PolicyFinding {
                rule_id: Some(policy.ruleset_id),
                rule_type: "code_scanning".to_owned(),
                field: format!("tool[{}]", exp.tool),
                expected: format!("tool '{}' configured", exp.tool),
                actual: "tool missing".to_owned(),
                remediation: format!("Configure code scanning tool '{}'", exp.tool),
            });
        }
    }
}

fn check_code_quality(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    let Some(cq) = &policy.code_quality else {
        return;
    };
    let rule = detail.rules.iter().find(|r| r.rule_type == "code_quality");
    let Some(rule) = rule else {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "code_quality".to_owned(),
            field: "rule".to_owned(),
            expected: "code_quality rule present".to_owned(),
            actual: "rule missing".to_owned(),
            remediation: "Add code_quality rule to ruleset".to_owned(),
        });
        return;
    };
    let severity = rule
        .parameters
        .as_ref()
        .and_then(|p| p.get("severity"))
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    if severity != cq.severity {
        findings.push(PolicyFinding {
            rule_id: Some(policy.ruleset_id),
            rule_type: "code_quality".to_owned(),
            field: "severity".to_owned(),
            expected: cq.severity.clone(),
            actual: severity.to_owned(),
            remediation: format!("Set code_quality severity to '{}'", cq.severity),
        });
    }
}

fn check_other_rules(
    policy: &PrMergePolicy,
    detail: &RulesetDetail,
    findings: &mut Vec<PolicyFinding>,
) {
    if let Some(other) = &policy.other_rules {
        if other.deletion {
            let has_deletion = detail.rules.iter().any(|r| r.rule_type == "deletion");
            if !has_deletion {
                findings.push(PolicyFinding {
                    rule_id: Some(policy.ruleset_id),
                    rule_type: "deletion".to_owned(),
                    field: "deletion".to_owned(),
                    expected: "deletion rule present".to_owned(),
                    actual: "deletion rule missing".to_owned(),
                    remediation: "Add deletion prevention rule to ruleset".to_owned(),
                });
            }
        }
    }
}

pub(crate) fn check_branch_rules(
    policy: &PrMergePolicy,
    branch_rules: &[BranchRule],
    findings: &mut Vec<PolicyFinding>,
) {
    for rule in branch_rules {
        if rule.ruleset_id != Some(policy.ruleset_id) {
            findings.push(PolicyFinding {
                rule_id: rule.ruleset_id,
                rule_type: rule.rule_type.clone(),
                field: "applicable_ruleset".to_owned(),
                expected: format!("ruleset {}", policy.ruleset_id),
                actual: format!(
                    "unexpected rule from {:?} ({:?}, id {:?})",
                    rule.ruleset_source, rule.ruleset_source_type, rule.ruleset_id
                ),
                remediation: "Review unexpected branch rule or update pr-merge-policy.toml"
                    .to_owned(),
            });
        }
    }
}
