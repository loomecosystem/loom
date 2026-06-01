// Smallholm - a small on-chain settlement economy with agent-driven NPCs,
// built on Loom. Exercises ECS Components, tick-driven Systems, bounded
// per-entity work, and NPCs whose behavior is an on-chain policy. Deterministic:
// the same seed and slot sequence produce the same state hash.

import {
  Access,
  LoomClient,
  type System,
  SystemCtx,
  fnv1a,
  field,
} from "@loom/sdk";

export const GRID = 12n;

/** NPC archetypes. */
export const FARMER = 0;
export const SOLDIER = 1;
export const SCOUT = 2;

/** Up to this many waypoints fit in a Route's packed `data` field. */
export const MAX_WAYPOINTS = 32;

/**
 * Terrain: a wall down column x=6 (rows 0..9) with a gap at the bottom. A scout
 * crossing the map must route around it; that pathfinding runs off-chain and
 * settles through the compute bridge.
 */
export function isBlocked(x: bigint, y: bigint): boolean {
  return x === 6n && y >= 0n && y <= 9n;
}

// Economy / combat tuning.
const FARM_YIELD = 3n; // grain produced per tick while on the home plot
const SELL_THRESHOLD = 9n; // grain above this is sold ...
const SELL_BATCH = 6n; // ... in batches of this many grain ...
const SELL_PRICE = 2n; // ... for this much gold each
const UPKEEP = 1n; // grain eaten per tick
const STARVE_DAMAGE = 2n; // hp lost per tick when out of grain
const RETREAT_HP = 6n; // a soldier below this retreats home instead of fighting

export interface Smallholm {
  loom: LoomClient;
  position: number;
  stats: number;
  inventory: number;
  agent: number;
  route: number;
  systems: System[];
}

/** Deterministic per-agent value from (seed, slot); no clock, no RNG state. */
export function prng(seed: bigint, slot: bigint): bigint {
  const buf = new Uint8Array(16);
  const dv = new DataView(buf.buffer);
  dv.setBigUint64(0, seed, true);
  dv.setBigUint64(8, slot, true);
  return fnv1a(buf);
}

/** One Manhattan step from (x,y) toward (tx,ty); `xFirst` breaks ties. */
export function stepToward(
  x: bigint,
  y: bigint,
  tx: bigint,
  ty: bigint,
  xFirst: boolean,
): [bigint, bigint] {
  const moveX = (): [bigint, bigint] => [x < tx ? x + 1n : x - 1n, y];
  const moveY = (): [bigint, bigint] => [x, y < ty ? y + 1n : y - 1n];
  if (xFirst) {
    if (x !== tx) return moveX();
    if (y !== ty) return moveY();
  } else {
    if (y !== ty) return moveY();
    if (x !== tx) return moveX();
  }
  return [x, y];
}

// --- Systems -----------------------------------------------------------------------

/** Each NPC decides and moves once per tick. */
class AgentSystem implements System {
  s: Smallholm;
  constructor(s: Smallholm) {
    this.s = s;
  }
  id() {
    return 10;
  }
  access() {
    return Access.new()
      .withReads([this.s.agent, this.s.stats, this.s.position])
      .withWrites([this.s.position, this.s.agent]);
  }
  query() {
    return this.s.agent;
  }
  run(ctx: SystemCtx, e: bigint) {
    const { agent, stats, position } = this.s;
    if (!ctx.readBool(agent, e, "alive")) return;
    if (ctx.readI64(stats, e, "hp") <= 0n) {
      ctx.mutate(agent, e, (r) => r.setBool("alive", false));
      return;
    }

    const kind = ctx.readU8(agent, e, "kind");
    if (kind === SCOUT) return; // scouts move via their settled Route instead
    const seed = ctx.readU64(agent, e, "seed");
    const homeX = ctx.readI64(agent, e, "homeX");
    const homeY = ctx.readI64(agent, e, "homeY");
    const x = ctx.readI64(position, e, "x");
    const y = ctx.readI64(position, e, "y");

    // Pick a destination. Farmers head to their plot. Soldiers chase their rival
    // when healthy and retreat home when hurt.
    let tx = homeX;
    let ty = homeY;
    if (kind === SOLDIER) {
      const rival = ctx.readEntity(agent, e, "rival");
      const rivalAlive = rival !== 0n && ctx.readI64(stats, rival, "hp") > 0n;
      const healthy = ctx.readI64(stats, e, "hp") >= RETREAT_HP;
      if (rivalAlive && healthy) {
        tx = ctx.readI64(position, rival, "x");
        ty = ctx.readI64(position, rival, "y");
      }
    }

    const xFirst = (prng(seed, ctx.slot) & 1n) === 0n;
    const [nx, ny] = stepToward(x, y, tx, ty, xFirst);
    ctx.mutate(position, e, (r) => r.setI64("x", nx).setI64("y", ny));
  }
}

/** A soldier adjacent to its rival strikes it. One rival per soldier. */
class CombatSystem implements System {
  s: Smallholm;
  constructor(s: Smallholm) {
    this.s = s;
  }
  id() {
    return 11;
  }
  access() {
    return Access.new()
      .withReads([this.s.agent, this.s.position, this.s.stats])
      .withWrites([this.s.stats]);
  }
  query() {
    return this.s.agent;
  }
  run(ctx: SystemCtx, e: bigint) {
    const { agent, position, stats } = this.s;
    if (ctx.readU8(agent, e, "kind") !== SOLDIER) return;
    if (!ctx.readBool(agent, e, "alive")) return;
    const rival = ctx.readEntity(agent, e, "rival");
    if (rival === 0n || ctx.readI64(stats, rival, "hp") <= 0n) return;

    const dx = ctx.readI64(position, e, "x") - ctx.readI64(position, rival, "x");
    const dy = ctx.readI64(position, e, "y") - ctx.readI64(position, rival, "y");
    const adjacent = (dx < 0n ? -dx : dx) + (dy < 0n ? -dy : dy) <= 1n;
    if (!adjacent) return;

    const atk = ctx.readI64(stats, e, "atk");
    ctx.mutate(stats, rival, (r) => r.setI64("hp", r.getI64("hp") - atk));
  }
}

/** A farmer on its plot grows grain and sells the surplus for gold. */
class FarmSystem implements System {
  s: Smallholm;
  constructor(s: Smallholm) {
    this.s = s;
  }
  id() {
    return 12;
  }
  access() {
    return Access.new()
      .withReads([this.s.agent, this.s.position])
      .withWrites([this.s.inventory]);
  }
  query() {
    return this.s.agent;
  }
  run(ctx: SystemCtx, e: bigint) {
    const { agent, position, inventory } = this.s;
    if (ctx.readU8(agent, e, "kind") !== FARMER) return;
    if (!ctx.readBool(agent, e, "alive")) return;
    const onPlot =
      ctx.readI64(position, e, "x") === ctx.readI64(agent, e, "homeX") &&
      ctx.readI64(position, e, "y") === ctx.readI64(agent, e, "homeY");
    if (!onPlot) return;

    ctx.mutate(inventory, e, (r) => {
      let grain = r.getU64("grain") + FARM_YIELD;
      let gold = r.getU64("gold");
      if (grain > SELL_THRESHOLD) {
        grain -= SELL_BATCH;
        gold += SELL_BATCH * SELL_PRICE;
      }
      r.setU64("grain", grain).setU64("gold", gold);
    });
  }
}

/** Every living NPC eats grain; those without it lose health. */
class UpkeepSystem implements System {
  s: Smallholm;
  constructor(s: Smallholm) {
    this.s = s;
  }
  id() {
    return 13;
  }
  access() {
    return Access.new()
      .withReads([this.s.agent, this.s.inventory])
      .withWrites([this.s.inventory, this.s.stats]);
  }
  query() {
    return this.s.agent;
  }
  run(ctx: SystemCtx, e: bigint) {
    const { agent, inventory, stats } = this.s;
    if (!ctx.readBool(agent, e, "alive")) return;
    const grain = ctx.readU64(inventory, e, "grain");
    if (grain >= UPKEEP) {
      ctx.mutate(inventory, e, (r) => r.setU64("grain", grain - UPKEEP));
    } else {
      ctx.mutate(stats, e, (r) => r.setI64("hp", r.getI64("hp") - STARVE_DAMAGE));
    }
  }
}

/**
 * Walks the scout one waypoint per tick along the Route the compute bridge
 * settled onto it. Entities with an empty Route (len 0) are no-ops, so this is
 * safe to run every tick.
 */
class FollowRouteSystem implements System {
  s: Smallholm;
  constructor(s: Smallholm) {
    this.s = s;
  }
  id() {
    return 14;
  }
  access() {
    return Access.new().withReads([this.s.route]).withWrites([this.s.position, this.s.route]);
  }
  query() {
    return this.s.route;
  }
  run(ctx: SystemCtx, e: bigint) {
    const { route, position } = this.s;
    const len = ctx.readU8(route, e, "len");
    const cursor = ctx.readU8(route, e, "cursor");
    if (cursor >= len) return; // arrived (or no route yet)
    const data = ctx.readBytes(route, e, "data");
    const x = data[2 * cursor];
    const y = data[2 * cursor + 1];
    ctx.mutate(position, e, (r) => r.setI64("x", BigInt(x)).setI64("y", BigInt(y)));
    ctx.mutate(route, e, (r) => r.setU8("cursor", cursor + 1));
  }
}

/** Register Smallholm's Components and Systems against a fresh world. */
export function createSmallholm(worldId: bigint = 1n): Smallholm {
  const loom = LoomClient.local(worldId);
  const position = loom.registerComponent("Position", [
    field("x", "i64"),
    field("y", "i64"),
  ]);
  const stats = loom.registerComponent("Stats", [field("hp", "i64"), field("atk", "i64")]);
  const inventory = loom.registerComponent("Inventory", [
    field("gold", "u64"),
    field("grain", "u64"),
  ]);
  const agent = loom.registerComponent("Agent", [
    field("kind", "u8"),
    field("alive", "bool"),
    field("seed", "u64"),
    field("homeX", "i64"),
    field("homeY", "i64"),
    field("rival", "entity"),
  ]);
  const route = loom.registerComponent("Route", [
    field("len", "u8"),
    field("cursor", "u8"),
    field("inputHash", "u64"),
    field("data", { bytes: MAX_WAYPOINTS * 2 }),
  ]);

  const s: Smallholm = { loom, position, stats, inventory, agent, route, systems: [] };
  s.systems = [
    new AgentSystem(s),
    new CombatSystem(s),
    new FarmSystem(s),
    new UpkeepSystem(s),
    new FollowRouteSystem(s),
  ];
  return s;
}

/** Advance the world one tick: run every System to completion, in order. */
export function step(s: Smallholm, slot: bigint): void {
  for (const sys of s.systems) s.loom.tick(sys, slot);
}
