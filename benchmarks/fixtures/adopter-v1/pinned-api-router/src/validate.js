// Field validation for routed inputs (part of the multi-file fix surface).
const TYPES = new Set(["string", "number", "boolean"]);

export function validate(schema, value) {
  const errors = [];
  if (typeof value !== "object" || value === null) {
    return { valid: false, errors: ["body must be an object"] };
  }
  for (const [key, rule] of Object.entries(schema ?? {})) {
    if (rule.required && value[key] === undefined) {
      errors.push(`${key} is required`);
      continue;
    }
    if (value[key] === undefined) continue;
    if (rule.type && !TYPES.has(rule.type)) {
      errors.push(`${key} has unknown type ${rule.type}`);
      continue;
    }
    if (rule.type === "string" && typeof value[key] !== "string") {
      errors.push(`${key} must be a string`);
    } else if (rule.type === "number" && typeof value[key] !== "number") {
      errors.push(`${key} must be a number`);
    }
  }
  return { valid: errors.length === 0, errors };
}

export function normalize(value) {
  if (typeof value === "string") {
    return value.trim();
  }
  return value;
}
