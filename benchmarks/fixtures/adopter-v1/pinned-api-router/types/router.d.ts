// Declaration surface checked by scripts/build-check.js (TS-equivalent).
export interface RouteInput {
  [field: string]: { type?: "string" | "number" | "boolean"; required?: boolean };
}

export interface Route {
  pattern: string;
  input: RouteInput;
  handler(body: unknown, query: Record<string, string>): unknown;
}

export interface DispatchResult {
  status: number;
  body: unknown;
  fromCache?: boolean;
}

export declare class Router {
  constructor(routes?: Route[]);
  dispatch(pathname: string, body: unknown, options?: {
    query?: Record<string, string>;
  }): DispatchResult;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
}

export declare function validate(
  schema: RouteInput,
  value: unknown
): ValidationResult;

export declare function normalize(value: unknown): unknown;

export declare class Cache {
  constructor(maxEntries?: number);
  get(pathname: string, body: unknown, query: Record<string, string>): unknown;
  set(
    pathname: string,
    body: unknown,
    query: Record<string, string>,
    result: unknown
  ): void;
}
