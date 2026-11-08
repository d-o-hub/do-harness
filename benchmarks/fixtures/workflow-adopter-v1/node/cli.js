#!/usr/bin/env node
// Offline build-log filtering CLI used as the Node adopter fixture.

import { readFileSync } from "node:fs";
import { parse } from "./parser.js";
import { apply } from "./filter.js";
import { summarize } from "./summary.js";

const USAGE = `Usage: cli.js [--min-severity INFO|WARN|ERROR] [--component NAME]
                   [--message SUBSTR] [--file PATH]
Filter \`[INFO|WARN|ERROR] component: message\` lines and print a JSON report.
--help  Print this usage text and exit 0.
`;

function parseArgs(argv) {
  // The argument vector is consumed through its own iterator, so a position is
  // never used as a key: the fixture parses untrusted argv without any computed
  // member access, and each option lands in a named local returned as a literal.
  const tokens = argv.values();
  let min = "INFO";
  let component = null;
  let message = null;
  let file = null;
  let help = false;

  for (let step = tokens.next(); !step.done; step = tokens.next()) {
    const flag = step.value;
    switch (flag) {
      case "--help":
      case "-h":
        help = true;
        break;
      case "--min-severity":
      case "--component":
      case "--message":
      case "--file": {
        const valueStep = tokens.next();
        if (valueStep.done) throw new Error(`${flag} requires a value`);
        const value = valueStep.value;
        switch (flag) {
          case "--min-severity":
            if (!["INFO", "WARN", "ERROR"].includes(value)) {
              throw new Error(`unknown severity: ${value}`);
            }
            min = value;
            break;
          case "--component":
            component = value;
            break;
          case "--message":
            message = value;
            break;
          default:
            file = value;
            break;
        }
        break;
      }
      default:
        if (flag.startsWith("-") && flag !== "-") throw new Error(`unknown option: ${flag}`);
        throw new Error(`unexpected positional argument: ${flag}`);
    }
  }
  return { min, component, message, file, help };
}
function readInput(options) {
  // The corpus runs this CLI with a log path it created itself; `--file` is the
  // documented way to point it at that fixture log.
  // eslint-disable-next-line security/detect-non-literal-fs-filename
  if (options.file !== null) return readFileSync(options.file, "utf8");
  return readFileSync(0, "utf8");
}

let parsed;
try {
  parsed = parseArgs(process.argv.slice(2));
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exit(2);
}
if (parsed.help) {
  process.stdout.write(USAGE);
  process.exit(0);
}

let source;
try {
  source = readInput(parsed);
} catch (error) {
  process.stderr.write(`${error.message}\n`);
  process.exit(2);
}

const records = parse(source);
const retained = apply(records, parsed);
const report = summarize(retained);
process.stdout.write(`${JSON.stringify(report)}\n`);
