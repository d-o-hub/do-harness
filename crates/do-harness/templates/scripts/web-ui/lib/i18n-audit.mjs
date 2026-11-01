// i18n-audit.mjs — locale-regression sensor for the web-ui pack.
//
// Runs the existing text probe per locale and reports only what is
// LOCALE-SPECIFIC: findings that appear under a non-baseline locale but not
// under the baseline (German text expansion overflowing a container, an RTL
// locale rendered with LTR direction, etc.). Global defects are the
// viewport-ux sensor's job; this sensor isolates the per-locale delta so a
// baseline-locale defect does not drown out the signal.
//
// Locale switching is app-specific, so the sensor supports the two common
// mechanisms: a URL query parameter (`switchVia: { param: "lang" }`) or a
// cookie (`switchVia: { cookie: "locale" }`). Direction is validated against
// the locale's expected writing direction (WCAG 3.1.1/3.1.2: language of the
// page and of parts must be declared correctly, including `dir`).

/**
 * Locales whose writing direction is right-to-left (BCP-47 primary language
 * subtag). Everything else defaults to LTR.
 */
const RTL_LANGUAGES = new Set(["ar", "he", "fa", "ur", "ps", "sd", "ug", "yi"]);

/**
 * @param {string} locale — BCP-47 tag (e.g. "ar", "pt-BR", "zh-Hans-CN").
 * @returns {"rtl" | "ltr"}
 */
export function expectedDirection(locale) {
  const primary = String(locale).toLowerCase().split("-")[0];
  return RTL_LANGUAGES.has(primary) ? "rtl" : "ltr";
}

/**
 * Reason a rendered document direction violates its locale's writing
 * direction, or null when it matches. Callers must pass the `dir` attribute of
 * the page already navigated to this locale — reading it from another
 * locale's page reports that page's direction under the wrong name.
 *
 * An absent `dir` is treated as `ltr`: HTML's own default direction.
 * @param {string | null} dir — raw `documentElement` dir attribute
 * @param {string} locale — BCP-47 tag being audited
 * @returns {string | null}
 */
export function directionReason(dir, locale) {
  const expected = expectedDirection(locale);
  if ((dir ?? "ltr") === expected) return null;
  return `document dir is '${dir ?? "unset"}' but locale ${locale} expects '${expected}' (WCAG 3.1.2)`;
}

/**
 * Build the URL for one route x locale via a query parameter.
 * @param {string} route — absolute URL or path with any existing search.
 * @param {string} param
 * @param {string} locale
 */
export function buildLocaleUrl(route, param, locale) {
  const url = new URL(route, "http://localhost");
  url.searchParams.set(param, locale);
  return url.toString();
}

/** Stable key for cross-locale finding comparison. */
export function keyFinding(finding) {
  return `${finding.stage}|${finding.selector}|${finding.reason}`;
}

/**
 * Findings present under one locale but absent from the baseline locale.
 * @param {Array<object>} baselineFindings
 * @param {Array<object>} localeFindings
 */
export function diffLocaleFindings(baselineFindings, localeFindings) {
  const baseline = new Set(baselineFindings.map(keyFinding));
  return localeFindings.filter((f) => !baseline.has(keyFinding(f)));
}

/**
 * Audit one route across locales.
 *
 * @param {import('playwright').Page} page — un-navigated; this function owns
 *   navigation so it can set locale state before each load.
 * @param {{ route: string, locales: string[], baselineLocale?: string,
 *           switchVia: { param?: string, cookie?: string },
 *           allowlist?: string[], maxFindings?: number }} options
 * @returns {Promise<{ route: string, findings: Array<object> }>} findings are
 *   locale-only deltas plus any direction mismatches.
 */
export async function auditLocales(page, options) {
  const { pageProbe } = await import("./page-probe.mjs");
  const locales = options.locales;
  if (!Array.isArray(locales) || locales.length < 2) {
    throw new TypeError("locales must list at least two entries (baseline first)");
  }
  const baselineLocale = options.baselineLocale ?? locales[0];
  if (!locales.includes(baselineLocale)) {
    throw new TypeError(`baseline locale ${baselineLocale} must be in locales`);
  }

  const runProbe = async (locale) => {
    const url = options.switchVia?.param
      ? buildLocaleUrl(options.route, options.switchVia.param, locale)
      : options.route;
    if (options.switchVia?.cookie) {
      await page.context().addCookies([
        { name: options.switchVia.cookie, value: locale, url: url.origin ?? url },
      ]);
    }
    await page.goto(url, { waitUntil: "networkidle" });
    return page.evaluate(pageProbe, {
      allowlist: options.allowlist ?? [],
      maxFindings: options.maxFindings ?? 50,
      checkFocusVisibility: false,
      viewport: { width: page.viewportSize().width, height: page.viewportSize().height },
    });
  };

  const findings = [];
  const baseline = await runProbe(baselineLocale);
  for (const locale of locales) {
    if (locale === baselineLocale) continue;
    // Probe first: the direction contract (WCAG 3.1.1/3.1.2) must be read from
    // this locale's own document, so the finding names the locale it describes.
    const localeResult = await runProbe(locale);
    const dir = await page.evaluate(() => document.documentElement.getAttribute("dir"));
    const reason = directionReason(dir, locale);
    if (reason !== null) {
      findings.push({
        stage: "i18n-direction",
        selector: "html",
        route: options.route,
        locale,
        reason,
      });
    }
    for (const delta of diffLocaleFindings(baseline.findings, localeResult.findings)) {
      findings.push({ ...delta, route: options.route, locale });
    }
  }
  return { route: options.route, findings };
}
