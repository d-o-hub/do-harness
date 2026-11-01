// audit.mjs — orchestrator for the viewport text audit. Thin: normalizes the
// viewport matrix, injects lib/page-probe.mjs per cell, and attaches route/
// viewport context to findings. The heavy lifting happens in the probe.
//
// Usage from a Playwright test or sensor wrapper:
//   import { auditPage, DEFAULT_VIEWPORT_MATRIX } from "./lib/audit.mjs";
//   const report = await auditPage(page, { route: "/login", viewport });
//   if (report.findings.length) throw new Error(JSON.stringify(report, null, 2));

import { pageProbe } from "./page-probe.mjs";
import { annotateFindings } from "./annotate.mjs";
import { cellKey } from "./visual-audit.mjs";

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

/** Schemes audited by default: a defect that appears only in dark mode is
 * invisible to a light-only run. */
export const DEFAULT_SCHEME_MATRIX = ["light", "dark"];

const SUPPORTED_SCHEMES = new Set(DEFAULT_SCHEME_MATRIX);

/** Validate + lowercase one color scheme; throws on an unsupported value. */
export function normalizeScheme(scheme) {
  if (typeof scheme !== "string") {
    throw new TypeError("scheme must be a string");
  }
  const normalized = scheme.trim().toLowerCase();
  if (!SUPPORTED_SCHEMES.has(normalized)) {
    throw new RangeError(`unsupported color scheme: ${scheme}`);
  }
  return normalized;
}

/** @returns {string[]} validated, duplicate-free scheme matrix. */
export function normalizeSchemeMatrix(matrix = DEFAULT_SCHEME_MATRIX) {
  if (!Array.isArray(matrix)) {
    throw new TypeError("scheme matrix must be an array");
  }
  const normalized = matrix.map(normalizeScheme);
  if (normalized.length === 0) throw new RangeError("scheme matrix is empty");
  if (new Set(normalized).size !== normalized.length) {
    throw new RangeError("scheme matrix contains duplicates");
  }
  return normalized;
}

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
 * Returns a serializable report; findings carry route/viewport/scheme context.
 * @param {import('playwright').Page} page
 * @param {{ route: string, viewport: {label, width, height}, scheme?: "light" | "dark", allowlist?: string[], maxFindings?: number, checkFocusVisibility?: boolean, annotate?: boolean, findingsDir?: string }} options
 */
export async function auditPage(page, options) {
  const viewport = normalizeViewport(options.viewport);
  const scheme = options.scheme === undefined ? undefined : normalizeScheme(options.scheme);
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  // null restores the browser's own preference so an omitted scheme cannot
  // inherit the previous cell's emulation.
  await page.emulateMedia({ colorScheme: scheme ?? null });
  if (options.route !== undefined) await page.goto(options.route, { waitUntil: "networkidle" });
  const probe = await page.evaluate(pageProbe, {
    allowlist: options.allowlist ?? [],
    maxFindings: options.maxFindings ?? 50,
    checkFocusVisibility: options.checkFocusVisibility ?? true,
    viewport: { width: viewport.width, height: viewport.height },
  });
  const route = options.route ?? page.url();
  const findings = probe.findings.map((f) => ({
    ...f,
    route,
    viewport: viewport.label,
    scheme: scheme ?? null,
  }));
  const annotationArtifacts = [];
  if (options.annotate === true && findings.length > 0) {
    const annotation = await annotateFindings(page, findings, {
      key: cellKey(route, viewport, scheme),
      outDir: options.findingsDir,
    });
    if (annotation?.path) annotationArtifacts.push(annotation.path);
  }
  return {
    route,
    viewport,
    scheme: scheme ?? null,
    textLeaves: probe.textLeaves,
    truncated: probe.truncated,
    findings,
    annotationArtifacts,
  };
}

/**
 * Run the audit across a full route x viewport x scheme matrix.
 * @param {import('playwright').Page} page
 * @param {{ routes: string[], matrix?: Array<{label,width,height}>, schemes?: string[], allowlist?: string[], annotate?: boolean, findingsDir?: string }} options
 */
export async function auditMatrix(page, options) {
  const matrix = normalizeMatrix(options.matrix);
  const schemes = normalizeSchemeMatrix(options.schemes);
  const reports = [];
  for (const route of options.routes) {
    await page.goto(route, { waitUntil: "networkidle" });
    for (const viewport of matrix) {
      for (const scheme of schemes) {
        reports.push(
          await auditPage(page, {
            route,
            viewport,
            scheme,
            allowlist: options.allowlist,
            annotate: options.annotate,
            findingsDir: options.findingsDir,
          }),
        );
      }
    }
  }
  return {
    matrix: matrix.map((m) => m.label),
    schemes,
    routes: options.routes,
    cells: reports,
    findingCount: reports.reduce((n, r) => n + r.findings.length, 0),
    annotationArtifacts: reports.flatMap((r) => r.annotationArtifacts),
  };
}
