#!/usr/bin/env node
// scripts/viewport-audit.mjs — do-harness "viewport-ux" sensor runner.
//
// Runs the web-ui text audit (./web-ui/lib) over WEB_AUDIT_ROUTES at the
// default viewport matrix and exits non-zero when findings exceed the cap.
// Policy mirrors do-harness sensors: a missing tool prints "SKIP:" and exits
// 0 (verify reports WARN; --strict/status stay honest), while real findings
// exit 1 with the JSON report on stderr for the failure tail.

const routes = (process.env.WEB_AUDIT_ROUTES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (routes.length === 0) {
  console.log("SKIP: WEB_AUDIT_ROUTES is not set (comma-separated app routes)");
  process.exit(0);
}
const base = process.env.WEB_AUDIT_BASE_URL ?? "http://127.0.0.1:3000";
const maxFindings = Number(process.env.WEB_AUDIT_MAX_FINDINGS ?? 0);
const allowlist = (process.env.WEB_AUDIT_ALLOWLIST ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  console.log("SKIP: playwright is not installed in this workspace");
  process.exit(0);
}

const { auditMatrix } = await import("./web-ui/lib/audit.mjs");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const report = await auditMatrix(page, {
    routes: routes.map((route) => new URL(route, base).toString()),
    allowlist,
  });
  if (report.findingCount > maxFindings) {
    console.error(JSON.stringify(report, null, 2));
    console.error(`FAIL: ${report.findingCount} text findings exceed the ${maxFindings} baseline`);
    process.exit(1);
  }
  console.log(
    `OK: viewport-ux clean across ${report.cells.length} cells (${report.findingCount} findings <= ${maxFindings})`,
  );
} finally {
  await browser.close();
}
