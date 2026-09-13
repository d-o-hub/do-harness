"use strict";

// Test helper: patch process.platform before the shim loads so the
// unsupported-platform branch can be exercised on any host.

Object.defineProperty(process, "platform", { value: "win32" });
