// Stable JSON report generation with counts and preserved record order.

export function summarize(records) {
  const bySeverity = { ERROR: 0, INFO: 0, WARN: 0 };
  for (const record of records) {
    bySeverity[record.severity] += 1;
  }
  return { total: records.length, by_severity: bySeverity, records };
}
