import { test } from "node:test";
import assert from "node:assert/strict";
import { Budget, LoomClient, field, startCursor } from "../src/index.ts";
import { Movement } from "./helpers.ts";

const N = 10_000n;

test("10k entities advance across 3 cranks, each within budget", () => {
  const loom = LoomClient.local(1n);
  const pos = loom.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  const vel = loom.registerComponent("Velocity", [field("dx", "i64"), field("dy", "i64")]);

  for (let i = 0n; i < N; i++) {
    const e = loom.spawn();
    loom.set(pos, e, () => {});
    loom.set(vel, e, (r) => r.setI64("dx", 1n));
  }

  // 1.0M CU at 250 CU/entity => 4000 entities/crank => 3 cranks for 10k.
  const budget = new Budget(1_000_000n, 250n);
  assert.equal(budget.maxEntities(), 4000);

  const movement = new Movement(pos, vel);
  const cursor = startCursor();
  let cranks = 0;
  let maxInOneCrank = 0;
  while (!cursor.done) {
    const r = loom.crank(movement, cursor, 0n, budget);
    assert.ok(r.cuUsed <= budget.maxCu, "no crank exceeds the CU ceiling");
    maxInOneCrank = Math.max(maxInOneCrank, r.processed);
    cranks++;
  }

  assert.equal(cranks, 3);
  assert.ok(maxInOneCrank <= 4000);
  assert.equal(cursor.processed, N);

  for (let e = 1n; e <= N; e++) {
    assert.equal(loom.get(pos, e)!.x, 1n);
  }
});

test("dirty-set sweep touches only changed entities", () => {
  const loom = LoomClient.local(1n);
  const pos = loom.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  const vel = loom.registerComponent("Velocity", [field("dx", "i64"), field("dy", "i64")]);
  for (let i = 0n; i < 100n; i++) {
    const e = loom.spawn();
    loom.set(pos, e, () => {});
    loom.set(vel, e, (r) => r.setI64("dx", 1n));
  }

  loom.clearDirty();
  for (const e of [5n, 17n, 42n]) loom.set(pos, e, (r) => r.setI64("x", 0n));

  const processed = loom.tickDirty(new Movement(pos, vel));
  assert.equal(processed, 3);

  for (let e = 1n; e <= 100n; e++) {
    const x = loom.get(pos, e)!.x;
    assert.equal(x, [5n, 17n, 42n].includes(e) ? 1n : 0n);
  }
});
