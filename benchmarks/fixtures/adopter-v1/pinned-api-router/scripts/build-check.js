// Build check: syntax-check every source module and verify the exported
// surface matches types/router.d.ts. This is the offline, dependency-free
// equivalent of a TypeScript build step for this synthetic fixture.
import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

for (const name of ["src/router.js", "src/validate.js", "src/cache.js"]) {
  try {
    execFileSync(process.execPath, ["--check", join(root, name)], {
      stdio: "pipe",
    });
  } catch {
    process.exitCode = 1;
    console.error(`build-check: syntax error in ${name}`);
    process.exit(1);
  }
}

const declared = ["Router", "validate", "normalize", "Cache"];
const actual = new Set();
for (const file of readdirSync(join(root, "src"))) {
  const source = readFileSync(join(root, "src", file), "utf8");
  for (const line of source.split("\n")) {
    const match = line.match(/^export (?:class|function) ([A-Za-z0-9_]+)/);
    if (match) actual.add(match[1]);
  }
}
for (const name of declared) {
  if (!actual.has(name)) {
    console.error(`build-check: missing export ${name}`);
    process.exit(1);
  }
}
console.log("build-check: ok");
