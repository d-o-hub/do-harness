// perf-audit.mjs — Lighthouse performance sensor for the web-ui pack.
//
// The budget evaluation is pure and headlessly testable; the Lighthouse
// invocation is a thin adapter. `lighthouse` is a PEER DEPENDENCY of the
// adopting repo (like @axe-core/playwright) — when absent the audit returns
// a SKIP-shaped result per the do-harness sensor policy.
//
// Thresholds encode the Core Web Vitals "good" boundaries (web.dev/vitals):
// LCP <= 2500ms, CLS <= 0.1, and Lighthouse's lab proxy for INP, TBT,
// <= 600ms ("poor" boundary) with the overall performance category score at
// the 0.9 "good/needs-improvement" crossover by default.

export const DEFAULT_BUDGETS = Object.freeze({
  performanceScore: 0.9,
  lcpMs: 2500,
  cls: 0.1,
  tbtMs: 600,
});

/**
 * Parse and validate a budgets JSON string (or object). Unknown keys and
 * non-numeric values fail loudly — a typo like `lcpMs: "2500"` must not
 * silently disable a budget.
 * @param {string | Record<string, number>} input
 */
export function parseBudgets(input) {
  let raw = input;
  if (typeof input === "string") {
    try {
      raw = JSON.parse(input);
    } catch {
      throw new TypeError("budgets JSON is not parseable");
    }
  }
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    throw new TypeError("budgets must be an object");
  }
  const merged = { ...DEFAULT_BUDGETS };
  for (const [key, value] of Object.entries(raw)) {
    if (!(key in DEFAULT_BUDGETS)) {
      throw new TypeError(`unknown budget key: ${key}`);
    }
    if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
      throw new TypeError(`budget ${key} must be a finite non-negative number`);
    }
    merged[key] = value;
  }
  return merged;
}

/**
 * Pure comparison of extracted Lighthouse metrics against budgets.
 * @param {{ performanceScore?: number|null, lcpMs?: number|null, cls?: number|null, tbtMs?: number|null }} metrics
 * @param {typeof DEFAULT_BUDGETS} budgets
 * @returns {Array<object>} findings in the shared shape (empty when clean).
 */
export function evaluateBudgets(metrics, budgets = DEFAULT_BUDGETS) {
  const findings = [];
  const checks = [
    {
      key: "performanceScore",
      label: "performance category score",
      actual: metrics.performanceScore,
      limit: budgets.performanceScore,
      worse: (m, limit) => m !== null && m !== undefined && m < limit,
      detail: (m, limit) => `score ${m} < budget ${limit}`,
    },
    {
      key: "lcpMs",
      label: "Largest Contentful Paint",
      actual: metrics.lcpMs,
      limit: budgets.lcpMs,
      worse: (m, limit) => m !== null && m !== undefined && m > limit,
      detail: (m, limit) => `LCP ${Math.round(m)}ms > budget ${limit}ms (good <= 2500ms)`,
    },
    {
      key: "cls",
      label: "Cumulative Layout Shift",
      actual: metrics.cls,
      limit: budgets.cls,
      worse: (m, limit) => m !== null && m !== undefined && m > limit,
      detail: (m, limit) => `CLS ${m} > budget ${limit} (good <= 0.1)`,
    },
    {
      key: "tbtMs",
      label: "Total Blocking Time",
      actual: metrics.tbtMs,
      limit: budgets.tbtMs,
      worse: (m, limit) => m !== null && m !== undefined && m > limit,
      detail: (m, limit) => `TBT ${Math.round(m)}ms > budget ${limit}ms`,
    },
  ];
  for (const check of checks) {
    if (check.worse(check.actual, check.limit)) {
      findings.push({
        stage: "perf",
        selector: "page",
        rule: check.key,
        reason: `${check.label}: ${check.detail(check.actual, check.limit)}`,
        actual: check.actual,
        budget: check.limit,
      });
    }
  }
  return findings;
}

/**
 * Run Lighthouse against one URL and evaluate budgets.
 * @param {{ url: string, budgets?: string | Record<string, number>, chromePath?: string }} options
 * @returns {Promise<{ findings: Array<object>, metrics: object } | { skipped: string }>}
 */
export async function auditPerformance(options) {
  let lighthouse;
  try {
    lighthouse = (await import("lighthouse")).default;
  } catch {
    return { skipped: "lighthouse is not installed in this workspace" };
  }
  const budgets = parseBudgets(options.budgets ?? {});
  if (options.chromePath) {
    process.env.CHROME_PATH = options.chromePath;
  }
  const result = await lighthouse(options.url, {
    onlyCategories: ["performance"],
    chromeFlags: ["--headless=new", "--no-sandbox", "--disable-dev-shm-usage"],
  });
  const audits = result.lhr.audits;
  const metrics = {
    performanceScore: result.lhr.categories.performance?.score ?? null,
    lcpMs: audits["largest-contentful-paint"]?.numericValue ?? null,
    cls: audits["cumulative-layout-shift"]?.numericValue ?? null,
    tbtMs: audits["total-blocking-time"]?.numericValue ?? null,
  };
  const findings = evaluateBudgets(metrics, budgets).map((f) => ({
    ...f,
    route: options.url,
  }));
  return { findings, metrics };
}
