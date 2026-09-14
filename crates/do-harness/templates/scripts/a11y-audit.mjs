#!/usr/bin/env node
// scripts/a11y-audit.mjs — do-harness "a11y" sensor runner (axe, WCAG 2.2 AA).
//
// Requires @axe-core/playwright in the workspace's node_modules (peer
// dependency of the web-ui pack). Missing tooling prints "SKIP:" and exits 0
// per the do-harness sensor policy; violations above the impact floor exit 1.

const routes = (process.env.WEB_AUDIT_ROUTES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (routes.length === 0) {
  console.log("SKIP: WEB_AUDIT_ROUTES is not set (comma-separated app routes)");
  process.exit(0);
}
const base = process.env.WEB_AUDIT_BASE_URL ?? "http://127.0.0.1:3000";
const impactFloor = process.env.WEB_AUDIT_A11Y_IMPACT_FLOOR ?? "serious";

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  console.log("SKIP: playwright is not installed in this workspace");
  process.exit(0);
}

const { auditAccessibility } = await import("./web-ui/lib/a11y-audit.mjs");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const findings = [];
  for (const route of routes) {
    await page.goto(new URL(route, base).toString(), { waitUntil: "networkidle" });
    const result = await auditAccessibility(page, { impactFloor });
    if (result.skipped) {
      console.log(`SKIP: ${result.skipped}`);
      process.exit(0);
    }
    findings.push(...result.findings.map((f) => ({ ...f, route })));
  }
  if (findings.length > 0) {
    console.error(JSON.stringify(findings, null, 2));
    console.error(`FAIL: ${findings.length} a11y violations at impact >= ${impactFloor}`);
    process.exit(1);
  }
  console.log(`OK: a11y clean on ${routes.length} route(s) at impact >= ${impactFloor}`);
} finally {
  await browser.close();
}
