// @loom/sdk - public exports.

export const LOOM_SDK_VERSION = "0.1.0";

export { Hasher, fnv1a, utf8, toHex64 } from "./hash.ts";
export { componentAddress, toHex } from "./addressing.ts";
export type { Address } from "./addressing.ts";
export { EngineError } from "./errors.ts";
export type { EngineErrorCode } from "./errors.ts";

export { ComponentSchema, SchemaRegistry, field, fieldSize, fieldTag } from "./schema.ts";
export type { Field, FieldType } from "./schema.ts";

export { Record } from "./record.ts";

export { World } from "./world.ts";
export { Access, SystemCtx } from "./system.ts";
export type { System } from "./system.ts";

export { Budget, crank, crankDirty, crankerReward, runToCompletion, startCursor } from "./tick.ts";
export type { Cursor, CrankReceipt } from "./tick.ts";

export { Indexer, decodeRecord } from "./indexer.ts";
export type { FieldValue, DecodedRecord, EntityView, ComponentRow } from "./indexer.ts";
export { LoomClient } from "./client.ts";
export { generateClient } from "./codegen.ts";
