#!/usr/bin/env node
// scripts/i18n-audit.mjs — do-harness "i18n" sensor runner (locale regression).
//
// Runs the text probe per locale and reports only locale-specific deltas
// (expansion overflow, RTL direction mismatch) via WEB_AUDIT_LOCALES
// (comma-separated BCP-47 tags, baseline locale first). Locale switching
// uses WEB_AUDIT_LOCALE_PARAM (default "lang") or WEB_AUDIT_LOCALE_COOKIE.
// Findings exit 1 with the JSON report on stderr; missing routes, fewer
// than two locales, or a missing Playwright prints "SKIP:" and exits 0.

const routes = (process.env.WEB_AUDIT_ROUTES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (routes.length === 0) {
  console.log("SKIP: WEB_AUDIT_ROUTES is not set (comma-separated app routes)");
  process.exit(0);
}
const locales = (process.env.WEB_AUDIT_LOCALES ?? "")
  .split(",")
  .map((s) => s.trim())
  .filter(Boolean);
if (locales.length < 2) {
  console.log("SKIP: WEB_AUDIT_LOCALES must list at least two locales (baseline first)");
  process.exit(0);
}
const base = process.env.WEB_AUDIT_BASE_URL ?? "http://127.0.0.1:3000";
const switchVia = process.env.WEB_AUDIT_LOCALE_COOKIE
  ? { cookie: process.env.WEB_AUDIT_LOCALE_COOKIE }
  : { param: process.env.WEB_AUDIT_LOCALE_PARAM ?? "lang" };

let chromium;
try {
  ({ chromium } = await import("playwright"));
} catch {
  console.log("SKIP: playwright is not installed in this workspace");
  process.exit(0);
}

const { auditLocales } = await import("./web-ui/lib/i18n-audit.mjs");
const browser = await chromium.launch();
try {
  const page = await browser.newPage();
  const findings = [];
  for (const route of routes) {
    const result = await auditLocales(page, {
      route: new URL(route, base).toString(),
      locales,
      switchVia,
    });
    findings.push(...result.findings);
  }
  if (findings.length > 0) {
    console.error(JSON.stringify(findings, null, 2));
    console.error(`FAIL: ${findings.length} locale-specific finding(s)`);
    process.exit(1);
  }
  console.log(`OK: no locale-specific regressions on ${routes.length} route(s)`);
} finally {
  await browser.close();
}
