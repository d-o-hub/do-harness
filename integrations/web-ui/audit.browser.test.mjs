// audit.browser.test.mjs — rendered Playwright coverage for the web-ui pack.
// The test intentionally skips when Playwright or its browser is unavailable;
// CI installs both explicitly and treats this path as a required check.

import { createRequire } from "node:module";
import { createServer } from "node:http";
import { mkdir, mkdtemp, readFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import assert from "node:assert/strict";
import { auditMatrix } from "./lib/audit.mjs";
import { auditVisual } from "./lib/visual-audit.mjs";

const require = createRequire(import.meta.url);
const projectRoot = fileURLToPath(new URL("../../", import.meta.url));
const fixturesRoot = fileURLToPath(new URL("./fixtures/", import.meta.url));

function loadChromium() {
  try {
    return require("playwright").chromium;
  } catch {
    return null;
  }
}

async function serveFixtures() {
  const server = createServer(async (request, response) => {
    const pathname = new URL(request.url ?? "/", "http://127.0.0.1").pathname;
    const fixture = pathname.slice(1);
    if (!/^[a-z0-9-]+\.html$/.test(fixture)) {
      response.writeHead(404).end();
      return;
    }
    try {
      const content = await readFile(join(fixturesRoot, fixture));
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end(content);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  assert.ok(address && typeof address === "object");
  return { server, baseUrl: `http://127.0.0.1:${address.port}` };
}

function route(baseUrl, fixture) {
  return `${baseUrl}/${fixture}`;
}

const matrix = [
  { label: "mobile-sm", width: 320, height: 568 },
  { label: "desktop", width: 1280, height: 720 },
];

test("browser: rendered fixtures cover clean, overlap, contrast, and baseline isolation", async (t) => {
  const chromium = loadChromium();
  if (!chromium) {
    t.skip("SKIP: playwright is not installed in this workspace");
    return;
  }

  let browser;
  try {
    browser = await chromium.launch({ headless: true });
  } catch (error) {
    t.skip(`SKIP: chromium is unavailable (${error.message})`);
    return;
  }

  let fixtureServer;
  let scratch;
  try {
    fixtureServer = await serveFixtures();
    await mkdir(join(projectRoot, "target"), { recursive: true });
    scratch = await mkdtemp(join(projectRoot, "target", "web-ui-browser-"));
    const findingsDir = join(scratch, "findings");
    const baselineDir = join(scratch, "baselines");
    const page = await browser.newPage();
    const cleanUrl = route(fixtureServer.baseUrl, "clean.html");
    const overlapUrl = route(fixtureServer.baseUrl, "overlap.html");
    const descendantUrl = route(fixtureServer.baseUrl, "overlap-descendant.html");
    const contrastUrl = route(fixtureServer.baseUrl, "contrast.html");
    const shellUrl = route(fixtureServer.baseUrl, "app-shell.html");

    const clean = await auditMatrix(page, { routes: [cleanUrl], matrix });
    assert.equal(clean.findingCount, 0, JSON.stringify(clean, null, 2));

    const overlap = await auditMatrix(page, {
      routes: [overlapUrl],
      matrix,
      annotate: true,
      findingsDir,
    });
    assert.ok(overlap.findingCount > 0, "the sibling overlay must be reported");
    for (const cell of overlap.cells) {
      const findings = cell.findings.filter((finding) => finding.stage === "text-overlap");
      assert.equal(findings.length, 1, JSON.stringify(cell, null, 2));
      assert.equal(findings[0].route, overlapUrl);
      assert.equal(findings[0].viewport, cell.viewport.label);
      assert.ok(findings[0].rect.width > 0 && findings[0].rect.height > 0);
      assert.match(findings[0].reason, /overlaps another text leaf/);
      assert.match(findings[0].selector, /span$/);
      assert.match(findings[0].overlapsWith, /h1/);
    }
    assert.equal(overlap.annotationArtifacts.length, matrix.length);
    for (const artifact of overlap.annotationArtifacts) {
      const png = await readFile(artifact);
      assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
    }
    assert.equal(
      await page.evaluate(() => Boolean(document.getElementById("__do-harness-annotations"))),
      false,
    );

    const descendant = await auditMatrix(page, { routes: [descendantUrl], matrix });
    assert.ok(
      descendant.cells.every((cell) => cell.findings.some((finding) => finding.stage === "text-overlap")),
      JSON.stringify(descendant, null, 2),
    );

    const contrast = await auditMatrix(page, { routes: [contrastUrl], matrix: [matrix[1]] });
    assert.equal(
      contrast.cells[0].findings.filter((finding) => finding.stage === "contrast").length,
      1,
      JSON.stringify(contrast, null, 2),
    );
    assert.match(contrast.cells[0].findings.find((finding) => finding.stage === "contrast").text, /grey on light grey/);

    const shell = await auditMatrix(page, { routes: [shellUrl], matrix: [matrix[1]] });
    assert.ok(shell.findingCount > 0, "the app-shell flyout must be reported");
    assert.ok(shell.cells[0].findings.some((finding) => finding.stage === "text-overlap"));

    await page.goto(cleanUrl, { waitUntil: "networkidle" });
    await page.setViewportSize({ width: matrix[1].width, height: matrix[1].height });
    const firstVisual = await auditVisual(page, {
      route: cleanUrl,
      viewport: matrix[1],
      baselineDir,
    });
    assert.equal(firstVisual.status, "new");

    await page.goto(cleanUrl, { waitUntil: "networkidle" });
    const secondVisual = await auditVisual(page, {
      route: cleanUrl,
      viewport: matrix[1],
      baselineDir,
    });
    assert.equal(secondVisual.status, "unchanged");
  } finally {
    if (fixtureServer) {
      await new Promise((resolve) => fixtureServer.server.close(resolve));
    }
    await browser.close();
    if (scratch) await rm(scratch, { recursive: true, force: true });
  }
});
