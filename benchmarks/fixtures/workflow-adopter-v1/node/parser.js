// Parse `[INFO|WARN|ERROR] component: message` build log lines.

const SEVERITY_RANK = { INFO: 0, WARN: 1, ERROR: 2 };

export function parseLine(line) {
  const trimmed = line.trim();
  if (!trimmed.startsWith("[")) return null;
  const close = trimmed.indexOf("]");
  if (close < 0) return null;
  const severity = trimmed.slice(1, close).trim();
  if (!(severity in SEVERITY_RANK)) return null;
  const after = trimmed.slice(close + 1).trim();
  const colon = after.indexOf(":");
  if (colon < 0) return null;
  const component = after.slice(0, colon).trim();
  const message = after.slice(colon + 1).trim();
  if (!component || !message) return null;
  return { severity, component, message };
}

export function parse(text) {
  const records = [];
  for (const line of text.split(/\r?\n/)) {
    const record = parseLine(line);
    if (record) records.push(record);
  }
  return records;
}

export const severityRank = SEVERITY_RANK;
