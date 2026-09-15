// annotate.mjs — annotated finding artifacts for the viewport text audit.
//
// The JSON report is the machine contract; a reviewable artefact is what makes
// a finding actionable for a human. `annotateFindings` draws one outline box
// and label per finding straight into the live page (viewport coordinates —
// the frame the probe measured in), screenshots it, then removes the overlays
// again. Artefacts live in their own directory, so the visual baseline digests
// under `.do-harness/visual/` never see the annotations.
//
// Label formatting and box normalization are pure and unit-tested headlessly.
// The drawing helpers are serialized into the page by Playwright, so like
// ./page-probe.mjs they declare everything inside their own bodies.

import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** Artifact directory used when none is provided. */
export const DEFAULT_FINDINGS_DIR = ".do-harness/visual/findings";

/** Annotation cap: a catastrophically broken page must not draw 500 boxes. */
export const MAX_ANNOTATIONS = 20;

const LABEL_MAX = 72;

/**
 * One-line label for a finding: stage, element, and the overlap partner when
 * the stage reports one. Truncated so a 300-character selector path cannot
 * cover the page it is pointing at.
 * @param {{ stage?: string, selector?: string, overlapsWith?: string }} finding
 * @param {{ maxLength?: number }} [options]
 */
export function findingLabel(finding, { maxLength = LABEL_MAX } = {}) {
  const parts = [finding.stage ?? "finding", finding.selector ?? "page"];
  if (finding.overlapsWith) parts.push(`↔ ${finding.overlapsWith}`);
  const label = parts.join(" · ");
  return label.length > maxLength ? `${label.slice(0, maxLength - 1)}…` : label;
}

/**
 * Normalize findings into drawable annotations: rect-bearing stages only
 * (invisible-text, contrast, occluded-text, text-overlap, target-size,
 * focus-visibility), capped at `max`.
 * @param {Array<{ stage?: string, selector?: string, overlapsWith?: string, rect?: {x,y,width,height} }>} findings
 * @param {{ max?: number }} [options]
 * @returns {Array<{ rect: {x,y,width,height}, label: string, stage: string }>}
 */
export function annotationsForFindings(findings, { max = MAX_ANNOTATIONS } = {}) {
  if (!(max > 0)) return [];
  const annotations = [];
  for (const finding of findings ?? []) {
    const rect = finding?.rect;
    if (
      !rect ||
      !Number.isFinite(rect.x) ||
      !Number.isFinite(rect.y) ||
      !Number.isFinite(rect.width) ||
      !Number.isFinite(rect.height) ||
      !(rect.width > 0) ||
      !(rect.height > 0)
    )
      continue;
    annotations.push({
      rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
      label: findingLabel(finding),
      stage: finding.stage ?? "finding",
    });
    if (annotations.length >= max) break;
  }
  return annotations;
}

// --- page-side helpers (serialized by Playwright: no outer references) ---

function isAtScrollOrigin() {
  return window.scrollX === 0 && window.scrollY === 0;
}

function drawAnnotations(annotations) {
  const rootId = "__do-harness-annotations";
  document.getElementById(rootId)?.remove();
  const root = document.createElement("div");
  root.id = rootId;
  root.style.cssText = "position:fixed;inset:0;pointer-events:none;z-index:2147483647";
  for (const { rect, label } of annotations) {
    const box = document.createElement("div");
    box.style.cssText = [
      "position:fixed",
      `left:${rect.x}px`,
      `top:${rect.y}px`,
      `width:${rect.width}px`,
      `height:${rect.height}px`,
      "outline:2px solid #b00020",
      "outline-offset:1px",
      "background:rgba(176,0,32,0.12)",
      "box-sizing:border-box",
    ].join(";");
    const tag = document.createElement("span");
    tag.textContent = label;
    // Rects hugging the top edge would push their label out of frame.
    const tagTop = rect.y < 20 ? `${rect.height}px` : "-18px";
    tag.style.cssText = [
      "position:absolute",
      "left:0",
      `top:${tagTop}`,
      "white-space:nowrap",
      "background:#b00020",
      "color:#fff",
      "font:11px/16px ui-monospace,monospace",
      "padding:0 4px",
      "border-radius:2px",
    ].join(";");
    box.append(tag);
    root.append(box);
  }
  document.body.append(root);
  return annotations.length;
}

function clearAnnotations() {
  document.getElementById("__do-harness-annotations")?.remove();
}

/**
 * Capture one annotated screenshot for a cell's findings.
 *
 * The probe measures in viewport coordinates without scrolling, so the page
 * must still be at the scroll origin; otherwise the boxes would point at the
 * wrong pixels and the call reports a skip instead of writing a misleading
 * artefact.
 *
 * @param {import('playwright').Page} page — live page, same frame the probe ran in
 * @param {Array<object>} findings — findings carrying `rect`
 * @param {{ key: string, outDir?: string }} options
 * @returns {Promise<{ path: string } | { skipped: string } | null>} null when no finding has a rect
 */
export async function annotateFindings(page, findings, options = {}) {
  const annotations = annotationsForFindings(findings);
  if (annotations.length === 0) return null;
  const key = options.key;
  if (typeof key !== "string" || key.length === 0) {
    throw new TypeError("annotateFindings requires a cell key for the artifact filename");
  }
  const atOrigin = await page.evaluate(isAtScrollOrigin);
  if (!atOrigin) {
    return { skipped: "page is scrolled; finding rects use the probe's viewport frame" };
  }
  await page.evaluate(drawAnnotations, annotations);
  try {
    const buffer = await page.screenshot({
      animations: "disabled",
      caret: "hide",
      fullPage: false,
    });
    const outDir = options.outDir ?? DEFAULT_FINDINGS_DIR;
    mkdirSync(outDir, { recursive: true });
    const path = join(outDir, `${key}.png`);
    writeFileSync(path, buffer);
    return { path };
  } finally {
    await page.evaluate(clearAnnotations);
  }
}
