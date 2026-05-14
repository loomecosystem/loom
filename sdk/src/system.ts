// Systems and engine-enforced access control.

import { World } from "./world.ts";
import { Record } from "./record.ts";
import { EngineError } from "./errors.ts";

/** The Components a System may touch. */
export class Access {
  reads: number[] = [];
  writes: number[] = [];

  static new(): Access {
    return new Access();
  }

  withReads(components: number[]): this {
    this.reads.push(...components);
    return this;
  }

  withWrites(components: number[]): this {
    this.writes.push(...components);
    return this;
  }

  canRead(c: number): boolean {
    return this.reads.includes(c) || this.writes.includes(c);
  }

  canWrite(c: number): boolean {
    return this.writes.includes(c);
  }
}

/** A unit of game logic. */
export interface System {
  id(): number;
  access(): Access;
  /** The Component whose entity set this System iterates. */
  query(): number;
  /** Per-entity work; must be bounded. */
  run(ctx: SystemCtx, entity: bigint): void;
}

/** The checked handle a running System uses to touch world state. */
export class SystemCtx {
  private world: World;
  private access: Access;
  /** The current slot/tick - the System's only notion of time. */
  readonly slot: bigint;

  constructor(world: World, access: Access, slot: bigint) {
    this.world = world;
    this.access = access;
    this.slot = slot;
  }

  worldId(): bigint {
    return this.world.id;
  }

  private guardRead(c: number): void {
    if (!this.access.canRead(c)) {
      throw new EngineError("AccessDenied", `system did not declare read on component ${c}`, {
        component: c,
        write: false,
      });
    }
  }

  private guardWrite(c: number): void {
    if (!this.access.canWrite(c)) {
      throw new EngineError("AccessDenied", `system did not declare write on component ${c}`, {
        component: c,
        write: true,
      });
    }
  }

  has(c: number, entity: bigint): boolean {
    this.guardRead(c);
    return this.world.has(c, entity);
  }

  spawn(): bigint {
    return this.world.spawnEntity();
  }

  private readRecord(c: number, entity: bigint): Record {
    this.guardRead(c);
    const schema = this.world.schema(c);
    const bytes = this.world.get(c, entity);
    return bytes ? Record.fromBytes(schema, bytes.slice()) : Record.zeroed(schema);
  }

  readU8(c: number, e: bigint, field: string): number {
    return this.readRecord(c, e).getU8(field);
  }
  readU32(c: number, e: bigint, field: string): number {
    return this.readRecord(c, e).getU32(field);
  }
  readU64(c: number, e: bigint, field: string): bigint {
    return this.readRecord(c, e).getU64(field);
  }
  readI64(c: number, e: bigint, field: string): bigint {
    return this.readRecord(c, e).getI64(field);
  }
  readBool(c: number, e: bigint, field: string): boolean {
    return this.readRecord(c, e).getBool(field);
  }
  readEntity(c: number, e: bigint, field: string): bigint {
    return this.readRecord(c, e).getEntity(field);
  }
  readBytes(c: number, e: bigint, field: string): Uint8Array {
    return this.readRecord(c, e).getBytes(field);
  }

  /** Read-modify-write a Component record under one write-access check. */
  mutate(c: number, entity: bigint, f: (r: Record) => void): void {
    this.guardWrite(c);
    const schema = this.world.schema(c);
    const existing = this.world.get(c, entity);
    const rec = existing ? Record.fromBytes(schema, existing.slice()) : Record.zeroed(schema);
    f(rec);
    this.world.set(c, entity, rec.toBytes());
  }
}
