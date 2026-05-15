// Tick coordinator - mirror of `engine-core/src/tick.rs`. Bounded-work cranks
// with resumable cursors.

import type { World } from "./world.ts";
import { type System, SystemCtx } from "./system.ts";

/** A compute-budget envelope for one crank. */
export class Budget {
  readonly maxCu: bigint;
  readonly cuPerEntity: bigint;

  constructor(maxCu: bigint, cuPerEntity: bigint) {
    if (cuPerEntity <= 0n) throw new Error("cuPerEntity must be positive");
    this.maxCu = maxCu;
    this.cuPerEntity = cuPerEntity;
  }

  /** How many entities fit in this budget (at least one). */
  maxEntities(): number {
    const n = this.maxCu / this.cuPerEntity;
    return Number(n > 0n ? n : 1n);
  }
}

/** Where a System sweep has gotten to. */
export interface Cursor {
  resumeFrom: bigint;
  processed: bigint;
  cranks: number;
  done: boolean;
}

export function startCursor(): Cursor {
  return { resumeFrom: 0n, processed: 0n, cranks: 0, done: false };
}

export interface CrankReceipt {
  processed: number;
  cuUsed: bigint;
  done: boolean;
}

/** Run one bounded crank of `system`, advancing `cursor`. */
export function crank(
  world: World,
  system: System,
  cursor: Cursor,
  budget: Budget,
  slot: bigint,
): CrankReceipt {
  if (cursor.done) return { processed: 0, cuUsed: 0n, done: true };

  const maxN = budget.maxEntities();
  const query = system.query();
  const access = system.access();

  const ids = world.entitiesWithFrom(query, cursor.resumeFrom).slice(0, maxN + 1);
  const hasMore = ids.length > maxN;
  const batch = ids.slice(0, maxN);

  for (const e of batch) {
    const ctx = new SystemCtx(world, access, slot);
    system.run(ctx, e);
  }

  const processed = batch.length;
  cursor.processed += BigInt(processed);
  cursor.cranks += 1;
  if (processed > 0) cursor.resumeFrom = batch[batch.length - 1] + 1n;
  if (!hasMore) cursor.done = true;

  return { processed, cuUsed: BigInt(processed) * budget.cuPerEntity, done: cursor.done };
}

/** Crank repeatedly until the sweep completes. */
export function runToCompletion(
  world: World,
  system: System,
  budget: Budget,
  slot: bigint,
): Cursor {
  const cursor = startCursor();
  while (!cursor.done) crank(world, system, cursor, budget, slot);
  return cursor;
}

/** Crank only over dirty entities carrying the System's query Component. */
export function crankDirty(world: World, system: System, slot: bigint): number {
  const query = system.query();
  const access = system.access();
  const targets = world
    .dirtyPairs()
    .filter((p) => p.component === query)
    .map((p) => p.entity);
  for (const e of targets) {
    const ctx = new SystemCtx(world, access, slot);
    system.run(ctx, e);
  }
  return targets.length;
}

/** Reward paid to a cranker for advancing `processed` entities. */
export function crankerReward(processed: bigint, ratePerEntity: bigint): bigint {
  return processed * ratePerEntity;
}
