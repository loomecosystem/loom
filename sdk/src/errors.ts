// Engine error codes.

export type EngineErrorCode =
  | "WorldFrozen"
  | "DuplicateComponent"
  | "UnknownComponent"
  | "UnknownEntity"
  | "BadRecordSize"
  | "UnknownField"
  | "FieldTypeMismatch"
  | "AccessDenied"
  | "ClaimNotFinalized"
  | "ClaimWindowOpen"
  | "ClaimAlreadySettled"
  | "ClaimInputMismatch"
  | "FraudProofInvalid"
  | "ModPermissionDenied"
  | "CrossWorldMismatch"
  | "InsufficientBalance"
  | "InsufficientGrants";

export class EngineError extends Error {
  readonly code: EngineErrorCode;
  readonly detail: Record<string, unknown>;

  constructor(code: EngineErrorCode, message?: string, detail: Record<string, unknown> = {}) {
    super(message ?? code);
    this.name = "EngineError";
    this.code = code;
    this.detail = detail;
  }
}
