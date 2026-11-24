"use strict";

// Windows download-on-first-run for the npm wrapper.
//
// The `do-harness-win32-x64` platform package is rejected by npm's registry
// name screening, so on Windows x64 the pinned optionalDependency can never
// resolve. Instead of failing, the shim fetches the release zip for the exact
// wrapper version, verifies its SHA-256 against the release `checksums.txt`,
// and caches the extracted binary. There is no `postinstall` script: nothing
// runs at install time, which keeps the install-time-execution supply-chain
// surface away.
//
// Fail-closed rules:
// - a missing checksum entry aborts the download (nothing is fetched);
// - a checksum mismatch aborts before extraction (nothing is executed);
// - a cached binary that no longer matches its recorded digest aborts with
//   guidance (fail closed, never silently re-download or execute).
//
// Trust model: the digest comes from the same GitHub release as the zip, so
// this detects corruption and truncated downloads, not a compromised origin —
// the same boundary as scripts/install.sh (see docs/provenance-trust-model.md).

const crypto = require("node:crypto");
const fs = require("node:fs");
const http = require("node:http");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");
const zlib = require("node:zlib");

const WINDOWS_TARGET = "x86_64-pc-windows-msvc";
const BINARY_NAME = "do-harness.exe";
const MAX_REDIRECTS = 10;
const REQUEST_TIMEOUT_MS = 60 * 1000;
const MAX_CHECKSUMS_BYTES = 1 << 20; // checksums.txt is a few hundred bytes.
const MAX_ZIP_BYTES = 512 << 20; // the release zip carries one ~20 MB binary.
const RELEASE_VERSION_PATTERN = /^\d+\.\d+\.\d+$/;

// ZIP record signatures in little-endian byte order, compared without packing
// them into a JavaScript number.
const ZIP_SIGNATURES = Object.freeze({
  EOCD: Object.freeze([0x50, 0x4b, 0x05, 0x06]),
  CENTRAL_DIRECTORY: Object.freeze([0x50, 0x4b, 0x01, 0x02]),
  LOCAL_HEADER: Object.freeze([0x50, 0x4b, 0x03, 0x04]),
});

const ERR = Object.freeze({
  CHECKSUM_MISMATCH: "ERR_DO_HARNESS_CHECKSUM_MISMATCH",
  MISSING_CHECKSUM: "ERR_DO_HARNESS_MISSING_CHECKSUM",
  DOWNLOAD_FAILED: "ERR_DO_HARNESS_DOWNLOAD",
  BAD_RESPONSE: "ERR_DO_HARNESS_BAD_RESPONSE",
  INVALID_ARCHIVE: "ERR_DO_HARNESS_INVALID_ARCHIVE",
  INVALID_VERSION: "ERR_DO_HARNESS_INVALID_VERSION",
});

/**
 * Base URL of the release assets, mirroring GitHub Releases layout:
 * `<base>/v<version>/<asset>`. Overridable so tests and CI can point at a
 * local fixture server.
 */
function releaseBaseUrl() {
  return (
    process.env.DO_HARNESS_RELEASE_BASE_URL ??
    "https://github.com/d-o-hub/do-harness/releases/download"
  );
}

/**
 * Cache root. On Windows this is the per-user cache dir (%LOCALAPPDATA%);
 * overridable with DO_HARNESS_CACHE_DIR (tests, CI).
 */
function cacheRoot() {
  if (process.env.DO_HARNESS_CACHE_DIR) {
    return process.env.DO_HARNESS_CACHE_DIR;
  }
  return path.join(
    process.env.LOCALAPPDATA || os.homedir(),
    "do-harness",
  );
}

function zipAssetName(version) {
  return `do-harness-v${version}-${WINDOWS_TARGET}.zip`;
}

function invalidArchive(message) {
  return Object.assign(new Error(message), { code: ERR.INVALID_ARCHIVE });
}

function readU16(buf, offset) {
  return buf.readUInt16LE(offset);
}

function readU32(buf, offset) {
  return buf.readUInt32LE(offset);
}

function matchesSignature(buf, offset, signature) {
  return (
    buf[offset] === signature[0] &&
    buf[offset + 1] === signature[1] &&
    buf[offset + 2] === signature[2] &&
    buf[offset + 3] === signature[3]
  );
}

/**
 * Minimal ZIP reader for the release archive. 7z stores the exe with
 * DEFLATE (method 8) or stored (method 0); only those two methods are
 * supported. The whole zip is authenticated by the release checksum before
 * extraction, so per-entry CRCs are not re-verified here.
 */
function extractBinary(zipBuffer) {
  // End-of-central-directory: the last 22 bytes, but a trailing comment can
  // push it up to 65557 bytes from the end.
  const eocdStart = Math.max(0, zipBuffer.length - 65557);
  let eocd = -1;
  for (let i = zipBuffer.length - 22; i >= eocdStart; i--) {
    if (matchesSignature(zipBuffer, i, ZIP_SIGNATURES.EOCD)) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) {
    throw invalidArchive("release zip has no end-of-central-directory record");
  }
  const count = readU16(zipBuffer, eocd + 10);
  let offset = readU32(zipBuffer, eocd + 16);
  for (let i = 0; i < count; i++) {
    if (!matchesSignature(zipBuffer, offset, ZIP_SIGNATURES.CENTRAL_DIRECTORY)) {
      throw invalidArchive("release zip has a corrupt central directory");
    }
    const method = readU16(zipBuffer, offset + 10);
    const compSize = readU32(zipBuffer, offset + 20);
    const nameLen = readU16(zipBuffer, offset + 28);
    const extraLen = readU16(zipBuffer, offset + 30);
    const commentLen = readU16(zipBuffer, offset + 32);
    const localOffset = readU32(zipBuffer, offset + 42);
    const name = zipBuffer.toString("utf8", offset + 46, offset + 46 + nameLen);
    if (name.endsWith(`/${BINARY_NAME}`)) {
      if (!matchesSignature(zipBuffer, localOffset, ZIP_SIGNATURES.LOCAL_HEADER)) {
        throw invalidArchive("release zip has a corrupt local header");
      }
      const dataStart =
        localOffset +
        30 +
        readU16(zipBuffer, localOffset + 26) +
        readU16(zipBuffer, localOffset + 28);
      const data = zipBuffer.subarray(dataStart, dataStart + compSize);
      if (method === 0) {
        return Buffer.from(data);
      }
      if (method === 8) {
        return zlib.inflateRawSync(data);
      }
      throw invalidArchive(`unsupported compression method ${method}`);
    }
    offset += 46 + nameLen + extraLen + commentLen;
  }
  throw invalidArchive(`${BINARY_NAME} not found in the release zip`);
}

/** SHA-256 of a buffer as lowercase hex. */
function sha256Hex(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

/** Constant-time hex digest comparison; false on any shape mismatch. */
function digestsEqual(a, b) {
  const ba = Buffer.from(a, "hex");
  const bb = Buffer.from(b, "hex");
  if (ba.length !== 32 || bb.length !== 32) {
    return false;
  }
  return crypto.timingSafeEqual(ba, bb);
}

/** Parse `sha256sum`-style output: `<64-hex>  <filename>` per line. */
function parseChecksums(text) {
  const digests = new Map();
  for (const line of text.split(/\r?\n/)) {
    const match = /^([0-9a-fA-F]{64})[ \t]+(\S+)/.exec(line.trimEnd());
    if (match) {
      digests.set(match[2], match[1].toLowerCase());
    }
  }
  return digests;
}

/** One GET, following redirects; resolves with a Buffer of the body. */
function fetchWithRedirects(url, { maxBytes }, remaining = MAX_REDIRECTS) {
  return new Promise((resolve, reject) => {
    const mod = url.startsWith("https:") ? https : http;
    const req = mod.get(url, { headers: { accept: "*/*" } }, (res) => {
      const status = res.statusCode || 0;
      if (status >= 300 && status < 400 && res.headers.location) {
        res.resume();
        if (remaining <= 0) {
          reject(
            Object.assign(new Error(`too many redirects fetching ${url}`), {
              code: ERR.DOWNLOAD_FAILED,
            }),
          );
          return;
        }
        resolve(
          fetchWithRedirects(
            new URL(res.headers.location, url).toString(),
            { maxBytes },
            remaining - 1,
          ),
        );
        return;
      }
      if (status !== 200) {
        res.resume();
        reject(
          Object.assign(new Error(`HTTP ${status} fetching ${url}`), {
            code: ERR.BAD_RESPONSE,
          }),
        );
        return;
      }
      const chunks = [];
      let size = 0;
      let settled = false;
      const fail = (err) => {
        if (!settled) {
          settled = true;
          reject(err);
        }
      };
      res.on("data", (chunk) => {
        if (settled) {
          return;
        }
        size += chunk.length;
        if (maxBytes && size > maxBytes) {
          req.destroy();
          fail(
            Object.assign(
              new Error(`response from ${url} exceeded ${maxBytes} bytes`),
              { code: ERR.BAD_RESPONSE },
            ),
          );
          return;
        }
        chunks.push(chunk);
      });
      res.on("end", () => {
        if (!settled) {
          settled = true;
          resolve(Buffer.concat(chunks));
        }
      });
    });
    req.on("error", (err) => {
      reject(Object.assign(err, { code: ERR.DOWNLOAD_FAILED }));
    });
    req.setTimeout(REQUEST_TIMEOUT_MS, () => {
      req.destroy(
        Object.assign(
          new Error(`request to ${url} timed out after ${REQUEST_TIMEOUT_MS}ms`),
          { code: ERR.DOWNLOAD_FAILED },
        ),
      );
    });
  });
}

/** The cached exe when it exists and still matches its recorded digest. */
function cachedBinary(exePath, sidecarPath) {
  if (!fs.existsSync(exePath) || !fs.existsSync(sidecarPath)) {
    return null;
  }
  let expected;
  try {
    expected = fs.readFileSync(sidecarPath, "utf8").trim().toLowerCase();
  } catch {
    return null;
  }
  if (!/^[0-9a-f]{64}$/.test(expected)) {
    return null;
  }
  let actual;
  try {
    actual = sha256Hex(fs.readFileSync(exePath));
  } catch {
    return null;
  }
  if (!digestsEqual(actual, expected)) {
    throw Object.assign(
      new Error(
        `cached binary ${exePath} failed its recorded SHA-256 check; ` +
          "delete the cache directory to re-download",
      ),
      { code: ERR.CHECKSUM_MISMATCH },
    );
  }
  return exePath;
}

/**
 * Ensures the Windows binary is available locally and returns its path,
 * downloading and verifying it on the first run. Fails closed on a checksum
 * mismatch, a missing checksum entry, or a failed download (no execution).
 */
async function ensureWindowsBinary({ version, baseUrl, cacheDir }) {
  if (typeof version !== "string" || !RELEASE_VERSION_PATTERN.test(version)) {
    throw Object.assign(
      new Error("release version must be plain MAJOR.MINOR.PATCH"),
      { code: ERR.INVALID_VERSION },
    );
  }
  const asset = zipAssetName(version);
  const base = baseUrl ?? releaseBaseUrl();
  const root = cacheDir ?? cacheRoot();
  const dir = path.join(root, "do-harness", `v${version}`);
  const exePath = path.join(dir, BINARY_NAME);
  const sidecarPath = `${exePath}.sha256`;

  const cached = cachedBinary(exePath, sidecarPath);
  if (cached) {
    return cached;
  }

  const checksums = await fetchWithRedirects(
    `${base}/v${version}/checksums.txt`,
    { maxBytes: MAX_CHECKSUMS_BYTES },
  );
  const digests = parseChecksums(checksums.toString("utf8"));
  const expected = digests.get(asset);
  if (!expected) {
    throw Object.assign(
      new Error(
        `release checksums.txt for v${version} has no entry for ${asset}`,
      ),
      { code: ERR.MISSING_CHECKSUM },
    );
  }

  const archive = await fetchWithRedirects(`${base}/v${version}/${asset}`, {
    maxBytes: MAX_ZIP_BYTES,
  });
  const actual = sha256Hex(archive);
  if (!digestsEqual(actual, expected)) {
    throw Object.assign(
      new Error(
        `SHA-256 mismatch for ${asset}: expected ${expected}, got ${actual}`,
      ),
      { code: ERR.CHECKSUM_MISMATCH },
    );
  }

  const binary = extractBinary(archive);
  fs.mkdirSync(dir, { recursive: true });
  const tmpPath = path.join(dir, `.${BINARY_NAME}.${process.pid}.tmp`);
  fs.writeFileSync(tmpPath, binary, { mode: 0o755 });
  try {
    fs.renameSync(tmpPath, exePath);
  } catch {
    // A concurrent first run may have landed the file first; the cache is
    // verified on the next use, so either outcome is safe.
    fs.rmSync(tmpPath, { force: true });
  }
  fs.writeFileSync(sidecarPath, `${sha256Hex(binary)}\n`);
  return exePath;
}

module.exports = {
  ERR,
  BINARY_NAME,
  WINDOWS_TARGET,
  cacheRoot,
  digestsEqual,
  ensureWindowsBinary,
  extractBinary,
  fetchWithRedirects,
  parseChecksums,
  releaseBaseUrl,
  sha256Hex,
  zipAssetName,
};
