// audit.test.mjs — headless unit tests (node --test) for the pure logic of
// the viewport text audit. Browser-dependent behavior (getComputedStyle,
// elementsFromPoint) is covered by the optional browser suite in audit.browser
// .test.mjs, which SKIPs when Playwright browsers are unavailable — mirroring
// do-harness's sensor SKIP policy so CI without browsers stays honest.

import test from "node:test";
import assert from "node:assert/strict";
import {
  rectsIntersect,
  intersectionArea,
  rectContains,
  classifyPair,
  collectOverlaps,
  horizontalOverflowPx,
} from "./lib/geometry.mjs";
import {
  relativeLuminance,
  compositeOver,
  parseColor,
  contrastRatio,
  minimumRatio,
} from "./lib/contrast.mjs";
import { normalizeViewport, normalizeMatrix, DEFAULT_VIEWPORT_MATRIX } from "./lib/audit.mjs";

const rect = (x, y, w, h) => ({ x, y, width: w, height: h });

test("rectsIntersect: positive-area overlap only", () => {
  assert.equal(rectsIntersect(rect(0, 0, 10, 10), rect(5, 5, 10, 10)), true);
  assert.equal(rectsIntersect(rect(0, 0, 10, 10), rect(10, 0, 10, 10)), false); // touching edge
  assert.equal(rectsIntersect(rect(0, 0, 10, 10), rect(20, 20, 5, 5)), false);
});

test("intersectionArea: zero for touching edges", () => {
  assert.equal(intersectionArea(rect(0, 0, 10, 10), rect(5, 5, 10, 10)), 25);
  assert.equal(intersectionArea(rect(0, 0, 10, 10), rect(10, 0, 10, 10)), 0);
});

test("classifyPair: contained pairs never overlap", () => {
  assert.equal(classifyPair(rect(0, 0, 100, 20), rect(10, 5, 10, 10)), "contained");
  assert.equal(classifyPair(rect(10, 5, 10, 10), rect(0, 0, 100, 20)), "contained");
});

test("classifyPair: same-line inline siblings are not overlap", () => {
  assert.equal(classifyPair(rect(0, 0, 40, 20), rect(41, 0, 40, 20)), "none");
  // vertically adjacent rows intersecting horizontally => not same-line
  assert.equal(classifyPair(rect(0, 0, 50, 20), rect(10, 21, 50, 20)), "none");
});

test("classifyPair: genuine cross-line overlap is flagged", () => {
  // deliberately absolutely-positioned label over a heading, overlapping by
  // only half the line height: NOT same-line
  assert.equal(classifyPair(rect(0, 0, 60, 24), rect(10, 12, 60, 24)), "overlap");
  // same-line pair (full vertical overlap, slight horizontal nudge)
  assert.equal(classifyPair(rect(0, 0, 40, 20), rect(39, 0, 40, 20)), "same-line");
});

test("collectOverlaps: skips contained pairs, flags overlaps, caps output", () => {
  const leaves = [
    { rect: rect(0, 0, 50, 20), path: "h1", text: "title" },
    { rect: rect(0, 0, 50, 20), path: "h1-wrap", text: "title" }, // contained
    { rect: rect(10, 12, 60, 24), path: "label", text: "badge" },
  ];
  const found = collectOverlaps(leaves);
  // h1-wrap and h1 are contained (skipped); label genuinely overlaps BOTH
  // the wrap and the inner heading at the same coordinates => 2 findings.
  assert.equal(found.length, 2);
  assert.ok(found.every((f) => f.b === "label"));
  // Even leaves sit on one line, odd leaves half-overlap the next line, and
  // every even-odd pair intersects horizontally: 900 candidate pairs, capped.
  const many = Array.from({ length: 60 }, (_, i) => ({
    rect: rect(i * 10, (i % 2) * 10, 50, 20),
    path: `p${i}`,
    text: "x",
  }));
  assert.equal(collectOverlaps(many, { maxFindings: 5 }).length, 5);
});

test("horizontalOverflowPx: only counts past the viewport edge", () => {
  assert.equal(horizontalOverflowPx(rect(300, 0, 100, 10), { width: 320 }), 80);
  assert.equal(horizontalOverflowPx(rect(0, 0, 320, 10), { width: 320 }), 0);
});

test("contrast: WCAG luminance and ratios", () => {
  assert.equal(parseColor("#fff").length, 4);
  assert.deepEqual(parseColor("rgb(255, 255, 255)"), [255, 255, 255, 1]);
  assert.deepEqual(parseColor("rgba(0, 0, 0, 0.5)"), [0, 0, 0, 0.5]);
  // black on white is 21:1
  assert.ok(Math.abs(contrastRatio([0, 0, 0], [255, 255, 255]) - 21) < 0.01);
  // gray #767676 on white is exactly the 4.5:1 boundary region
  assert.ok(contrastRatio([118, 118, 118], [255, 255, 255]) < 4.6);
  // 50% black over white = gray, not black
  assert.deepEqual(compositeOver([0, 0, 0, 0.5], [255, 255, 255]), [128, 128, 128]);
  assert.equal(relativeLuminance([255, 255, 255]), 1);
});

test("contrast: minimum ratio follows the large-text rule", () => {
  assert.equal(minimumRatio({ fontSizePx: 16, bold: false }), 4.5);
  assert.equal(minimumRatio({ fontSizePx: 24, bold: false }), 3);
  assert.equal(minimumRatio({ fontSizePx: 19, bold: true }), 3);
  assert.equal(minimumRatio({ fontSizePx: 19, bold: false }), 4.5);
});

test("viewport matrix: normalizes, validates, and rejects garbage", () => {
  const m = normalizeMatrix([{ width: 320, height: 568 }]);
  assert.deepEqual(m, [{ label: "320x568", width: 320, height: 568 }]);
  assert.throws(() => normalizeViewport({ width: 10, height: 500 }), RangeError);
  assert.throws(() => normalizeMatrix([]), RangeError);
  assert.ok(DEFAULT_VIEWPORT_MATRIX.some((v) => v.width === 320)); // reflow floor
  assert.ok(DEFAULT_VIEWPORT_MATRIX.some((v) => v.width === 360)); // Android majority
});
