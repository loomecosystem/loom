// LoomClient - high-level client surface for a world author.
// Drives the in-process `World`, which mirrors the on-chain program. The same
// surface can sit over an RPC connection to a deployed program unchanged.

import { World } from "./world.ts";
import type { Field } from "./schema.ts";
import type { Record as LoomRecord } from "./record.ts";
import type { System } from "./system.ts";
import { Budget, type Cursor, type CrankReceipt, crank, crankDirty, runToCompletion } from "./tick.ts";
import { Indexer, type DecodedRecord, decodeRecord } from "./indexer.ts";
import { generateClient } from "./codegen.ts";
import type { Address } from "./addressing.ts";

export class LoomClient {
  readonly world: World;
  private idx: Indexer;

  /** Per-crank compute ceiling. Solana's hard cap is 1.4M CU. */
  static readonly DEFAULT_BUDGET = new Budget(1_400_000n, 1_000n);

  constructor(world: World) {
    this.world = world;
    this.idx = new Indexer(world);
  }

  /** A fresh in-memory world. */
  static local(worldId: bigint = 1n, authority?: Uint8Array): LoomClient {
    return new LoomClient(new World(worldId, authority));
  }

  // --- schema & governance ---

  registerComponent(name: string, fields: Field[]): number {
    return this.world.registerComponent(name, fields);
  }
  freeze(): void {
    this.world.freeze();
  }
  isFrozen(): boolean {
    return this.world.isFrozen();
  }

  // --- entities & data ---

  spawn(): bigint {
    return this.world.spawnEntity();
  }

  /** Write a Component via a builder over a zeroed record. */
  set(component: number, entity: bigint, build: (r: LoomRecord) => void): void {
    const r = this.world.record(component);
    build(r);
    this.world.set(component, entity, r.toBytes());
  }

  /** Typed record view, or undefined if the entity lacks the Component. */
  read(component: number, entity: bigint): LoomRecord | undefined {
    return this.world.read(component, entity);
  }

  /** Decoded plain-object view of one Component on one entity. */
  get(component: number, entity: bigint): DecodedRecord | undefined {
    const bytes = this.world.get(component, entity);
    if (!bytes) return undefined;
    return decodeRecord(this.world.schema(component), bytes);
  }

  query(component: number): bigint[] {
    return this.world.entitiesWith(component);
  }

  address(component: number, entity: bigint): Address {
    return this.world.address(component, entity);
  }

  // --- ticking ---

  /** Run a System to completion across as many bounded cranks as needed. */
  tick(system: System, slot: bigint = 0n, budget: Budget = LoomClient.DEFAULT_BUDGET): Cursor {
    return runToCompletion(this.world, system, budget, slot);
  }

  /** Run one bounded crank, advancing the supplied cursor. */
  crank(
    system: System,
    cursor: Cursor,
    slot: bigint = 0n,
    budget: Budget = LoomClient.DEFAULT_BUDGET,
  ): CrankReceipt {
    return crank(this.world, system, cursor, budget, slot);
  }

  /** Sweep only entities changed since the last `clearDirty`. */
  tickDirty(system: System, slot: bigint = 0n): number {
    return crankDirty(this.world, system, slot);
  }
  clearDirty(): void {
    this.world.clearDirty();
  }

  // --- reflection ---

  stateHash(): bigint {
    return this.world.stateHash();
  }
  indexer(): Indexer {
    return this.idx;
  }
  /** Generate a typed TS client from the current schema. */
  codegen(): string {
    return generateClient(this.world.registry());
  }
}
