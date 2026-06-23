import { test } from "node:test";
import assert from "node:assert/strict";
import {
  Access,
  ComputeBridge,
  EngineError,
  LoomClient,
  type System,
  SystemCtx,
  field,
  fnv1a,
  utf8,
} from "../src/index.ts";

const PATHFIND = 1n;
const REQUEST_HASH = fnv1a(utf8("req:(0,0)->(2,0)"));

/** A precomputed off-chain route. */
const PATH: [number, number][] = [
  [0, 0],
  [0, 1],
  [1, 1],
  [2, 1],
  [2, 0],
];

function packPath(path: [number, number][]): Uint8Array {
  const out = new Uint8Array(16); // up to 8 waypoints
  path.forEach(([x, y], i) => {
    out[2 * i] = x;
    out[2 * i + 1] = y;
  });
  return out;
}

/** Walks an entity one waypoint per tick along its settled Route. */
class FollowPath implements System {
  pos: number;
  route: number;
  constructor(pos: number, route: number) {
    this.pos = pos;
    this.route = route;
  }
  id() {
    return 7;
  }
  access() {
    return Access.new().withReads([this.route]).withWrites([this.pos, this.route]);
  }
  query() {
    return this.route;
  }
  run(ctx: SystemCtx, e: bigint) {
    const len = ctx.readU8(this.route, e, "len");
    const cursor = ctx.readU8(this.route, e, "cursor");
    if (cursor >= len) return;
    const data = ctx.readBytes(this.route, e, "data");
    const x = data[2 * cursor];
    const y = data[2 * cursor + 1];
    ctx.mutate(this.pos, e, (r) => r.setI64("x", BigInt(x)).setI64("y", BigInt(y)));
    ctx.mutate(this.route, e, (r) => r.setU8("cursor", cursor + 1));
  }
}

test("off-chain route settled on-chain and consumed by a System", () => {
  const packed = packPath(PATH);
  const bridge = new ComputeBridge(20n);
  const worker = new Uint8Array(32).fill(1);
  const claim = bridge.postResult(PATHFIND, REQUEST_HASH, packed, worker, 1_000n, 0n);

  // Not consumable before the window closes.
  assert.throws(
    () => bridge.consume(claim, REQUEST_HASH),
    (e: unknown) => e instanceof EngineError && e.code === "ClaimNotFinalized",
  );
  assert.throws(
    () => bridge.finalize(claim, 5n),
    (e: unknown) => e instanceof EngineError && e.code === "ClaimWindowOpen",
  );

  bridge.finalize(claim, 25n);
  assert.equal(bridge.get(claim).status.kind, "finalized");

  // Settle on-chain: fold the route into world state.
  const loom = LoomClient.local(1n);
  const pos = loom.registerComponent("Position", [field("x", "i64"), field("y", "i64")]);
  const route = loom.registerComponent("Route", [
    field("len", "u8"),
    field("cursor", "u8"),
    field("data", { bytes: 16 }),
  ]);
  const unit = loom.spawn();
  loom.set(pos, unit, (r) => r.setI64("x", 0n).setI64("y", 0n));

  const settled = bridge.consume(claim, REQUEST_HASH);
  loom.set(route, unit, (r) =>
    r.setU8("len", PATH.length).setU8("cursor", 0).setBytes("data", settled),
  );

  const follow = new FollowPath(pos, route);
  for (let i = 0; i < PATH.length; i++) loom.tick(follow);

  const p = loom.get(pos, unit)!;
  assert.equal(p.x, 2n);
  assert.equal(p.y, 0n);
});

test("a fraudulent result is challenged and slashed", () => {
  const correct = packPath(PATH);
  const bogus = new Uint8Array(16); // all zeros
  const bridge = new ComputeBridge(20n);
  const liar = new Uint8Array(32).fill(9);
  const claim = bridge.postResult(PATHFIND, REQUEST_HASH, bogus, liar, 5_000n, 0n);

  bridge.challenge(claim, correct, 10n);
  assert.equal(bridge.get(claim).status.kind, "disputed");
  assert.equal(bridge.totalSlashed, 5_000n);

  assert.throws(
    () => bridge.finalize(claim, 25n),
    (e: unknown) => e instanceof EngineError && e.code === "ClaimAlreadySettled",
  );
});

test("an honest result cannot be slashed", () => {
  const correct = packPath(PATH);
  const bridge = new ComputeBridge(20n);
  const claim = bridge.postResult(PATHFIND, REQUEST_HASH, correct, new Uint8Array(32), 5_000n, 0n);

  assert.throws(
    () => bridge.challenge(claim, correct, 10n),
    (e: unknown) => e instanceof EngineError && e.code === "FraudProofInvalid",
  );
  assert.equal(bridge.totalSlashed, 0n);
  bridge.finalize(claim, 25n);
  assert.equal(bridge.get(claim).status.kind, "finalized");
});

test("a finalized result cannot be consumed against a different request", () => {
  const bridge = new ComputeBridge(20n);
  const claim = bridge.postResult(PATHFIND, REQUEST_HASH, packPath(PATH), new Uint8Array(32), 1_000n, 0n);
  bridge.finalize(claim, 25n);

  // Its own request: fine.
  assert.deepEqual(bridge.consume(claim, REQUEST_HASH), packPath(PATH));
  // A different request: a distinct mismatch error, not FraudProofInvalid.
  assert.throws(
    () => bridge.consume(claim, REQUEST_HASH ^ 1n),
    (e: unknown) => e instanceof EngineError && e.code === "ClaimInputMismatch",
  );
});

test("a verified result finalizes immediately, with no window", () => {
  const packed = packPath(PATH);
  const resultHash = fnv1a(packed);
  // Stand-in for an on-chain SNARK verifier: a one-byte proof token that must bind
  // the posted result's hash.
  const verify = (_inp: bigint, res: bigint, proof: Uint8Array) =>
    res === resultHash && proof.length > 0 && proof[0] === 1;

  const bridge = new ComputeBridge(20n);
  const worker = new Uint8Array(32);

  // A proof the verifier rejects is refused outright.
  assert.throws(
    () => bridge.postVerified(PATHFIND, REQUEST_HASH, packed, worker, Uint8Array.of(0), verify),
    (e: unknown) => e instanceof EngineError && e.code === "FraudProofInvalid",
  );

  // A valid proof finalizes on the spot - consumable without a finalize() call.
  const claim = bridge.postVerified(PATHFIND, REQUEST_HASH, packed, worker, Uint8Array.of(1), verify);
  assert.equal(bridge.get(claim).status.kind, "finalized");
  assert.deepEqual(bridge.consume(claim, REQUEST_HASH), packed);
});
