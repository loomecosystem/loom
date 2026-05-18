import { test } from "node:test";
import assert from "node:assert/strict";
import { Access, EngineError, LoomClient, type System, SystemCtx, field } from "../src/index.ts";
import { Movement } from "./helpers.ts";

test("create entity → system mutates → client reads back", () => {
  const loom = LoomClient.local(1n);
  const pos = loom.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  const vel = loom.registerComponent("Velocity", [field("dx", "i64"), field("dy", "i64")]);

  const e = loom.spawn();
  loom.set(pos, e, (r) => r.setI64("x", 10n).setI64("y", 20n));
  loom.set(vel, e, (r) => r.setI64("dx", 3n).setI64("dy", -4n));

  loom.tick(new Movement(pos, vel));

  // Read back through the decoded client view.
  const p = loom.get(pos, e)!;
  assert.equal(p.x, 13n);
  assert.equal(p.y, 16n);

  // And through the indexer, reconstructed by entity.
  const view = loom.indexer().entityView(e);
  assert.equal(view.Position.x, 13n);
  assert.equal(view.Velocity.dx, 3n);
});

test("the engine enforces declared access (a rogue write is refused)", () => {
  const loom = LoomClient.local(1n);
  const pos = loom.registerComponent("Position", [field("x", "i64")]);
  const secret = loom.registerComponent("Secret", [field("v", "u64")]);
  const e = loom.spawn();
  loom.set(pos, e, () => {});

  class Rogue implements System {
    id() {
      return 2;
    }
    access() {
      return Access.new().withWrites([pos]); // declares pos, not secret
    }
    query() {
      return pos;
    }
    run(ctx: SystemCtx, ent: bigint) {
      ctx.mutate(secret, ent, (r) => r.setU64("v", 1n)); // undeclared write
    }
  }

  assert.throws(
    () => loom.tick(new Rogue()),
    (err: unknown) => err instanceof EngineError && err.code === "AccessDenied",
  );
});

test("a frozen world rejects schema changes", () => {
  const loom = LoomClient.local(1n);
  loom.freeze();
  assert.throws(
    () => loom.registerComponent("X", []),
    (err: unknown) => err instanceof EngineError && err.code === "WorldFrozen",
  );
});
