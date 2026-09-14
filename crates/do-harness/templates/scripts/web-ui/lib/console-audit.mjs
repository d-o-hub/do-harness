// console-audit.mjs — console/noise sensor for the web-ui pack. Attaches
// Playwright page listeners that collect console errors, page errors, and
// failed network responses during a route visit, then classifies them.
//
// Design: the *classification* logic is pure and headlessly testable
// (classifyConsoleMessage); the listener wiring is a thin Playwright adapter.
// Noise filtering mirrors what a human reviewer would discount: framework
// dev-mode warnings, CORS/API failures from a not-yet-running backend, and
// favicon 404s are downgraded to `noise` instead of `error`.

/**
 * Classify one console/page/network event.
 * @param {{ kind: "console" | "pageerror" | "response-failed" | "response-error",
 *           type?: string, text?: string, url?: string, status?: number }} event
 * @returns {"error" | "noise"}
 */
export function classifyConsoleMessage(event) {
  const text = `${event.text ?? ""} ${event.url ?? ""}`.toLowerCase();

  if (event.kind === "response-error" && event.status) {
    // Backend-not-running / auth-needed noise in dev and preview lanes.
    if (event.status === 401 || event.status === 403 || event.status === 503) return "noise";
    //favicon and sourcemap 404s are never user-facing defects
    if (event.status === 404 && /(favicon|\.map|\.png|\.ico)/.test(text)) return "noise";
    return "error";
  }
  if (event.kind === "response-failed") {
    // net::ERR_FAILED on aborted fetches / offline probes is ambient noise.
    if (/err_aborted|err_failed|err_internet_disconnected/.test(text) && /api|health|ping/.test(text)) {
      return "noise";
    }
    return "error";
  }
  if (event.kind === "pageerror") return "error";
  // Console channel: only errors count; warnings/info are noise by design.
  if (event.type && event.type !== "error") return "noise";
  if (/devtools|deprecat|hmr|\[vite\]|react-devtools/.test(text)) return "noise";
  return "error";
}

/**
 * Collector to pass around a page visit. Wire it, navigate, then `drain()`.
 * @param {import('playwright').Page} page
 */
export function attachConsoleCollector(page) {
  const events = [];
  const onConsole = (msg) =>
    events.push({ kind: "console", type: msg.type(), text: msg.text() });
  const onPageError = (err) => events.push({ kind: "pageerror", text: String(err) });
  const onResponse = (res) => {
    if (res.status() >= 400) {
      events.push({ kind: "response-error", status: res.status(), url: res.url() });
    }
  };
  const onRequestFailed = (req) =>
    events.push({ kind: "response-failed", text: req.failure()?.errorText, url: req.url() });

  page.on("console", onConsole);
  page.on("pageerror", onPageError);
  page.on("response", onResponse);
  page.on("requestfailed", onRequestFailed);

  return {
    /** Split collected events into actionable errors vs discounted noise. */
    drain() {
      const errors = [];
      const noise = [];
      for (const event of events) {
        (classifyConsoleMessage(event) === "error" ? errors : noise).push(event);
      }
      return { errors, noise };
    },
    detach() {
      page.off("console", onConsole);
      page.off("pageerror", onPageError);
      page.off("response", onResponse);
      page.off("requestfailed", onRequestFailed);
    },
  };
}
