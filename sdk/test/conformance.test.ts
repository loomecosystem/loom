// Cross-implementation determinism conformance. Replays the scenario from
// `engine-core/tests/conformance.rs`: the TS runtime must arrive at the same
// state hash as the Rust core. Mirrored in `conformance/expected.json`.

import { test } from "node:test";
import assert from "node:assert/strict";
import { World, field, runToCompletion, Budget, toHex64 } from "../src/index.ts";
import { Movement } from "./helpers.ts";

const SCENARIO_ID = "loom-conformance-v1";
const EXPECTED_STATE_HASH = 0x80b9a6c42a0e765fn;

function buildScenario(): World {
  const world = new World(42n);
  const pos = world.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  const vel = world.registerComponent("Velocity", [field("dx", "i64"), field("dy", "i64")]);
  const health = world.registerComponent("Health", [field("hp", "u32"), field("alive", "bool")]);

  for (let i = 1n; i <= 6n; i++) {
    const e = world.spawnEntity();
    world.set(pos, e, world.record(pos).setI64("x", i).setI64("y", 2n * i).toBytes());
    world.set(vel, e, world.record(vel).setI64("dx", i).setI64("dy", -i).toBytes());
    world.set(
      health,
      e,
      world
        .record(health)
        .setU32("hp", Number(100n - 10n * i))
        .setBool("alive", i % 2n === 1n)
        .toBytes(),
    );
  }

  const movement = new Movement(pos, vel);
  for (let s = 0; s < 3; s++) runToCompletion(world, movement, new Budget(1_000_000n, 1_000n), 0n);
  return world;
}

test(`${SCENARIO_ID} state hash matches the Rust core`, () => {
  const world = buildScenario();

  // After 3 sweeps, x = 4i and y = -i.
  const p3 = world.read(0, 3n)!;
  assert.equal(p3.getI64("x"), 12n);
  assert.equal(p3.getI64("y"), -3n);

  const h = world.stateHash();
  assert.equal(
    h,
    EXPECTED_STATE_HASH,
    `TS state hash ${toHex64(h)} != Rust ${toHex64(EXPECTED_STATE_HASH)}`,
  );
});
