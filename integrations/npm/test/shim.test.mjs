import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import platform from "../lib/platform.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const npmRoot = path.resolve(here, "..");
const isWindows = process.platform === "win32";

test("platformPackage maps supported platforms", () => {
  assert.equal(platform.platformPackage("linux", "x64"), "do-harness-linux-x64");
  assert.equal(
    platform.platformPackage("linux", "arm64"),
    "do-harness-linux-arm64",
  );
  assert.equal(
    platform.platformPackage("darwin", "x64"),
    "do-harness-darwin-x64",
  );
  assert.equal(
    platform.platformPackage("darwin", "arm64"),
    "do-harness-darwin-arm64",
  );
  assert.equal(platform.platformPackage("win32", "x64"), "do-harness-win32-x64");
  assert.equal(platform.platformPackage("win32", "arm64"), null);
  assert.equal(platform.platformPackage("freebsd", "x64"), null);
  assert.equal(platform.platformPackage("linux", "ia32"), null);
});

test("binaryName appends .exe only on Windows", () => {
  assert.equal(platform.binaryName("win32"), "do-harness.exe");
  assert.equal(platform.binaryName("linux"), "do-harness");
  assert.equal(platform.binaryName("darwin"), "do-harness");
});

/** Stages a node_modules tree containing the shim and an optional platform package. */
function stage({ withPlatform, binary = null }) {
  const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "do-harness-npm-"));
  const metaDir = path.join(tmp, "node_modules", "do-harness");
  fs.mkdirSync(path.join(metaDir, "bin"), { recursive: true });
  fs.mkdirSync(path.join(metaDir, "lib"), { recursive: true });
  fs.copyFileSync(
    path.join(npmRoot, "bin", "do-harness.js"),
    path.join(metaDir, "bin", "do-harness.js"),
  );
  fs.copyFileSync(
    path.join(npmRoot, "lib", "platform.js"),
    path.join(metaDir, "lib", "platform.js"),
  );

  const pkg = platform.platformPackage(process.platform, process.arch);
  if (withPlatform && pkg) {
    const platformDir = path.join(tmp, "node_modules", pkg);
    const name = platform.binaryName(process.platform);
    fs.mkdirSync(path.join(platformDir, "bin"), { recursive: true });
    fs.writeFileSync(
      path.join(platformDir, "package.json"),
      JSON.stringify({ name: pkg, version: "0.1.0" }),
    );
    fs.writeFileSync(path.join(platformDir, "bin", name), binary);
    fs.chmodSync(path.join(platformDir, "bin", name), 0o755);
  }

  return { tmp, shim: path.join(metaDir, "bin", "do-harness.js") };
}

test("shim execs the platform binary with args and propagates the exit code", (t) => {
  if (!platform.platformPackage(process.platform, process.arch) || isWindows) {
    t.skip(`no POSIX fake binary on ${process.platform}/${process.arch}`);
    return;
  }
  const { shim } = stage({
    withPlatform: true,
    binary: '#!/bin/sh\necho "args:$*"\nexit 7\n',
  });

  const result = spawnSync(process.execPath, [shim, "verify", "--changed"], {
    encoding: "utf8",
  });

  assert.equal(result.status, 7);
  assert.match(result.stdout, /args:verify --changed/);
});

test("shim fails with guidance when the platform package is missing", (t) => {
  // Force a package that IS published but absent from the staged tree, so the
  // assertion is host-independent and exercises the "optional dependencies
  // disabled" path rather than the unavailable-platform path (which correctly
  // answers differently -- see the next test).
  const probe = "do-harness-linux-x64";
  const { shim } = stage({ withPlatform: false });

  const result = spawnSync(process.execPath, [shim, "version"], {
    encoding: "utf8",
    env: { ...process.env, DO_HARNESS_TEST_PLATFORM_PACKAGE: probe },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /is not installed/);
  assert.match(result.stderr, /--include=optional/);
});

/// A platform package that is mapped but not published on npm must not be
/// reported as "optional dependencies may be disabled": npm installs the meta
/// package cleanly (an absent optional dependency is not an install error), so
/// `--include=optional` cannot help. The shim must point at the GitHub release
/// instead.
///
/// The host platform is overridden through `DO_HARNESS_TEST_PLATFORM_PACKAGE`
/// (a seam used only by tests; ignored unless set) so the assertion holds on
/// every CI platform, including the ones where the win32 package is the real,
/// working path.
test("shim points unavailable npm platforms at the release channel", () => {
  const { shim } = stage({ withPlatform: false });

  const result = spawnSync(process.execPath, [shim, "version"], {
    encoding: "utf8",
    env: {
      ...process.env,
      DO_HARNESS_TEST_PLATFORM_PACKAGE: "do-harness-win32-x64",
    },
  });

  assert.equal(result.status, 1);
  assert.match(result.stderr, /is unavailable/);
  assert.match(result.stderr, /GitHub release/);
  assert.doesNotMatch(result.stderr, /--include=optional/);
});

test("shim rejects unsupported platforms with release guidance", () => {
  const { shim } = stage({ withPlatform: false });
  const preload = path.join(here, "fake-unsupported.cjs");

  const result = spawnSync(
    process.execPath,
    ["--require", preload, shim, "version"],
    { encoding: "utf8" },
  );

  assert.equal(result.status, 1);
  assert.match(result.stderr, /no prebuilt binary for freebsd/);
  assert.match(result.stderr, /releases/);
});
