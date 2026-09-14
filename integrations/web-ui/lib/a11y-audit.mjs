// a11y-audit.mjs — axe-core accessibility sensor for the web-ui pack.
//
// Thin adapter over `@axe-core/playwright`, which is a PEER DEPENDENCY of the
// adopting repo (this library ships no npm manifest; the sensor wrapper in the
// adopting repo resolves axe from its own node_modules, keeping do-harness's
// sensors-without-tooling SKIP policy intact). Violations are normalized into
// the same finding shape as the viewport text audit so one report consumer
// handles both.
//
// Ruleset: WCAG 2.2 AA per the accessibility-auditor policy — axe tags
// `wcag2a`, `wcag2aa`, `wcag21a`, `wcag21aa`, `wcag22aa` (2.2 AA includes the
// 2.5.8 target-size and 2.4.13 focus-appearance additions).

const WCAG_22_AA_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"];

/**
 * Run axe against the current page state and normalize violations.
 * @param {import('playwright').Page} page
 * @param {{ impactFloor?: "minor" | "moderate" | "serious" | "critical" }} [options]
 * @returns {Promise<{ findings: Array<object>, scannedRules: number }>}
 */
export async function auditAccessibility(page, options = {}) {
  let AxeBuilder;
  try {
    ({ AxeBuilder } = await import("@axe-core/playwright"));
  } catch {
    // Mirror do-harness's sensor SKIP policy: tool unavailable is a SKIP, not
    // a failure — the caller decides whether strict mode promotes it.
    return { findings: [], scannedRules: 0, skipped: "axe-core unavailable" };
  }
  const impactOrder = ["minor", "moderate", "serious", "critical"];
  const floor = impactOrder.indexOf(options.impactFloor ?? "serious");
  const results = await new AxeBuilder({ page })
    .withTags(WCAG_22_AA_TAGS)
    .analyze();

  const findings = results.violations
    .filter((v) => impactOrder.indexOf(v.impact ?? "minor") >= floor)
    .flatMap((v) =>
      v.nodes.map((node) => ({
        stage: "a11y",
        rule: v.id,
        impact: v.impact,
        wcag: (v.tags ?? []).filter((t) => t.startsWith("wcag")),
        selector: node.target.join(" "),
        reason: `${v.id}: ${v.help} (impact ${v.impact ?? "unknown"})`,
        html: node.html.slice(0, 200),
      })),
    );
  return { findings, scannedRules: (results.passes ?? []).length };
}
