# web-ui — viewport text audit for do-harness

The browser-side sensor library for do-harness's web-ui pack (issue #68).
It validates, per **route × viewport × locale × scheme**, that all text is
readable: visible, not occluded, not overlapping, and not overflowing —
grounded in WCAG 2.2 (1.4.3 contrast, 1.4.10 reflow, 2.4.7 focus visibility,
2.5.8 target size).

## Layout

| File | Role | Testable headlessly |
|------|------|---------------------|
| `lib/geometry.mjs` | rect math + overlap classification | yes (pure) |
| `lib/contrast.mjs` | WCAG luminance/compositing/ratios | yes (pure) |
| `lib/page-probe.mjs` | browser-context probe (all 5 stages) | via browser suite |
| `lib/console-audit.mjs` | console/page-error/failed-request collector + noise classifier | classification yes |
| `lib/a11y-audit.mjs` | axe-core adapter (WCAG 2.2 AA tags; `@axe-core/playwright` is an adopting-repo peer dep) | via browser suite |
| `lib/i18n-audit.mjs` | per-locale probe diffing + RTL direction contract (3.1.1/3.1.2) | diffing + direction yes |
| `lib/perf-audit.mjs` | Lighthouse adapter (peer dep) + CWV budget evaluation | budget evaluation yes |
| `lib/visual-audit.mjs` | screenshot capture + digest baselines under `.do-harness/visual/` | classification yes |
| `lib/audit.mjs` | matrix normalization + orchestrator | normalization yes |
| `audit.test.mjs` | `node --test` unit tests (no browser needed) | — |

## The five audit stages

1. **invisible-text** — text that occupies layout space but cannot be read:
   `visibility:hidden`, `opacity:0`, transparent color, or `font-size < 8px`.
   (`display:none` content is skipped silently — collapsed UI is normal.)
2. **contrast** — WCAG 1.4.3: effective color (alpha-composited over the
   nearest opaque ancestor background) vs 4.5:1, or 3:1 for large text.
3. **occluded-text** — `document.elementsFromPoint()` sampling at the text
   center + inset corners; catches z-index overlays and sticky headers that
   bounding-box intersection cannot.
4. **text-overlap** — pairwise rect intersection among text leaves,
   excluding ancestor/descendant pairs and same-line inline siblings.
5. **reflow/scroll** — horizontal document overflow (1.4.10) and
   unintentionally horizontally-scrollable containers (`pre`/`code`/`table`
   and `.audit-allow-horizontal-scroll` are allowlisted), plus focus
   traversal visibility (2.4.7) and target size (2.5.8).

## Running the tests

```bash
node --test integrations/web-ui/audit.test.mjs
```

Browser-dependent probes need Playwright browsers; a future
`audit.browser.test.mjs` SKIPs when they are unavailable, matching the
do-harness sensor policy ("SKIP: tool unavailable") rather than failing CI
that has no browsers installed.

## Wiring as a do-harness sensor

`[[sensors]]` entry for an adopting web repo (sensor wraps the repo's own
script so local evidence and CI cannot drift):

```toml
[[sensors]]
name = "viewport-ux"
argv = ["node", "scripts/viewport-audit.mjs", "--routes", "/,/login,/catalog"]
when-changed = ["apps/web/**", "packages/ui/**"]

[[sensors]]
name = "a11y"
argv = ["node", "scripts/a11y-audit.mjs", "--impact-floor", "serious"]
when-changed = ["apps/web/**", "packages/ui/**"]

[[sensors]]
name = "console"
argv = ["node", "scripts/console-audit.mjs", "--routes", "/,/login"]
when-changed = ["apps/web/**", "apps/worker/**"]
```

`scripts/viewport-audit.mjs` (in the adopting repo) starts a browser, calls
`auditMatrix()`, and exits non-zero when findings exceed the ratchet
baseline — pairing naturally with the severity/ratchet proposal in #66 and
the evidence matrix-manifest proposal in #67. The `a11y` wrapper calls
`auditAccessibility()` (requires `@axe-core/playwright` in the adopting
repo's node_modules; the audit reports a SKIP-shaped result when it is
absent), and the `console` wrapper drains `attachConsoleCollector()` after
navigating each route, failing on classified `error` events.

## Visual baselines

`auditVisual(page, { route, viewport })` captures a full-page screenshot per
matrix cell (animations disabled, caret hidden) and stores blessed baselines
as PNGs under `.do-harness/visual/` (gitignore that directory in the
adopting repo). Classification is digest-based with zero image dependencies:

- **new** — no baseline yet: a WARN-class finding; the capture is blessed on
  this run so the next run compares.
- **changed** — digest differs from the blessed baseline: a failure. Rerun
  with `WEB_VISUAL_UPDATE=1` **only after verifying the change is intended**
  (that flow is the visual equivalent of `eval --bless`).
- **unchanged** — no finding.

Because screenshots are environment-sensitive (font rendering, GPU), bless
baselines in the same environment CI uses (pinned fonts in a container) —
the same discipline as any snapshot test. Per-pixel diffing with masked
regions and thresholds can layer on top; digest equality is deliberately
the v1 contract.
