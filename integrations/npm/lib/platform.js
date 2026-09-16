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

/**
 * Platform packages mapped above that are NOT published on npm.
 *
 * `do-harness-win32-x64` is blocked by npm's registry name screening
 * (HTTP 403 "Package name triggered spam detection") and the meta package's
 * pinned `optionalDependencies` therefore cannot resolve on Windows. The
 * Windows binary ships as a GitHub release zip instead. npm installs the meta
 * package successfully regardless (an absent optional dependency is not an
 * install error), so the shim must recognise this case and point at the
 * channel that works rather than suggesting `--include=optional`, which
 * cannot help.
 *
 * Keep this in sync with docs/adoption.md's Windows section; removing an
 * entry requires the corresponding package to be live on npm.
 */
const UNAVAILABLE_PACKAGES = Object.freeze(new Set(["do-harness-win32-x64"]));

/** Returns the binary filename for a Node platform. */
function binaryName(platform) {
  return platform === "win32" ? "do-harness.exe" : "do-harness";
}

module.exports = {
  PLATFORM_PACKAGES,
  UNAVAILABLE_PACKAGES,
  binaryName,
  platformPackage,
};
