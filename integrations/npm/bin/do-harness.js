#!/usr/bin/env node
"use strict";

// Thin launcher: resolve the prebuilt platform package installed through
// optionalDependencies and exec the real binary with inherited stdio.
// The model never sees this file; `npx do-harness ...` is the entrypoint.

const { spawnSync } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const { BINARY_RELATIVE_PATH, platformPackage } = require("../lib/platform.js");

const pkg = platformPackage(process.platform, process.arch);
if (!pkg) {
  console.error(
    `do-harness: no prebuilt binary for ${process.platform}/${process.arch}.`,
  );
  console.error("Install with the shell installer or from source instead:");
  console.error(
    "  curl -fsSL https://raw.githubusercontent.com/d-o-hub/do-harness/main/scripts/install.sh | sh",
  );
  process.exit(1);
}

let binary;
try {
  const packageJson = require.resolve(`${pkg}/package.json`);
  binary = path.join(path.dirname(packageJson), ...BINARY_RELATIVE_PATH);
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
