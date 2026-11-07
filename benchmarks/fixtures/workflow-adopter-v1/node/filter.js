// Severity/component/message filtering that preserves record order.

import { severityRank } from "./parser.js";

export function apply(records, { min = "INFO", component = null, message = null } = {}) {
  const floor = severityRank[min];
  return records.filter((record) => {
    if (severityRank[record.severity] < floor) return false;
    if (component !== null && record.component !== component) return false;
    if (message !== null && !record.message.toLowerCase().includes(message.toLowerCase())) return false;
    return true;
  });
}
