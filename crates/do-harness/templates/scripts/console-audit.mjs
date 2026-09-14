#!/usr/bin/env node
// scripts/console-audit.mjs — do-harness "console" sensor runner.
//
// Navigates each route with a console collector attached and fails on events
// classified as errors (dev-mode warnings, favicon 404s, 401/403/503
// backend-availability statuses, and aborted api probes are discounted noise
// — see web-ui/lib/console-audit.mjs). Missing tooling prints "SKIP:".

const routes = (process.env.WEB_AUDIT_ROUTES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (routes.length === 0) {
  console.log("SKIP: WEB_AUDIT_ROUTES is not set (comma-separated app routes)");
  process.exit(0);
}
const base = process.env.WEB_AUDIT_BASE_URL ?? "http://127.0.0.1:3000";

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  console.log("SKIP: playwright is not installed in this workspace");
  process.exit(0);
}

const { attachConsoleCollector } = await import("./web-ui/lib/console-audit.mjs");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const errors = [];
  for (const route of routes) {
    const collector = attachConsoleCollector(page);
    await page.goto(new URL(route, base).toString(), { waitUntil: "networkidle" });
    await page.waitForTimeout(500); // let late console/IO land
    const drained = collector.drain();
    errors.push(...drained.errors.map((e) => ({ ...e, route })));
  }
  if (errors.length > 0) {
    console.error(JSON.stringify(errors, null, 2));
    console.error(`FAIL: ${errors.length} console error(s) across ${routes.length} route(s)`);
    process.exit(1);
  }
  console.log(`OK: console clean on ${routes.length} route(s)`);
} finally {
  await browser.close();
}
