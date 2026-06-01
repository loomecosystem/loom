// The world runs end-to-end with agent-driven NPCs, and a third-party mod
// attaches to it at runtime without redeploying.

import { test } from "node:test";
import assert from "node:assert/strict";
import { ComputeBridge, EngineError, ModPolicy, admitMod } from "@loom/sdk";
import { createSmallholm, isBlocked, step, type Smallholm } from "../src/world.ts";
import {
  RaidMod,
  TitheMod,
  dispatchScout,
  settleScoutRoute,
  spawnSettlement,
  type Settlement,
} from "../src/agents.ts";

function play(ticks: number): { s: Smallholm; npc: Settlement } {
  const s = createSmallholm(1n);
  const npc = spawnSettlement(s);
  for (let t = 1; t <= ticks; t++) step(s, BigInt(t));
  return { s, npc };
}

function totalGold(s: Smallholm, who: bigint[]): bigint {
  return who.reduce((sum, e) => sum + (s.loom.get(s.inventory, e)!.gold as bigint), 0n);
}

test("the world is deterministic: identical seeds and slots → identical state", () => {
  const a = play(20).s.loom.stateHash();
  const b = play(20).s.loom.stateHash();
  assert.equal(a, b);
});

test("agent-driven NPCs move autonomously toward their goals", () => {
  const s = createSmallholm(1n);
  const npc = spawnSettlement(s);

  // A soldier moves toward its rival on its own; captured mid-march (it may
  // later fight and retreat home).
  const start = s.loom.get(s.position, npc.soldiers[0])!;
  for (let t = 1; t <= 6; t++) step(s, BigInt(t));
  const mid = s.loom.get(s.position, npc.soldiers[0])!;
  assert.notEqual(`${mid.x},${mid.y}`, `${start.x},${start.y}`, "soldier moved on its own");

  // Each farmer reached its plot.
  for (let t = 7; t <= 20; t++) step(s, BigInt(t));
  for (const f of npc.farmers) {
    const pos = s.loom.get(s.position, f)!;
    const agent = s.loom.get(s.agent, f)!;
    assert.equal(pos.x, agent.homeX, "farmer reached plot x");
    assert.equal(pos.y, agent.homeY, "farmer reached plot y");
  }
});

test("the on-chain economy accrues: farmers turn grain into gold", () => {
  const { s, npc } = play(20);
  const gold = totalGold(s, npc.farmers);
  assert.ok(gold > 0n, `farmers should have earned gold, got ${gold}`);
});

test("combat plays out: rival soldiers wear each other down", () => {
  const { s, npc } = play(20);
  const hp = npc.soldiers.reduce((sum, e) => sum + (s.loom.get(s.stats, e)!.hp as bigint), 0n);
  assert.ok(hp < 36n, `soldiers started at 36 total hp, ended at ${hp}`);
});

test("a third-party mod attaches to the live world under policy", () => {
  const { s, npc } = play(10);

  // Policy grants mods write access to Inventory only.
  const policy = ModPolicy.new().allowWrite(s.inventory);

  // The tithe mod is admitted and run against the live world.
  const tithe = new TitheMod(s.inventory, s.agent);
  assert.equal(admitMod(policy, tithe), tithe.id());

  const before = totalGold(s, npc.farmers);
  assert.ok(before > 0n);
  s.loom.tick(tithe, 11n);
  const after = totalGold(s, npc.farmers);
  assert.ok(after < before, "the tithe collected gold");
});

test("a scout's off-chain path is settled via the bridge and walked", () => {
  const s = createSmallholm(1n);
  const { scout } = spawnSettlement(s);

  // The planner computes a route around the wall and posts it with a bond.
  const bridge = new ComputeBridge(5n);
  const start: [number, number] = [1, 1];
  const goal: [number, number] = [11, 11];
  const dispatch = dispatchScout(bridge, start, goal, new Uint8Array(32).fill(3), 1_000n, 0n);

  // It cannot be consumed before the fraud window closes.
  assert.throws(
    () => settleScoutRoute(s, bridge, dispatch, scout),
    (e: unknown) => e instanceof EngineError && e.code === "ClaimNotFinalized",
  );

  // Window elapses, the result finalizes, and it settles onto the scout.
  bridge.finalize(dispatch.claim, 10n);
  settleScoutRoute(s, bridge, dispatch, scout);

  // A System walks the scout along the settled route; it never enters the wall,
  // confirming the off-chain pathfinding was consumed correctly.
  for (let t = 1; t <= dispatch.pathLength + 2; t++) {
    step(s, BigInt(t));
    const p = s.loom.get(s.position, scout)!;
    assert.ok(!isBlocked(p.x as bigint, p.y as bigint), `scout entered the wall at ${p.x},${p.y}`);
  }

  const end = s.loom.get(s.position, scout)!;
  assert.equal(end.x, 11n);
  assert.equal(end.y, 11n);
});

test("a mod outside policy is refused admission", () => {
  const s = createSmallholm(1n);
  spawnSettlement(s);
  const policy = ModPolicy.new().allowWrite(s.inventory);
  const raid = new RaidMod(s.position, s.agent); // wants to write Position
  assert.throws(
    () => admitMod(policy, raid),
    (e: unknown) => e instanceof EngineError && e.code === "ModPermissionDenied",
  );
});
