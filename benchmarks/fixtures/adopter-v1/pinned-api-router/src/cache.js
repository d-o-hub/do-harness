// Small fixed-capacity cache; part of the edit-after-green surface.
const DEFAULT_MAX_ENTRIES = 128;

export class Cache {
  constructor(maxEntries = DEFAULT_MAX_ENTRIES) {
    this.maxEntries = maxEntries;
    this.map = new Map();
  }

  key(pathname, body, query) {
    return JSON.stringify([pathname, body, query]);
  }

  get(pathname, body, query) {
    const hit = this.map.get(this.key(pathname, body, query));
    if (hit) {
      this.map.delete(this.key(pathname, body, query));
      this.map.set(this.key(pathname, body, query), hit);
    }
    return hit ?? null;
  }

  set(pathname, body, query, result) {
    const key = this.key(pathname, body, query);
    this.map.set(key, result);
    if (this.map.size > this.maxEntries) {
      this.map.delete(this.map.keys().next().value);
    }
  }
}
