// Replays the scenario from `engine-core/tests/economy.rs`; the
// fee/treasury/grants numbers must match the Rust core to the unit.

import { test } from "node:test";
import assert from "node:assert/strict";
import {
  Access,
  Budget,
  Economy,
  EngineError,
  type System,
  SystemCtx,
  World,
  crank,
  field,
  standardFees,
  startCursor,
} from "../src/index.ts";

const WORLD = 1n;
const N = 250n;

/** Movement reading only `dx`, matching the Rust economy scenario's schema. */
class MovementX implements System {
  pos: number;
  vel: number;
  constructor(pos: number, vel: number) {
    this.pos = pos;
    this.vel = vel;
  }
  id() {
    return 1;
  }
  access() {
    return Access.new().withReads([this.vel]).withWrites([this.pos]);
  }
  query() {
    return this.pos;
  }
  run(ctx: SystemCtx, e: bigint) {
    const dx = ctx.readI64(this.vel, e, "dx");
    ctx.mutate(this.pos, e, (r) => r.setI64("x", r.getI64("x") + dx));
  }
}

test("world pays metered fees; crankers, protocol and grants accrue exactly", () => {
  const world = new World(WORLD);
  const pos = world.registerComponent("Position", [field("x", "i64")]);
  const vel = world.registerComponent("Velocity", [field("dx", "i64")]);
  for (let i = 0n; i < N; i++) {
    const e = world.spawnEntity();
    world.set(pos, e, world.record(pos).toBytes());
    world.set(vel, e, world.record(vel).setI64("dx", 1n).toBytes());
  }

  const econ = new Economy(standardFees());
  econ.fundWorld(WORLD, 1_000_000n);

  const movement = new MovementX(pos, vel);
  const budget = new Budget(1_000n, 10n); // 100 entities/crank
  const cursor = startCursor();
  let crankerTotal = 0n;
  let cranks = 0;
  while (!cursor.done) {
    const receipt = crank(world, movement, cursor, budget, 0n);
    const bill = econ.chargeCrank(WORLD, BigInt(receipt.processed));
    crankerTotal += bill.crankerReward;
    cranks++;
  }

  assert.equal(cranks, 3);
  assert.equal(econ.ledger(WORLD).spent, 17_500n);
  assert.equal(crankerTotal, 7_000n);
  assert.equal(econ.treasury.protocol, 7_875n);
  assert.equal(econ.treasury.grants, 2_625n);

  assert.equal(world.storageBytes(), 4_000n);
  assert.equal(econ.chargeStorage(WORLD, world.storageBytes()), 4_000n);
  assert.equal(econ.treasury.protocol, 10_875n);
  assert.equal(econ.treasury.grants, 3_625n);

  econ.disburseGrant(3_000n);
  assert.equal(econ.treasury.grants, 625n);
  assert.equal(econ.balance(WORLD), 978_500n);
});

test("an underfunded world cannot crank", () => {
  const econ = new Economy(standardFees());
  econ.fundWorld(WORLD, 100n);
  assert.throws(
    () => econ.chargeCrank(WORLD, 100n),
    (e: unknown) => e instanceof EngineError && e.code === "InsufficientBalance",
  );
  assert.equal(econ.balance(WORLD), 100n); // failed charge moves nothing
});
