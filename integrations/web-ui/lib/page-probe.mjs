// page-probe.mjs — the browser-context probe. `pageProbe` is passed to
// `page.evaluate(pageProbe, options)`, and Playwright serializes ONLY the
// function source, so every helper must be declared inside its body (no
// module-scope references, no imports). The canonical geometry/contrast math
// lives in ./geometry.mjs and ./contrast.mjs — unit-tested headlessly — and
// the nested helpers below are the necessarily-duplicated browser-context
// implementations of the same rules. Keep them in lockstep.

export function pageProbe(options = {}) {
  const TEXT_GAP_EPSILON = 1; // sub-pixel tolerance for edge rounding
  const OCCLUSION_SAMPLE_INSET = 0.35; // sample inset from leaf edges
  const MAX_TABBABLES = 100; // bound focus-traversal runtime per page
  const MIN_TARGET_PX = 24; // WCAG 2.5.8 target size minimum
  const allowlist = options.allowlist || [];
  const maxFindings = options.maxFindings || 50;

  // --- contrast helpers (mirror of ../lib/contrast.mjs) ---
  function parseColor(css) {
    if (typeof css !== "string") return null;
    const value = css.trim().toLowerCase();
    const hex = value.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/);
    if (hex) {
      const digits =
        hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join("") : hex[1];
      return [
        parseInt(digits.slice(0, 2), 16),
        parseInt(digits.slice(2, 4), 16),
        parseInt(digits.slice(4, 6), 16),
        1,
      ];
    }
    const fn = value.match(/^rgba?\(([^)]+)\)$/);
    if (fn) {
      const parts = fn[1].split(/[,\s/]+/).filter(Boolean).map(Number);
      if (parts.length < 3) return null;
      return [parts[0], parts[1], parts[2], parts.length > 3 ? parts[3] : 1];
    }
    return null;
  }
  function relativeLuminance(rgb) {
    const [r, g, b] = rgb.map((v) => {
      const c = Math.min(255, Math.max(0, v)) / 255;
      return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }
  function compositeOver(fg, bg) {
    const a = Math.min(1, Math.max(0, fg[3]));
    return [
      Math.round(fg[0] * a + bg[0] * (1 - a)),
      Math.round(fg[1] * a + bg[1] * (1 - a)),
      Math.round(fg[2] * a + bg[2] * (1 - a)),
    ];
  }
  function contrastRatio(fg, bg) {
    const lf = relativeLuminance(fg);
    const lb = relativeLuminance(bg);
    return (Math.max(lf, lb) + 0.05) / (Math.min(lf, lb) + 0.05);
  }
  function minimumRatio(fontSizePx, bold) {
    const large = fontSizePx >= 24 || (bold && fontSizePx >= 18.66);
    return large ? 3 : 4.5;
  }

  // --- geometry helpers (mirror of ../lib/geometry.mjs) ---
  function rectsIntersect(a, b) {
    return (
      a.x + a.width > b.x &&
      b.x + b.width > a.x &&
      a.y + a.height > b.y &&
      b.y + b.height > a.y
    );
  }
  function rectContains(inner, outer) {
    return (
      inner.x >= outer.x - TEXT_GAP_EPSILON &&
      inner.y >= outer.y - TEXT_GAP_EPSILON &&
      inner.x + inner.width <= outer.x + outer.width + TEXT_GAP_EPSILON &&
      inner.y + inner.height <= outer.y + outer.height + TEXT_GAP_EPSILON
    );
  }
  function classifyPair(a, b) {
    if (rectContains(a, b) || rectContains(b, a)) return "contained";
    if (!rectsIntersect(a, b)) return "none";
    // Same-line exemption mirrors ../lib/geometry.mjs isSameLine: vertical
    // overlap >= 70% of the shorter leaf's height.
    const verticalOverlap =
      Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
    if (verticalOverlap >= 0.7 * Math.min(a.height, b.height)) return "same-line";
    return "overlap";
  }

  // --- DOM helpers ---
  function selectorPath(el) {
    const parts = [];
    let node = el;
    for (let depth = 0; node && node.nodeType === 1 && depth < 4; depth++) {
      if (node.id) {
        parts.unshift(`#${node.id}`);
        break;
      }
      let part = node.tagName.toLowerCase();
      const parent = node.parentElement;
      if (parent) {
        const same = Array.from(parent.children).filter(
          (c) => c.tagName === node.tagName,
        );
        if (same.length > 1) part += `:nth-of-type(${same.indexOf(node) + 1})`;
      }
      parts.unshift(part);
      node = parent;
    }
    return parts.join(" > ");
  }
  function rectOf(el) {
    const r = el.getBoundingClientRect();
    return { x: r.x, y: r.y, width: r.width, height: r.height };
  }
  function hasDirectText(el) {
    for (const child of el.childNodes) {
      if (child.nodeType === 3 && child.textContent.trim().length > 0) return true;
    }
    return false;
  }
  function allowlisted(el) {
    return allowlist.some((sel) => {
      try {
        return el.closest(sel) !== null;
      } catch {
        return false;
      }
    });
  }

  // Stage 1: text leaves + invisible/contrast findings.
  function collectLeaves() {
    const all = Array.from(document.body.querySelectorAll("*"));
    const leaves = [];
    const findings = [];
    for (const el of all) {
      if (!hasDirectText(el)) continue;
      if (el.closest("[aria-hidden='true'], [role='presentation']")) continue;
      const rect = rectOf(el);
      if (rect.width <= 0 || rect.height <= 0) continue; // display:none / collapsed
      if (allowlisted(el)) continue;
      const style = getComputedStyle(el);
      const fontSize = parseFloat(style.fontSize);
      const transparent = parseColor(style.color)?.[3] === 0;
      const hiddenButLaidOut =
        style.visibility === "hidden" || style.opacity === "0" || transparent || fontSize < 8;
      if (hiddenButLaidOut) {
        // Occupies layout space but cannot be read (visibility:hidden keeps
        // layout; display:none never gets here because its rect is empty).
        findings.push({
          stage: "invisible-text",
          selector: selectorPath(el),
          text: el.textContent.trim().slice(0, 80),
          rect,
          reason: transparent
            ? "text color is fully transparent"
            : style.visibility === "hidden"
              ? "visibility:hidden but occupies layout space"
              : style.opacity === "0"
                ? "opacity:0 but occupies layout space"
                : `font-size ${fontSize}px is below the 8px readability floor`,
        });
        continue;
      }
      leaves.push({ el, rect, path: selectorPath(el), text: el.textContent.trim().slice(0, 80), style });
    }
    return { leaves, findings };
  }

  function contrastFindings(leaves) {
    const findings = [];
    for (const leaf of leaves) {
      const fg = parseColor(leaf.style.color);
      if (!fg) continue;
      // Resolve the effective background: nearest ancestor with an opaque
      // background, alpha-compositing the text color chain onto it.
      let bg = [255, 255, 255];
      let node = leaf.el.parentElement;
      let resolved = false;
      while (node && node.nodeType === 1) {
        const bgStyle = parseColor(getComputedStyle(node).backgroundColor);
        if (bgStyle && bgStyle[3] > 0) {
          bg = bgStyle.slice(0, 3);
          resolved = true;
          break;
        }
        node = node.parentElement;
      }
      if (!resolved) bg = [255, 255, 255]; // assume white canvas
      const effectiveFg = compositeOver(fg, bg);
      const ratio = contrastRatio(effectiveFg, bg);
      const bold = Number.parseInt(leaf.style.fontWeight, 10) >= 700;
      const min = minimumRatio(parseFloat(leaf.style.fontSize), bold);
      if (ratio >= min) continue;
      findings.push({
        stage: "contrast",
        selector: leaf.path,
        text: leaf.text,
        rect: leaf.rect,
        reason: `contrast ${ratio.toFixed(2)}:1 < WCAG 1.4.3 minimum ${min}:1`,
        ratio: Number(ratio.toFixed(2)),
        minRatio: min,
      });
    }
    return findings;
  }

  // Stage 2: occlusion via elementsFromPoint at center + inset corners.
  function occlusionFindings(leaves) {
    const findings = [];
    for (const leaf of leaves) {
      const { x, y, width, height } = leaf.rect;
      const dx = (width / 2) * OCCLUSION_SAMPLE_INSET;
      const dy = (height / 2) * OCCLUSION_SAMPLE_INSET;
      const points = [
        [x + width / 2, y + height / 2],
        [x + dx, y + dy],
        [x + width - dx, y + dy],
        [x + dx, y + height - dy],
        [x + width - dx, y + height - dy],
      ];
      for (const [px, py] of points) {
        if (px < 0 || py < 0) continue;
        const stack = document.elementsFromPoint(px, py);
        if (!stack.length) continue;
        const related = stack.some(
          (s) => s === leaf.el || s.contains(leaf.el) || leaf.el.contains(s),
        );
        if (related) break; // a sample point sees the leaf itself: not occluded
        findings.push({
          stage: "occluded-text",
          selector: leaf.path,
          text: leaf.text,
          rect: leaf.rect,
          reason: "text is covered by another element at its center",
          occluder: selectorPath(stack[0]),
        });
        break; // one occlusion finding per leaf is enough
      }
    }
    return findings;
  }

  // Stage 3: pairwise text-over-text overlap.
  function pairwiseOverlaps(leaves) {
    const findings = [];
    for (let i = 0; i < leaves.length && findings.length < maxFindings; i++) {
      for (let j = i + 1; j < leaves.length && findings.length < maxFindings; j++) {
        if (classifyPair(leaves[i].rect, leaves[j].rect) !== "overlap") continue;
        findings.push({
          stage: "text-overlap",
          selector: leaves[i].path,
          text: leaves[i].text,
          rect: leaves[i].rect,
          reason: "overlaps another text leaf (not same-line, not ancestor)",
          overlapsWith: leaves[j].path,
          overlapsWithText: leaves[j].text,
        });
      }
    }
    return findings;
  }

  // Stage 4: reflow + unintentional horizontal scroll containers.
  function overflowFindings(viewport) {
    const findings = [];
    const scroller = document.scrollingElement || document.documentElement;
    if (scroller.scrollWidth > scroller.clientWidth + TEXT_GAP_EPSILON) {
      findings.push({
        stage: "reflow-overflow",
        selector: "html",
        reason: `document is ${scroller.scrollWidth}px wide at a ${viewport.width}px viewport (WCAG 1.4.10 reflow)`,
        overflowPx: scroller.scrollWidth - scroller.clientWidth,
      });
    }
    for (const el of document.body.querySelectorAll("*")) {
      const style = getComputedStyle(el);
      if (!["auto", "scroll"].includes(style.overflowX)) continue;
      if (el.scrollWidth <= el.clientWidth + TEXT_GAP_EPSILON) continue;
      if (el.matches("pre, code, table, .audit-allow-horizontal-scroll")) continue;
      findings.push({
        stage: "horizontal-scroll-container",
        selector: selectorPath(el),
        reason: `container scrolls horizontally (${el.scrollWidth}px content in ${el.clientWidth}px box) without an allowlist match`,
      });
    }
    return findings;
  }

  // Stage 5: focus traversal (WCAG 2.4.7) + target size (WCAG 2.5.8).
  function focusAndTargetFindings() {
    const findings = [];
    const tabbables = Array.from(
      document.body.querySelectorAll(
        "a[href], button, input, select, textarea, [tabindex]:not([tabindex='-1'])",
      ),
    ).slice(0, MAX_TABBABLES);
    const previousFocus = document.activeElement;
    for (const el of tabbables) {
      const rect = rectOf(el);
      if (rect.width <= 0 || rect.height <= 0) continue;
      if (rect.width < MIN_TARGET_PX || rect.height < MIN_TARGET_PX) {
        findings.push({
          stage: "target-size",
          selector: selectorPath(el),
          reason: `target is ${Math.round(rect.width)}x${Math.round(rect.height)}px < ${MIN_TARGET_PX}px (WCAG 2.5.8)`,
          rect,
        });
      }
      if (options.checkFocusVisibility === false) continue;
      el.focus({ preventScroll: true });
      const style = getComputedStyle(el);
      const top = document.elementsFromPoint(rect.x + rect.width / 2, rect.y + rect.height / 2)[0];
      const visibleAfterFocus =
        rect.y >= 0 &&
        rect.x >= 0 &&
        style.visibility !== "hidden" &&
        style.opacity !== "0" &&
        top !== undefined &&
        (top === el || el.contains(top) || top.contains(el));
      if (!visibleAfterFocus) {
        findings.push({
          stage: "focus-visibility",
          selector: selectorPath(el),
          reason:
            top && !(top === el || el.contains(top) || top.contains(el))
              ? "focused element is occluded at its center (WCAG 2.4.7)"
              : "focused element is outside the viewport or invisible",
        });
      }
    }
    if (previousFocus && previousFocus.focus) previousFocus.focus({ preventScroll: true });
    return findings;
  }

  const { leaves, findings: stage1 } = collectLeaves();
  const findings = [
    ...stage1,
    ...contrastFindings(leaves),
    ...occlusionFindings(leaves),
    ...pairwiseOverlaps(leaves),
    ...overflowFindings(options.viewport || { width: window.innerWidth }),
    ...focusAndTargetFindings(),
  ];
  return {
    textLeaves: leaves.length,
    findings: findings.slice(0, maxFindings * 2),
    truncated: findings.length > maxFindings * 2,
  };
}
