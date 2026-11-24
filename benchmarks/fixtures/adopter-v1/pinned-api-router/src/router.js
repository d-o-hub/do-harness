// Synthetic pinned adopter fixture: a small dependency-free API router.
// The module surface is checked against types/router.d.ts by build-check.js.
import { validate } from "./validate.js";
import { Cache } from "./cache.js";

export class Router {
  constructor(routes) {
    this.routes = routes ?? [];
    this.cache = new Cache();
  }

  dispatch(pathname, body, options = {}) {
    const route = this.routes.find((r) => r.pattern === pathname);
    if (!route) {
      return { status: 404, body: { error: "not_found" } };
    }
    const { valid, errors } = validate(route.input, body);
    if (!valid) {
      return { status: 422, body: { error: "invalid_input", fields: errors } };
    }
    // Baseline gap: query parameters are accepted but never forwarded to the
    // handler, and validation never normalizes whitespace-only strings.
    const cached = this.cache.get(pathname, body, {});
    if (cached) {
      return { status: 200, body: cached, fromCache: true };
    }
    const result = route.handler(body);
    this.cache.set(pathname, body, {}, result);
    return { status: 200, body: result, fromCache: false };
  }
}
