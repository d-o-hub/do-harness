// visual-audit.mjs — visual-regression sensor for the web-ui pack.
//
// Captures per route x viewport screenshots (animations disabled, caret
// hidden — the standard flake controls) and compares them against blessed
// baselines stored as PNG files under the workspace's `.do-harness/visual/`
// directory. Comparison is digest-based (SHA-256), so the sensor needs zero
// image dependencies; per-pixel diffing with thresholds can layer on top
// later (the severity/ratchet proposal in #66 is where bless/quarantine
// policy belongs).
//
// Classification is pure and headlessly testable: `cellKey` and
// `classifyBaseline` decide everything except the pixel bytes themselves.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

/** Baseline directory used when none is provided. */
export const DEFAULT_BASELINE_DIR = ".do-harness/visual";

/**
 * Deterministic, filesystem-safe key for one route x viewport cell. Route
 * hashes keep arbitrary paths (including ones with query strings) from
 * colliding or producing unsafe filenames.
 * @param {string} route
 * @param {{ label: string, width: number, height: number }} viewport
 */
export function cellKey(route, viewport) {
  const slug = viewport.label.replace(/[^a-z0-9-]+/gi, "-").replace(/^-+|-+$/g, "");
  const routeHash = createHash("sha256").update(route).digest("hex").slice(0, 12);
  return `${slug || "viewport"}-${routeHash}`;
}

/**
 * Pure classification of a capture against its baseline.
 * @returns {"new" | "unchanged" | "changed"}
 */
export function classifyBaseline({ hasBaseline, baselineDigest, currentDigest }) {
  if (!hasBaseline || !baselineDigest) return "new";
  return baselineDigest === currentDigest ? "unchanged" : "changed";
}

/** SHA-256 digest of a PNG buffer. */
export function digestOf(buffer) {
  return `sha256:${createHash("sha256").update(buffer).digest("hex")}`;
}

/**
 * Capture, compare, and (optionally) bless one visual cell.
 *
 * When a cell has no baseline yet it classifies as `new` — a finding that
 * warns (per #66 policy) rather than fails, so the first run seeds baselines
 * instead of blocking. An actual digest mismatch against a blessed baseline
 * classifies as `changed` and is a failure.
 *
 * @param {import('playwright').Page} page — already navigated to the route.
 * @param {{ route: string, viewport: {label,width,height}, baselineDir?: string,
 *           update?: boolean, fullPage?: boolean }} options
 */
export async function auditVisual(page, options) {
  const buffer = await page.screenshot({
    animations: "disabled",
    caret: "hide",
    fullPage: options.fullPage ?? true,
  });
  const currentDigest = digestOf(buffer);
  const key = cellKey(options.route, options.viewport);
  const baselineDir = options.baselineDir ?? DEFAULT_BASELINE_DIR;
  const baselinePath = join(baselineDir, `${key}.png`);
  const hasBaseline = existsSync(baselinePath);
  const baselineDigest = hasBaseline ? digestOf(readFileSync(baselinePath)) : undefined;
  const status = classifyBaseline({ hasBaseline, baselineDigest, currentDigest });

  if (status === "new" || (status === "changed" && options.update)) {
    mkdirSync(baselineDir, { recursive: true });
    writeFileSync(baselinePath, buffer);
  }

  const finding =
    status === "unchanged"
      ? null
      : {
          stage: "visual",
          selector: "page",
          route: options.route,
          viewport: options.viewport.label,
          reason:
            status === "new"
              ? `no baseline yet — blessed on this run (${baselinePath}); rerun to confirm`
              : `screenshot differs from blessed baseline (${baselinePath}); rerun with WEB_VISUAL_UPDATE=1 only after verifying the change is intended`,
          status,
          currentDigest,
          baselineDigest,
        };
  return { key, status, currentDigest, baselineDigest, finding };
}
