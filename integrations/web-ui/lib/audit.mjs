// audit.mjs — orchestrator for the viewport text audit. Thin: normalizes the
// viewport matrix, injects lib/page-probe.mjs per cell, and attaches route/
// viewport context to findings. The heavy lifting happens in the probe.
//
// Usage from a Playwright test or sensor wrapper:
//   import { auditPage, DEFAULT_VIEWPORT_MATRIX } from "./lib/audit.mjs";
//   const report = await auditPage(page, { route: "/login", viewport });
//   if (report.findings.length) throw new Error(JSON.stringify(report, null, 2));

import { pageProbe } from "./page-probe.mjs";

/**
 * Default 2026 viewport matrix: WCAG 1.4.10 reflow floor (320) through large
 * desktop, with the Android-majority 360 and Pixel 412 columns and the
 * Tailwind `sm` breakpoint boundary (640) that flips header layouts.
 */
export const DEFAULT_VIEWPORT_MATRIX = [
  { label: "mobile-sm", width: 320, height: 568 },
  { label: "mobile-md", width: 360, height: 800 },
  { label: "mobile-lg", width: 390, height: 844 },
  { label: "mobile-xl", width: 412, height: 915 },
  { label: "tablet-sm", width: 640, height: 960 },
  { label: "tablet", width: 768, height: 1024 },
  { label: "tablet-lg", width: 820, height: 1180 },
  { label: "laptop", width: 1024, height: 768 },
  { label: "desktop", width: 1280, height: 720 },
  { label: "desktop-lg", width: 1440, height: 900 },
  { label: "large-desktop", width: 1920, height: 1080 },
  { label: "landscape-mobile", width: 812, height: 375 },
];

/** Validate + fill a matrix entry; throws on malformed input. */
export function normalizeViewport(entry) {
  if (!entry || typeof entry !== "object") {
    throw new TypeError("viewport entry must be an object");
  }
  const width = Number(entry.width);
  const height = Number(entry.height);
  if (!Number.isInteger(width) || width < 200 || width > 7680) {
    throw new RangeError(`viewport width out of range: ${entry.width}`);
  }
  if (!Number.isInteger(height) || height < 200 || height > 4320) {
    throw new RangeError(`viewport height out of range: ${entry.height}`);
  }
  return {
    label: String(entry.label ?? `${width}x${height}`),
    width,
    height,
  };
}

/** @returns {Array<{label, width, height}>} normalized matrix. */
export function normalizeMatrix(matrix = DEFAULT_VIEWPORT_MATRIX) {
  const normalized = matrix.map(normalizeViewport);
  if (normalized.length === 0) throw new RangeError("viewport matrix is empty");
  return normalized;
}

/**
 * Run the text audit on an already-loaded page at a single viewport.
 * Returns a serializable report; findings carry route/viewport context.
 * @param {import('playwright').Page} page
 * @param {{ route: string, viewport: {label, width, height}, allowlist?: string[], maxFindings?: number, checkFocusVisibility?: boolean }} options
 */
export async function auditPage(page, options) {
  const viewport = normalizeViewport(options.viewport);
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  if (options.route !== undefined) await page.goto(options.route, { waitUntil: "networkidle" });
  const probe = await page.evaluate(pageProbe, {
    allowlist: options.allowlist ?? [],
    maxFindings: options.maxFindings ?? 50,
    checkFocusVisibility: options.checkFocusVisibility ?? true,
    viewport: { width: viewport.width, height: viewport.height },
  });
  return {
    route: options.route ?? page.url(),
    viewport,
    textLeaves: probe.textLeaves,
    truncated: probe.truncated,
    findings: probe.findings.map((f) => ({ ...f, route: options.route ?? page.url(), viewport: viewport.label })),
  };
}

/**
 * Run the audit across a full route x viewport matrix.
 * @param {import('playwright').Page} page
 * @param {{ routes: string[], matrix?: Array<{label,width,height}>, allowlist?: string[] }} options
 */
export async function auditMatrix(page, options) {
  const matrix = normalizeMatrix(options.matrix);
  const reports = [];
  for (const route of options.routes) {
    await page.goto(route, { waitUntil: "networkidle" });
    for (const viewport of matrix) {
      reports.push(await auditPage(page, { route, viewport, allowlist: options.allowlist }));
    }
  }
  return {
    matrix: matrix.map((m) => m.label),
    routes: options.routes,
    cells: reports,
    findingCount: reports.reduce((n, r) => n + r.findings.length, 0),
  };
}
