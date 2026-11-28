# do-harness-sdk (TypeScript / JavaScript)

Typed, zero-dependency bindings for the [do-harness](../../README.md) agent
harness CLI. The SDK spawns the `do-harness` binary (never a shell), asks for
JSON output, and parses it into typed models (TypeScript declarations in
`index.d.ts`).

- **Fail closed.** A missing binary raises `BinaryNotFoundError`; output that
  is not the expected JSON document raises `ParseError`; a non-zero exit
  raises `CliError` (or `VerifyFailed` / `StatusNotGreen`, which carry the
  parsed report in `.result`). No partial or silently-green results.
- **Zero runtime dependencies.** Node >= 18 only; tests use `node --test`.
- **Version coupling.** The package version mirrors the CLI release it
  targets (`0.4.8` ↔ CLI `0.4.x`). The SDK parses the CLI's documented JSON
  envelope contract (`SCHEMA_VERSION = 1`); `assertSchemaCompatible()` fails
  closed when the installed CLI is from a different release line.

## Installation

<!-- example: id=sdk_ts_install classification=illustrative reason="TypeScript SDK install command" -->
```sh
npm install do-harness-sdk
```

The harness binary is resolved from the `binary` option, then
`$DO_HARNESS_BIN`, then `do-harness` on `PATH`.

## Usage

<!-- example: id=sdk_ts_usage classification=illustrative reason="TypeScript SDK usage snippet" -->
```js
import { HarnessClient, VerifyFailed } from "do-harness-sdk";

const client = new HarnessClient(); // or { root: "/path/to/repo" }

// The gate: returns a VerifyResult; a red run raises VerifyFailed carrying
// the parsed result so you can inspect exactly which sensors failed.
try {
  const result = client.verify({ signalSet: "verification", strict: true });
  console.log(result.ok, result.failed);
} catch (err) {
  if (err instanceof VerifyFailed) {
    for (const sensor of err.result.sensors) {
      if (!sensor.ok) console.log(sensor.name, sensor.exitCode);
    }
  }
}

// Evidence freshness (status).
const status = client.status({ signalSet: "verification" }); // green -> StatusResult
// red/stale/missing raise StatusNotGreen; use err.result.state/err.result.reason

// Sensor selection without executing anything.
const explained = client.explain({ signalSet: "verification" });

// Strike state: fail-fast error signatures (`errors list`).
const strikes = client.errorsList({ scope: "branch:main" });
console.log(strikes[0].signature, strikes[0].attemptCount);

// Task board state (`task list` / `task export`).
const board = client.taskList({ status: "pending" });
const snapshot = client.taskExport();

// CLI version compatibility.
const cli = client.assertSchemaCompatible();
console.log(cli.version);
```

### LangChain

The Python SDK ships a LangChain `Tool` adapter (see
[`../sdk-python/README.md`](../sdk-python/README.md)); in TypeScript, wrap the
client in a `DynamicStructuredTool`:

<!-- example: id=sdk_ts_langchain classification=illustrative reason="LangChain DynamicStructuredTool wrap" -->
```js
import { DynamicStructuredTool } from "@langchain/core/tools";
import { z } from "zod";
import { HarnessClient } from "do-harness-sdk";

const client = new HarnessClient();
const verifyTool = new DynamicStructuredTool({
  name: "do-harness-verify",
  description: "Run do-harness verify and return the sensor report.",
  schema: z.object({ signal_set: z.string().optional(), strict: z.boolean().optional() }),
  func: async ({ signal_set, strict }) => client.verify({ signalSet: signal_set, strict }),
});
```

## Publishing

The package version mirrors the CLI release it targets. Run
`bash scripts/check-sdk-pack.sh` from the repository root to validate the
tarball. See [SDK publishing](../../docs/releasing.md#sdk-publishing) for
the initial npm publication and trusted publisher setup.

## Development

<!-- example: id=sdk_ts_dev classification=illustrative reason="TypeScript SDK test invocation" -->
```sh
npm test
```

Tests use a stub binary and fixture JSON documents; no real harness is needed.
