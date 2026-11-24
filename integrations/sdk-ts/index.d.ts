// Type declarations for do-harness-sdk (JavaScript implementation in lib/sdk.mjs).

export const SDK_VERSION: string;
/** Envelope contract revision this SDK parses; couples to the CLI release. */
export const SCHEMA_VERSION: number;

export class HarnessError extends Error {
  exitCode: number | null;
  stderr: string;
  result: unknown;
  argv: string[] | null;
  constructor(message: string, options?: HarnessErrorOptions);
}

export interface HarnessErrorOptions {
  exitCode?: number | null;
  stderr?: string;
  result?: unknown;
  argv?: string[] | null;
}

export class BinaryNotFoundError extends HarnessError {}
export class ParseError extends HarnessError {}
export class CliError extends HarnessError {}
export class VerifyFailed extends CliError {}
export class StatusNotGreen extends CliError {}

export interface SensorResult {
  name: string;
  ok: boolean;
  exitCode: number | null;
  durationMs: number | null;
  severity: string;
  allowFailure: boolean;
  warned: boolean;
  execution: string;
  reason: string | null;
  outputBytes: number | null;
  outputTruncated: boolean;
}

export interface VerifyResult {
  ok: boolean;
  root: string;
  failed: string[];
  unrunnable: string[];
  sensors: SensorResult[];
}

export interface EvidenceState {
  workspaceFingerprint: string;
  policyFingerprint: string;
  failed: string[];
  inputMismatches: string[];
}

export interface StatusResult {
  state: "green" | "red" | "stale" | "running" | "missing" | string;
  set: string | null;
  reason: string;
  current: EvidenceState | null;
  recovery: string | null;
}

export interface SelectedSensor {
  name: string;
  reason: string;
}

export interface ExplainResult {
  set: string | null;
  selectionMode: string;
  changedFiles: string[];
  selected: SelectedSensor[];
  skipped: SelectedSensor[];
  discoveryError: string | null;
  since: string | null;
  baseSha: string | null;
  headSha: string | null;
  mergeBaseSha: string | null;
}

export interface StrikeInfo {
  id: number;
  signature: string;
  scope: string;
  attemptCount: number;
  taskId: number | null;
  message: string | null;
  createdAt: number | null;
}

export interface TaskRecord {
  id: number;
  title: string;
  status: string;
  parentId: number | null;
  method: string | null;
  subtaskIndex: number;
  precondition: string | null;
  createdAt: number | null;
  updatedAt: number | null;
}

export interface TaskSummary {
  pending: number;
  inProgress: number;
  done: number;
  failed: number;
}

export interface TaskList {
  summary: TaskSummary;
  tasks: TaskRecord[];
}

export interface TaskExport {
  exportedAt: number;
  summary: TaskSummary;
  tasks: TaskRecord[];
}

export interface CliVersion {
  name: string;
  version: string;
  commit: string | null;
  commitDate: string | null;
  dirty: boolean;
}

export interface HarnessClientOptions {
  /** Binary path or command name; defaults to $DO_HARNESS_BIN then "do-harness". */
  binary?: string;
  /** Repository root passed as --root. */
  root?: string | null;
}

export interface VerifyOptions {
  signalSet?: string;
  strict?: boolean;
  changed?: boolean;
  since?: string;
  failFast?: boolean;
  only?: string[];
  exclude?: string[];
  jobs?: number;
  extra?: string[];
}

export interface StatusOptions {
  signalSet?: string;
  evidence?: string;
  since?: string;
  extra?: string[];
}

export interface ExplainOptions {
  signalSet?: string;
  changed?: boolean;
  since?: string;
  extra?: string[];
}

export interface ErrorsListOptions {
  taskId?: string;
  scope?: string;
  extra?: string[];
}

export interface TaskListOptions {
  status?: "pending" | "in_progress" | "completed" | "failed" | string;
  method?: string;
  parent?: string;
  extra?: string[];
}

export class HarnessClient {
  constructor(options?: HarnessClientOptions);
  verify(options?: VerifyOptions): VerifyResult;
  status(options?: StatusOptions): StatusResult;
  explain(options?: ExplainOptions): ExplainResult;
  errorsList(options?: ErrorsListOptions): StrikeInfo[];
  taskList(options?: TaskListOptions): TaskList;
  taskExport(options?: { extra?: string[] }): TaskExport;
  version(): CliVersion;
  /** Fail closed when the installed CLI is from a different release line. */
  assertSchemaCompatible(): CliVersion;
}

export function parseSensorResult(obj: unknown): SensorResult;
export function parseVerifyResult(obj: unknown): VerifyResult;
export function parseStatusResult(obj: unknown): StatusResult;
export function parseExplainResult(obj: unknown): ExplainResult;
export function parseStrikeInfo(obj: unknown): StrikeInfo;
export function parseTaskList(obj: unknown): TaskList;
export function parseTaskExport(obj: unknown): TaskExport;
export function parseCliVersion(obj: unknown): CliVersion;
