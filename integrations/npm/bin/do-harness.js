#!/usr/bin/env node
"use strict";

// Thin launcher: resolve the prebuilt platform package installed through
// optionalDependencies and exec the real binary with inherited stdio.
// The model never sees this file; `npx do-harness ...` is the entrypoint.

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const { binaryName, platformPackage } = require("../lib/platform.js");

const pkg = platformPackage(process.platform, process.arch);
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
