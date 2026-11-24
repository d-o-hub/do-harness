"use strict";

// Shared hermetic helpers for the npm wrapper tests. The zip builder mirrors
// the release archive layout produced by the release workflow (7z: a single
// `do-harness-v<version>-x86_64-pc-windows-msvc` directory containing the
// binary). The network helpers serve fixtures over localhost so no test
// touches the real registry or GitHub.

import http from "node:http";
import net from "node:net";
import { spawn } from "node:child_process";
import zlib from "node:zlib";
import crypto from "node:crypto";

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c;
  }
  return table;
})();

export function crc32(buf) {
  let c = -1;
  for (const b of buf) {
    c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  }
  return (c ^ -1) >>> 0;
}

export function sha256Hex(buf) {
  return crypto.createHash("sha256").update(buf).digest("hex");
}

/**
 * Builds a minimal ZIP with DEFLATE entries (method 8) or stored entries
 * (method 0), one entry per `{ name, data }` item.
 */
export function makeZip(files, method = 8) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const file of files) {
    const nameBuf = Buffer.from(file.name, "utf8");
    const raw = file.data;
    const comp = method === 0 ? raw : zlib.deflateRawSync(raw);
    const crc = crc32(raw);
    const loc = Buffer.alloc(30);
    loc.writeUInt32LE(0x04034b50, 0);
    loc.writeUInt16LE(20, 4);
    loc.writeUInt16LE(0x0800, 6);
    loc.writeUInt16LE(method, 8);
    loc.writeUInt32LE(crc, 14);
    loc.writeUInt32LE(comp.length, 18);
    loc.writeUInt32LE(raw.length, 22);
    loc.writeUInt16LE(nameBuf.length, 26);
    loc.writeUInt16LE(0, 28);
    local.push(loc, nameBuf, comp);
    const cen = Buffer.alloc(46);
    cen.writeUInt32LE(0x02014b50, 0);
    cen.writeUInt16LE(20, 4);
    cen.writeUInt16LE(20, 6);
    cen.writeUInt16LE(0x0800, 8);
    cen.writeUInt16LE(method, 10);
    cen.writeUInt32LE(crc, 16);
    cen.writeUInt32LE(comp.length, 20);
    cen.writeUInt32LE(raw.length, 24);
    cen.writeUInt16LE(nameBuf.length, 28);
    cen.writeUInt16LE(0, 30);
    cen.writeUInt16LE(0, 32);
    cen.writeUInt32LE(0, 38);
    cen.writeUInt32LE(offset, 42);
    central.push(cen, nameBuf);
    offset += 30 + nameBuf.length + comp.length;
  }
  const cdStart = Buffer.concat(local).length;
  const cd = Buffer.concat(central);
  const eocd = Buffer.alloc(22);
  eocd.writeUInt32LE(0x06054b50, 0);
  eocd.writeUInt16LE(files.length, 8);
  eocd.writeUInt16LE(files.length, 10);
  eocd.writeUInt32LE(cd.length, 12);
  eocd.writeUInt32LE(cdStart, 16);
  return Buffer.concat([...local, cd, eocd]);
}

/**
 * In-process fixture server for unit tests. `routes` maps URL pathnames to
 * body Buffers/strings; `hits` counts requests per pathname.
 */
export function startServer(routes) {
  const hits = new Map();
  const server = http.createServer((req, res) => {
    const pathname = new URL(req.url, "http://127.0.0.1").pathname;
    hits.set(pathname, (hits.get(pathname) || 0) + 1);
    const body = routes[pathname];
    if (body === undefined) {
      res.writeHead(404);
      res.end("not found");
      return;
    }
    res.writeHead(200, { "content-type": "application/octet-stream" });
    res.end(body);
  });
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      resolve({
        server,
        hits,
        baseUrl: `http://127.0.0.1:${server.address().port}`,
      });
    });
  });
}

/** A port that is guaranteed to refuse connections (bound, then closed). */
export function deadPort() {
  return new Promise((resolve) => {
    const probe = net.createServer();
    probe.listen(0, "127.0.0.1", () => {
      const port = probe.address().port;
      probe.close(() => resolve(port));
    });
  });
}

// A tiny static file server run as a child process: shim tests spawn the
// shim with spawnSync, which blocks this process's event loop, so the
// fixture server must live in its own process.
const FIXTURE_SERVER = `
"use strict";
const fs = require("node:fs");
const http = require("node:http");
const path = require("node:path");
const root = path.resolve(process.argv[1]);
const server = http.createServer((req, res) => {
  const pathname = decodeURIComponent(new URL(req.url, "http://127.0.0.1").pathname);
  const rel = path.normalize(pathname).replace(/^[/\\\\]+/, "");
  const file = path.join(root, rel);
  if (!file.startsWith(root) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    res.writeHead(404);
    res.end("not found");
    return;
  }
  res.writeHead(200, { "content-type": "application/octet-stream" });
  fs.createReadStream(file).pipe(res);
});
server.listen(0, "127.0.0.1", () => {
  process.stdout.write(String(server.address().port) + "\\n");
});
`;

/**
 * Starts the fixture server as a child process serving `dir` (the fixture
 * release tree, e.g. `v<version>/checksums.txt`). Resolves with the child
 * handle and the base URL.
 */
export function startFixtureServer(dir) {
  const child = spawn(process.execPath, ["-e", FIXTURE_SERVER, dir], {
    stdio: ["ignore", "pipe", "inherit"],
  });
  return new Promise((resolve, reject) => {
    let buf = "";
    let settled = false;
    child.stdout.on("data", (d) => {
      buf += d.toString();
      const match = /^(\d+)/.exec(buf);
      if (match && !settled) {
        settled = true;
        resolve({ child, baseUrl: `http://127.0.0.1:${match[1]}` });
      }
    });
    child.on("exit", (code) => {
      if (!settled) {
        settled = true;
        reject(new Error(`fixture server exited early (code ${code})`));
      }
    });
  });
}
