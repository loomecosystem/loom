// Populating Smallholm, plus example third-party mods. The mods are authored as
// if by someone other than the world's creator; they attach to the live world
// under its policy.

import { Access, type System, SystemCtx } from "@loom/sdk";
import { FARMER, SOLDIER, type Smallholm } from "./world.ts";

export interface Settlement {
  farmers: bigint[];
  soldiers: bigint[];
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

  // Two soldiers, each the other's rival, starting from opposite corners.
  const a = loom.spawn();
  const b = loom.spawn();
  loom.set(position, a, (r) => r.setI64("x", 1n).setI64("y", 1n));
  loom.set(position, b, (r) => r.setI64("x", 10n).setI64("y", 10n));
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
      .setI64("homeX", 10n)
      .setI64("homeY", 10n)
      .setEntity("rival", a),
  );
  soldiers.push(a, b);

  return { farmers, soldiers };
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
