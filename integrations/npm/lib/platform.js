"use strict";

// Maps the running Node platform/arch to the prebuilt platform package.

const PLATFORM_PACKAGES = Object.freeze({
  "linux-x64": "do-harness-linux-x64",
  "linux-arm64": "do-harness-linux-arm64",
  "darwin-x64": "do-harness-darwin-x64",
  "darwin-arm64": "do-harness-darwin-arm64",
  "win32-x64": "do-harness-win32-x64",
});

/**
 * Returns the platform package for `platform`/`arch`, or null when the
 * platform has no prebuilt binary.
 */
function platformPackage(platform, arch) {
  return PLATFORM_PACKAGES[`${platform}-${arch}`] ?? null;
}

/** Returns the binary filename for a Node platform. */
function binaryName(platform) {
  return platform === "win32" ? "do-harness.exe" : "do-harness";
}

module.exports = { PLATFORM_PACKAGES, binaryName, platformPackage };
