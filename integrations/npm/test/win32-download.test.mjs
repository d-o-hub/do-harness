import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";

import dl from "../lib/win32-download.js";
import { deadPort, makeZip, sha256Hex, startServer } from "./helpers.mjs";

const pkgJson = JSON.parse(
  fs.readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);
const version = pkgJson.version;
const asset = dl.zipAssetName(version);
const assetDir = `do-harness-v${version}-x86_64-pc-windows-msvc`;

function tempCache(t) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "doh-dl-"));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  return path.join(dir, "cache");
}

function releaseRoutes({ zip, checksums }) {
  const routes = {};
  routes[`/v${version}/checksums.txt`] = checksums;
  routes[`/v${version}/${asset}`] = zip;
  return routes;
}

test("parseChecksums reads sha256sum output", () => {
  const text = [
    `${"a".repeat(64)}  ${asset}`,
    `${"b".repeat(64)}  other-file.zip`,
    "not a checksum line",
    `${"C".repeat(64)}   extra-space.zip`,
  ].join("\r\n");
  const digests = dl.parseChecksums(text);
  assert.equal(digests.size, 3);
  assert.equal(digests.get(asset), "a".repeat(64));
  assert.equal(digests.get("other-file.zip"), "b".repeat(64));
  assert.equal(digests.get("extra-space.zip"), "c".repeat(64));
});

test("extractBinary returns the exe from a deflated zip", () => {
  const exe = Buffer.from("#!/bin/sh\necho hi\n");
  const zip = makeZip([{ name: `${assetDir}/${dl.BINARY_NAME}`, data: exe }]);
  assert.deepEqual(dl.extractBinary(zip), exe);
});

test("extractBinary returns the exe from a stored zip", () => {
  const exe = Buffer.from("MZ....");
  const zip = makeZip(
    [{ name: `${assetDir}/${dl.BINARY_NAME}`, data: exe }],
    0,
  );
  assert.deepEqual(dl.extractBinary(zip), exe);
});

test("extractBinary rejects a zip without the binary", () => {
  const zip = makeZip([{ name: "dir/readme.txt", data: Buffer.from("hi") }]);
  assert.throws(() => dl.extractBinary(zip), {
    code: dl.ERR.INVALID_ARCHIVE,
  });
});

test("extractBinary rejects non-zip bytes", () => {
  assert.throws(() => dl.extractBinary(Buffer.from("not a zip")), {
    code: dl.ERR.INVALID_ARCHIVE,
  });
});

test("ensureWindowsBinary downloads, verifies, and caches once", async (t) => {
  const cacheDir = tempCache(t);
  const exe = Buffer.from("#!/bin/sh\necho ok\n");
  const zip = makeZip([{ name: `${assetDir}/${dl.BINARY_NAME}`, data: exe }]);
  const { server, hits, baseUrl } = await startServer(
    releaseRoutes({ zip, checksums: `${sha256Hex(zip)}  ${asset}\n` }),
  );
  t.after(() => server.close());

  const first = await dl.ensureWindowsBinary({ version, baseUrl, cacheDir });
  const expected = path.join(
    cacheDir,
    "do-harness",
    `v${version}`,
    dl.BINARY_NAME,
  );
  assert.equal(first, expected);
  assert.ok(fs.existsSync(first));

  await dl.ensureWindowsBinary({ version, baseUrl, cacheDir });
  // The sidecar records the verified extracted binary's digest for offline
  // cache verification.
  assert.equal(
    fs.readFileSync(`${first}.sha256`, "utf8").trim(),
    sha256Hex(exe),
  );
  // Cache hit: exactly one fetch of each fixture, across both runs.
  assert.equal(hits.get(`/v${version}/checksums.txt`), 1);
  assert.equal(hits.get(`/v${version}/${asset}`), 1);
  // The sidecar records the release digest for offline cache verification.
  assert.equal(
    fs.readFileSync(`${first}.sha256`, "utf8").trim(),
    sha256Hex(exe),
  );
});

test("ensureWindowsBinary fails closed on a checksum mismatch", async (t) => {
  const cacheDir = tempCache(t);
  const zip = makeZip([{ name: `${assetDir}/${dl.BINARY_NAME}`, data: Buffer.from("A") }]);
  const otherZip = makeZip([{ name: `${assetDir}/${dl.BINARY_NAME}`, data: Buffer.from("B") }]);
  const { server, baseUrl } = await startServer(
    releaseRoutes({ zip, checksums: `${sha256Hex(otherZip)}  ${asset}\n` }),
  );
  t.after(() => server.close());

  await assert.rejects(
    dl.ensureWindowsBinary({ version, baseUrl, cacheDir }),
    { code: dl.ERR.CHECKSUM_MISMATCH },
  );
  // Fail closed: no binary is written, so nothing could have executed.
  assert.ok(
    !fs.existsSync(
      path.join(cacheDir, "do-harness", `v${version}`, dl.BINARY_NAME),
    ),
  );
});

test("ensureWindowsBinary fails closed on a missing checksum entry", async (t) => {
  const cacheDir = tempCache(t);
  const { server, hits, baseUrl } = await startServer({
    [`/v${version}/checksums.txt`]: `${"0".repeat(64)}  unrelated-file.zip\n`,
  });
  t.after(() => server.close());

  await assert.rejects(
    dl.ensureWindowsBinary({ version, baseUrl, cacheDir }),
    { code: dl.ERR.MISSING_CHECKSUM },
  );
  assert.equal(hits.get(`/v${version}/${asset}`), undefined, "zip never fetched");
});

test("ensureWindowsBinary rejects path traversal in version", async (t) => {
  const cacheDir = tempCache(t);
  const port = await deadPort();
  await assert.rejects(
    dl.ensureWindowsBinary({
      version: "../../escape",
      baseUrl: `http://127.0.0.1:${port}`,
      cacheDir,
    }),
    { code: dl.ERR.INVALID_VERSION },
  );
});

test("ensureWindowsBinary reports a download failure", async (t) => {
  const cacheDir = tempCache(t);
  const port = await deadPort();
  await assert.rejects(
    dl.ensureWindowsBinary({
      version,
      baseUrl: `http://127.0.0.1:${port}`,
      cacheDir,
    }),
    (err) => err.code === dl.ERR.DOWNLOAD_FAILED,
  );
});

test("ensureWindowsBinary fails closed when the cache was tampered", async (t) => {
  const cacheDir = tempCache(t);
  const exe = Buffer.from("#!/bin/sh\necho ok\n");
  const zip = makeZip([{ name: `${assetDir}/${dl.BINARY_NAME}`, data: exe }]);
  const { server, baseUrl } = await startServer(
    releaseRoutes({ zip, checksums: `${sha256Hex(zip)}  ${asset}\n` }),
  );
  t.after(() => server.close());

  const binary = await dl.ensureWindowsBinary({ version, baseUrl, cacheDir });
  fs.writeFileSync(binary, "tampered");
  await assert.rejects(
    dl.ensureWindowsBinary({ version, baseUrl, cacheDir }),
    { code: dl.ERR.CHECKSUM_MISMATCH },
  );
});
