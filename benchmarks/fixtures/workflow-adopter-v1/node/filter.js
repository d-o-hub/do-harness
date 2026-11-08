// Severity/component/message filtering that preserves record order.

import { severityRank } from "./parser.js";

export function apply(records, { min = "INFO", component = null, message = null } = {}) {
  const floor = severityRank.get(min);
  if (floor === undefined) throw new Error(`unknown severity: ${min}`);
  return records.filter((record) => {
    const rank = severityRank.get(record.severity);
    if (rank === undefined) throw new Error(`unknown severity: ${record.severity}`);
    if (rank < floor) return false;
    if (component !== null && record.component !== component) return false;
    if (message !== null && !record.message.toLowerCase().includes(message.toLowerCase())) return false;
    return true;
  });
}
