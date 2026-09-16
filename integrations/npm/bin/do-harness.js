#!/usr/bin/env node
"use strict";

// Thin launcher: resolve the prebuilt platform package installed through
// optionalDependencies and exec the real binary with inherited stdio.
// The model never sees this file; `npx do-harness ...` is the entrypoint.

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const { UNAVAILABLE_PACKAGES, binaryName, platformPackage } = require("../lib/platform.js");

// Test seam: lets the wrapper tests assert the guidance for a platform other
// than the host (e.g. the unavailable Windows package, checked on Linux CI).
// Ignored in normal use.
const pkg =
  process.env.DO_HARNESS_TEST_PLATFORM_PACKAGE ??
  platformPackage(process.platform, process.arch);
if (!pkg) {
  console.error(
    `do-harness: no prebuilt binary for ${process.platform}/${process.arch}.`,
  );
  console.error(
    "Install on a supported platform, build from source, or download a release:",
  );
  console.error("  https://github.com/d-o-hub/do-harness/releases");
  process.exit(1);
}

let binary;
try {
  const packageJson = require.resolve(`${pkg}/package.json`);
  binary = path.join(
    path.dirname(packageJson),
    "bin",
    binaryName(process.platform),
  );
} catch {
  if (UNAVAILABLE_PACKAGES.has(pkg)) {
    // Distinct from "optional dependencies disabled": npm installed cleanly
    // because the pinned package is not on the registry at all, so
    // --include=optional cannot help. Point at the channel that does work.
    console.error(
      `do-harness: no npm package is published for ${process.platform}/${process.arch} yet (${pkg} is unavailable).`,
    );
    console.error(
      "Install the prebuilt binary from the GitHub release instead (see the",
    );
    console.error(
      "Windows section of docs/adoption.md), or build from source with",
    );
    console.error("`cargo install do-harness`.");
    process.exit(1);
  }
  console.error(
    `do-harness: platform package ${pkg} is not installed (optional dependencies may be disabled).`,
  );
  console.error(`Try: npm install --include=optional do-harness`);
  process.exit(1);
}

// npm may normalize archive modes; make sure the binary is executable.
try {
  fs.accessSync(binary, fs.constants.X_OK);
} catch {
  try {
    fs.chmodSync(binary, 0o755);
  } catch {
    // The spawn below reports the real error if this fails.
  }
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`do-harness: failed to run ${binary}: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
