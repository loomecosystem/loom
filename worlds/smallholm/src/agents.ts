// Populating Smallholm, plus example third-party mods. The mods are authored as
// if by someone other than the world's creator; they attach to the live world
// under its policy.

import { Access, ComputeBridge, type System, SystemCtx, fnv1a } from "@loom/sdk";
import {
  FARMER,
  GRID,
  MAX_WAYPOINTS,
  SCOUT,
  SOLDIER,
  type Smallholm,
  isBlocked,
} from "./world.ts";

export interface Settlement {
  farmers: bigint[];
  soldiers: bigint[];
  scout: bigint;
}

/** Spawn three farmers and two rival soldiers into a fresh Smallholm. */
export function spawnSettlement(s: Smallholm): Settlement {
  const { loom, position, stats, inventory, agent } = s;
  const farmers: bigint[] = [];
  const soldiers: bigint[] = [];

  // Farmers start at (0,0) and walk to their plots, carrying enough grain to
  // survive the journey.
  const plots: [bigint, bigint][] = [
    [2n, 2n],
    [8n, 3n],
    [3n, 8n],
  ];
  plots.forEach(([px, py], i) => {
    const e = loom.spawn();
    loom.set(position, e, (r) => r.setI64("x", 0n).setI64("y", 0n));
    loom.set(stats, e, (r) => r.setI64("hp", 20n).setI64("atk", 0n));
    loom.set(inventory, e, (r) => r.setU64("gold", 0n).setU64("grain", 12n));
    loom.set(agent, e, (r) =>
      r
        .setU8("kind", FARMER)
        .setBool("alive", true)
        .setU64("seed", BigInt(1001 + i))
        .setI64("homeX", px)
        .setI64("homeY", py)
        .setEntity("rival", 0n),
    );
    farmers.push(e);
  });

  // Two soldiers, each the other's rival.
  const a = loom.spawn();
  const b = loom.spawn();
  // Both start west of the wall, so their fight never crosses the terrain the
  // scout has to route around.
  loom.set(position, a, (r) => r.setI64("x", 1n).setI64("y", 1n));
  loom.set(position, b, (r) => r.setI64("x", 4n).setI64("y", 10n));
  loom.set(stats, a, (r) => r.setI64("hp", 18n).setI64("atk", 3n));
  loom.set(stats, b, (r) => r.setI64("hp", 18n).setI64("atk", 3n));
  loom.set(inventory, a, (r) => r.setU64("gold", 0n).setU64("grain", 40n));
  loom.set(inventory, b, (r) => r.setU64("gold", 0n).setU64("grain", 40n));
  loom.set(agent, a, (r) =>
    r
      .setU8("kind", SOLDIER)
      .setBool("alive", true)
      .setU64("seed", 7n)
      .setI64("homeX", 1n)
      .setI64("homeY", 1n)
      .setEntity("rival", b),
  );
  loom.set(agent, b, (r) =>
    r
      .setU8("kind", SOLDIER)
      .setBool("alive", true)
      .setU64("seed", 13n)
      .setI64("homeX", 4n)
      .setI64("homeY", 10n)
      .setEntity("rival", a),
  );
  soldiers.push(a, b);

  // A scout. It stays idle until an off-chain route is settled onto it through
  // the compute bridge (see the planner below).
  const scout = loom.spawn();
  loom.set(position, scout, (r) => r.setI64("x", 1n).setI64("y", 1n));
  loom.set(stats, scout, (r) => r.setI64("hp", 16n).setI64("atk", 0n));
  loom.set(inventory, scout, (r) => r.setU64("gold", 0n).setU64("grain", 60n));
  loom.set(agent, scout, (r) =>
    r
      .setU8("kind", SCOUT)
      .setBool("alive", true)
      .setU64("seed", 99n)
      .setI64("homeX", 1n)
      .setI64("homeY", 1n)
      .setEntity("rival", 0n),
  );

  return { farmers, soldiers, scout };
}

// --- off-chain planner + compute bridge --------------------------------------------
//
// Pathfinding around the wall is too heavy for a transaction. An off-chain
// planner computes the route, posts it to the compute bridge with a bond, and
// after the fraud-proof window it settles on-chain into the scout's Route
// Component for a System to walk.

export const SCOUT_TASK = 42n;

/** Breadth-first shortest path around blocked cells. */
export function bfsPath(start: [number, number], goal: [number, number]): [number, number][] {
  const G = Number(GRID);
  const idx = (x: number, y: number) => y * G + x;
  const prev = new Array<number>(G * G).fill(-1);
  const seen = new Array<boolean>(G * G).fill(false);
  const queue: [number, number][] = [[...start]];
  seen[idx(start[0], start[1])] = true;

  while (queue.length) {
    const [x, y] = queue.shift()!;
    if (x === goal[0] && y === goal[1]) break;
    for (const [dx, dy] of [
      [1, 0],
      [-1, 0],
      [0, 1],
      [0, -1],
    ]) {
      const nx = x + dx;
      const ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= G || ny >= G) continue;
      if (isBlocked(BigInt(nx), BigInt(ny)) || seen[idx(nx, ny)]) continue;
      seen[idx(nx, ny)] = true;
      prev[idx(nx, ny)] = idx(x, y);
      queue.push([nx, ny]);
    }
  }

  const path: [number, number][] = [];
  let cur = idx(goal[0], goal[1]);
  if (!seen[cur]) throw new Error("goal unreachable");
  while (cur !== -1) {
    path.push([cur % G, Math.floor(cur / G)]);
    cur = prev[cur];
  }
  path.reverse();
  return path;
}

function pack(path: [number, number][]): Uint8Array {
  const out = new Uint8Array(MAX_WAYPOINTS * 2);
  path.forEach(([x, y], i) => {
    out[2 * i] = x;
    out[2 * i + 1] = y;
  });
  return out;
}

export function scoutRequestHash(start: [number, number], goal: [number, number]): bigint {
  return fnv1a(Uint8Array.of(start[0], start[1], goal[0], goal[1]));
}

export interface ScoutDispatch {
  claim: bigint;
  inputHash: bigint;
  pathLength: number;
}

/** Off-chain: compute the scout's path and post it optimistically to the bridge. */
export function dispatchScout(
  bridge: ComputeBridge,
  start: [number, number],
  goal: [number, number],
  worker: Uint8Array,
  bond: bigint,
  slot: bigint,
): ScoutDispatch {
  const path = bfsPath(start, goal);
  const inputHash = scoutRequestHash(start, goal);
  const claim = bridge.postResult(SCOUT_TASK, inputHash, pack(path), worker, bond, slot);
  return { claim, inputHash, pathLength: path.length };
}

/** On-chain settle: fold a finalized route into the scout's Route Component. */
export function settleScoutRoute(
  s: Smallholm,
  bridge: ComputeBridge,
  dispatch: ScoutDispatch,
  scout: bigint,
): void {
  const settled = bridge.consume(dispatch.claim, dispatch.inputHash);
  s.loom.set(s.route, scout, (r) =>
    r
      .setU8("len", dispatch.pathLength)
      .setU8("cursor", 0)
      .setU64("inputHash", dispatch.inputHash)
      .setBytes("data", settled),
  );
}

/**
 * A third-party mod: a tithe on accumulated gold. Needs only the Inventory
 * Component id, and stays within a policy that grants writes to Inventory.
 */
export class TitheMod implements System {
  inventory: number;
  agent: number;
  rate: bigint;
  constructor(inventory: number, agent: number, rate: bigint = 4n) {
    this.inventory = inventory;
    this.agent = agent;
    this.rate = rate;
  }
  id() {
    return 200;
  }
  access() {
    return Access.new().withWrites([this.inventory]);
  }
  query() {
    return this.agent; // every entity with an Agent record
  }
  run(ctx: SystemCtx, e: bigint) {
    ctx.mutate(this.inventory, e, (r) => {
      const gold = r.getU64("gold");
      r.setU64("gold", gold - (gold < this.rate ? gold : this.rate));
    });
  }
}

/**
 * A mod that tries to teleport units. A world whose policy grants only Inventory
 * writes refuses to admit it.
 */
export class RaidMod implements System {
  position: number;
  agent: number;
  constructor(position: number, agent: number) {
    this.position = position;
    this.agent = agent;
  }
  id() {
    return 201;
  }
  access() {
    return Access.new().withWrites([this.position]);
  }
  query() {
    return this.agent;
  }
  run(ctx: SystemCtx, e: bigint) {
    ctx.mutate(this.position, e, (r) => r.setI64("x", 0n).setI64("y", 0n));
  }
}
