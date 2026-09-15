// contrast.mjs — WCAG 2.2 relative-luminance math for the text-visibility
// stage. Pure functions (unit-testable headlessly); the page probe resolves
// effective colors through alpha compositing before calling these.

/**
 * WCAG 2.x relative luminance from an sRGB channel triple in [0, 255].
 * @param {[number, number, number]} rgb
 */
export function relativeLuminance(rgb) {
  const [r, g, b] = rgb.map((v) => {
    const c = Math.min(255, Math.max(0, v)) / 255;
    return c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Composite a foreground color over a background color with alpha
 * (source-over). `[r,g,b,a]` with a in [0,1]. This is how "gray text at 50%
 * opacity over white" gets its real effective color.
 * @param {[number, number, number, number]} fg
 * @param {[number, number, number]} bg
 * @returns {[number, number, number]}
 */
export function compositeOver(fg, bg) {
  const a = Math.min(1, Math.max(0, fg[3]));
  return [
    Math.round(fg[0] * a + bg[0] * (1 - a)),
    Math.round(fg[1] * a + bg[1] * (1 - a)),
    Math.round(fg[2] * a + bg[2] * (1 - a)),
  ];
}

/**
 * Composite a stack of background layers onto a canvas color. Layers are
 * ordered from the text outwards (the leaf element's own background first,
 * then its ancestors), so the probe passes what it collects while walking up
 * the tree. Translucent surfaces (rgba() backgrounds, 60% veils) resolve
 * against what is actually behind them instead of being treated as opaque.
 * @param {Array<[number, number, number, number]>} layers
 * @param {[number, number, number]} [canvas]
 * @returns {[number, number, number]}
 */
export function compositeLayers(layers, canvas = [255, 255, 255]) {
  let bg = canvas;
  for (let i = layers.length - 1; i >= 0; i--) bg = compositeOver(layers[i], bg);
  return bg;
}

/** Parse `rgb(...)`, `rgba(...)`, and 6/3-digit hex into [r,g,b,a]. */
export function parseColor(css) {
  if (typeof css !== "string") return null;
  const value = css.trim().toLowerCase();
  const hex = value.match(/^#([0-9a-f]{3}|[0-9a-f]{6})$/);
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join("") : hex[1];
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

/** WCAG 1.4.3 contrast ratio between two opaque RGB triples. */
export function contrastRatio(fg, bg) {
  const lf = relativeLuminance(fg);
  const lb = relativeLuminance(bg);
  const lighter = Math.max(lf, lb);
  const darker = Math.min(lf, lb);
  return (lighter + 0.05) / (darker + 0.05);
}

/**
 * WCAG 1.4.3 minimum ratio for this text. Large text (>= 18pt/24px, or
 * >= 14pt/18.66px bold) requires 3:1; everything else 4.5:1.
 * @param {{ fontSizePx: number, bold: boolean }} text
 */
export function minimumRatio(text) {
  const large = text.fontSizePx >= 24 || (text.bold && text.fontSizePx >= 18.66);
  return large ? 3 : 4.5;
}
