// geometry.mjs — pure bounding-rect math and overlap classification for the
// viewport text audit. No DOM access: everything here is unit-testable
// headlessly (node --test), which keeps the audit's decision logic covered
// even where browsers are unavailable (CI without Playwright browsers).

/** @typedef {{ x: number, y: number, width: number, height: number }} Rect */

/**
 * True when two rects share any area (strictly positive overlap).
 * Touching edges (zero-area contact) do not count as overlap.
 * @param {Rect} a
 * @param {Rect} b
 * @returns {boolean}
 */
export function rectsIntersect(a, b) {
  return (
    a.x + a.width > b.x &&
    b.x + b.width > a.x &&
    a.y + a.height > b.y &&
    b.y + b.height > a.y
  );
}

/** @returns {number} shared area of two rects (0 when they only touch). */
export function intersectionArea(a, b) {
  const w = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
  const h = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return w > 0 && h > 0 ? w * h : 0;
}

/**
 * True when `inner` lies fully inside `outer` (within a 1px tolerance, since
 * browsers report sub-pixel geometry that rounds at edges).
 * @param {Rect} inner
 * @param {Rect} outer
 */
export function rectContains(inner, outer, tolerance = 1) {
  return (
    inner.x >= outer.x - tolerance &&
    inner.y >= outer.y - tolerance &&
    inner.x + inner.width <= outer.x + outer.width + tolerance &&
    inner.y + inner.height <= outer.y + outer.height + tolerance
  );
}

/**
 * True when the rect is a chain link: one contains the other. Text inside a
 * container, or a container wrapping text, may legitimately share coordinates
 * and must never be reported as text-over-text overlap.
 */
export function isAncestorOrDescendant(a, b) {
  return rectContains(a, b, 1) || rectContains(b, a, 1);
}

/**
 * Two text leaves share a line box when their vertical overlap covers at
 * least 70% of the shorter leaf's height. Half-overlapping rows (a label
 * sitting across two lines' boundary) are NOT same-line — they are the
 * classic overlap defect this audit exists to catch.
 */
export function isSameLine(a, b) {
  const verticalOverlap =
    Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
  return verticalOverlap >= 0.7 * Math.min(a.height, b.height);
}

/**
 * True for out-of-flow positioning: such elements never share a line box
 * with in-flow text, so a same-baseline collision with them is a genuine
 * overlap (badge over heading, absolutely-positioned promo over copy).
 * `relative`/`sticky` lay out in flow and keep the exemption.
 * @param {string} position — computed `position` value, or "" when unknown.
 */
export function isOutOfFlow(position) {
  return position === "absolute" || position === "fixed";
}

/**
 * Classify a pair of text-leaf rects. Pass each leaf's computed `position`
 * via `options` (`{ positionedA, positionedB }`): when either side is
 * out-of-flow the same-line exemption is withheld, because absolutely- or
 * fixed-positioned elements cannot share a line box with anything.
 * @returns {"overlap" | "same-line" | "contained" | "none"}
 */
export function classifyPair(a, b, { positionedA = false, positionedB = false } = {}) {
  if (isAncestorOrDescendant(a, b)) return "contained";
  if (!rectsIntersect(a, b)) return "none";
  if (!positionedA && !positionedB && isSameLine(a, b)) return "same-line";
  return "overlap";
}

/**
 * Horizontal overflow of a rect past the viewport width — the WCAG 1.4.10
 * reflow signal when the document scrolls horizontally instead of wrapping.
 * @param {Rect} rect
 * @param {{ width: number }} viewport
 */
export function horizontalOverflowPx(rect, viewport) {
  return Math.max(0, rect.x + rect.width - viewport.width);
}

/**
 * Pairwise classification over N text leaves, skipping contained/same-line
 * pairs, with an O(n²) guard: callers pass at most a few hundred leaves per
 * route per viewport, and the audit stops collecting after `maxFindings` so a
 * catastrophically broken page cannot produce quadratic output blowup.
 * Leaves may carry `positioned: true` (computed absolute/fixed position) to
 * withhold the same-line exemption for that pair — see `classifyPair`.
 * @param {Array<{ rect: Rect, path: string, text: string, positioned?: boolean }>} leaves
 * @param {{ maxFindings?: number }} [options]
 */
export function collectOverlaps(leaves, { maxFindings = 50 } = {}) {
  /** @type {Array<{ a: string, b: string, areaPx: number, aText: string, bText: string }>} */
  const findings = [];
  for (let i = 0; i < leaves.length && findings.length < maxFindings; i++) {
    for (let j = i + 1; j < leaves.length && findings.length < maxFindings; j++) {
      const kind = classifyPair(leaves[i].rect, leaves[j].rect, {
        positionedA: leaves[i].positioned === true,
        positionedB: leaves[j].positioned === true,
      });
      if (kind !== "overlap") continue;
      findings.push({
        a: leaves[i].path,
        b: leaves[j].path,
        aText: leaves[i].text,
        bText: leaves[j].text,
        areaPx: Math.round(intersectionArea(leaves[i].rect, leaves[j].rect)),
      });
    }
  }
  return findings;
}
