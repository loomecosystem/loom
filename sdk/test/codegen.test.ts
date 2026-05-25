import { test } from "node:test";
import assert from "node:assert/strict";
import { LoomClient, field } from "../src/index.ts";

test("codegen emits typed interfaces and id constants from the on-chain schema", () => {
  const loom = LoomClient.local(1n);
  loom.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  loom.registerComponent("Health", [field("hp", "u32"), field("alive", "bool")]);
  loom.registerComponent("Inventory", [field("slots", { bytes: 32 })]);

  const code = loom.codegen();

  // Interfaces with TS types mapped from field types.
  assert.match(code, /export interface Position \{/);
  assert.match(code, /x: bigint;/);
  assert.match(code, /export interface Health \{/);
  assert.match(code, /hp: number;/);
  assert.match(code, /alive: boolean;/);
  assert.match(code, /slots: Uint8Array;/);

  // Id constants and the name union.
  assert.match(code, /Position: 0,/);
  assert.match(code, /Health: 1,/);
  assert.match(code, /export type ComponentName = "Position" \| "Health" \| "Inventory";/);
});
