"use strict";

// Maps the running Node platform/arch to the prebuilt platform package.
// Windows is intentionally absent: managed git hooks are bash and the
// release matrix does not ship Windows binaries.

const PLATFORM_PACKAGES = Object.freeze({
  "linux-x64": "do-harness-linux-x64",
  "linux-arm64": "do-harness-linux-arm64",
  "darwin-x64": "do-harness-darwin-x64",
  "darwin-arm64": "do-harness-darwin-arm64",
});

const BINARY_RELATIVE_PATH = ["bin", "do-harness"];

/**
 * Returns the platform package for `platform`/`arch`, or null when the
 * platform has no prebuilt binary.
 */
function platformPackage(platform, arch) {
  return PLATFORM_PACKAGES[`${platform}-${arch}`] ?? null;
}

module.exports = { PLATFORM_PACKAGES, BINARY_RELATIVE_PATH, platformPackage };
