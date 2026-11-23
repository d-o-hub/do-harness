// Test runner for the adopter corpus. Prints one JSON object with a boolean
// for each case target plus the shared regression result.
import { Router } from "../src/router.js";
import { validate, normalize } from "../src/validate.js";
import { Cache } from "../src/cache.js";

const routes = [
  {
    pattern: "/users",
    input: { name: { type: "string", required: true } },
    handler(body, query = {}) {
      return { id: 7, name: normalize(String(body.name)), q: query.q ?? null };
    },
  },
];

function run(label, fn) {
  try {
    return fn();
  } catch (error) {
    console.error(`suite ${label} crashed: ${error}`);
    return false;
  }
}

const regression = run("regression", () => {
  const router = new Router(routes);
  const missing = router.dispatch("/users", {});
  if (missing.status !== 422) return false;
  const unknown = router.dispatch("/nope", {});
  if (unknown.status !== 404) return false;
  const hit = router.dispatch("/users", { name: "ada" });
  const cached = router.dispatch("/users", { name: "ada" });
  return hit.status === 200 && cached.fromCache === true && hit.body.id === 7;
});

const multifile = run("multifile", () => {
  const router = new Router(routes);
  const withQuery = router.dispatch("/users", { name: "  ada  " }, { query: { q: "x" } });
  if (withQuery.status !== 200) return false;
  if (withQuery.body.name !== "ada") return false; // normalized
  if (withQuery.body.q !== "x") return false; // query forwarded to handler
  // Validation must reject whitespace-only required strings.
  const blank = router.dispatch("/users", { name: "   " });
  if (blank.status !== 422) return false;
  return true;
});

const editAfterGreen = run("editAfterGreen", () => {
  const cache = new Cache(2);
  const key = cache.key("/a", { n: 1 }, {});
  cache.set("/a", { n: 1 }, {}, "one");
  cache.set("/b", { n: 2 }, {}, "two");
  cache.set("/c", { n: 3 }, {}, "three");
  if (cache.get("/a", { n: 1 }, {}) !== null) return false; // evicted
  if (cache.get("/b", { n: 2 }, {}) !== "two") return false;
  if (cache.get("/c", { n: 3 }, {}) !== "three") return false;
  return key === cache.key("/a", { n: 1 }, {});
});

console.log(
  JSON.stringify({
    regression,
    multifile,
    editAfterGreen,
  })
);
