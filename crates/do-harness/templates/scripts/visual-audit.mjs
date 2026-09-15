#!/usr/bin/env node
// scripts/visual-audit.mjs — do-harness "visual" sensor runner.
//
// Captures per route x viewport screenshots (animations disabled, caret
// hidden) and compares SHA-256 digests against blessed baselines under
// WEB_VISUAL_BASELINE_DIR (default .do-harness/visual). A cell with no
// baseline yet ("new") or a digest mismatch ("changed") is a finding and
// exits 1 with the JSON report on stderr: bless by rerunning with
// WEB_VISUAL_UPDATE=1 only after verifying the change is intended.
// Policy mirrors the other web runners: missing routes or Playwright prints
// "SKIP:" and exits 0.

const routes = (process.env.WEB_AUDIT_ROUTES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (routes.length === 0) {
  console.log("SKIP: WEB_AUDIT_ROUTES is not set (comma-separated app routes)");
  process.exit(0);
}
const base = process.env.WEB_AUDIT_BASE_URL ?? "http://127.0.0.1:3000";
const baselineDir = process.env.WEB_VISUAL_BASELINE_DIR ?? ".do-harness/visual";
const update = process.env.WEB_VISUAL_UPDATE === "1";

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  console.log("SKIP: playwright is not installed in this workspace");
  process.exit(0);
}

const { DEFAULT_VIEWPORT_MATRIX } = await import("./web-ui/lib/audit.mjs");
const { auditVisual } = await import("./web-ui/lib/visual-audit.mjs");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const findings = [];
  let cells = 0;
  for (const route of routes) {
    const url = new URL(route, base).toString();
    for (const viewport of DEFAULT_VIEWPORT_MATRIX) {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto(url, { waitUntil: "networkidle" });
      const result = await auditVisual(page, { route: url, viewport, baselineDir, update });
      cells += 1;
      if (result.finding) findings.push(result.finding);
    }
  }
  if (findings.length > 0) {
    console.error(JSON.stringify(findings, null, 2));
    console.error(`FAIL: ${findings.length} visual finding(s) across ${cells} cells`);
    process.exit(1);
  }
  console.log(`OK: screenshots match blessed baselines across ${cells} cells`);
} finally {
  await browser.close();
}
