// A world: one ECS state space, schema registry, and governance. The store
// iteration order (component asc, then entity asc) defines the canonical
// stateHash encoding and the tick coordinator's resumable cursor order.

import { Hasher, utf8 } from "./hash.ts";
import { type Address, componentAddress } from "./addressing.ts";
import { ComponentSchema, type Field, SchemaRegistry } from "./schema.ts";
import { Record } from "./record.ts";
import { EngineError } from "./errors.ts";

const NULL_ENTITY = 0n;

export class World {
  readonly id: bigint;
  readonly authority: Uint8Array;
  private frozen = false;
  private schemas = new SchemaRegistry();
  private nextEntity = 1n; // entity 0 is the null entity
  private store = new Map<number, Map<bigint, Uint8Array>>();
  private dirtySet = new Set<string>();

  constructor(id: bigint, authority: Uint8Array = new Uint8Array(32)) {
    this.id = id;
    this.authority = authority;
  }

  // --- governance ---

  freeze(): void {
    this.frozen = true;
  }
  isFrozen(): boolean {
    return this.frozen;
  }

  // --- schema ---

  registerComponent(name: string, fields: Field[]): number {
    if (this.frozen) throw new EngineError("WorldFrozen", "world rules are frozen");
    return this.schemas.register(name, fields);
  }
  schema(component: number): ComponentSchema {
    return this.schemas.get(component);
  }
  registry(): SchemaRegistry {
    return this.schemas;
  }

  // --- entities ---

  spawnEntity(): bigint {
    const e = this.nextEntity;
    this.nextEntity += 1n;
    return e;
  }
  entityBound(): bigint {
    return this.nextEntity;
  }
  private checkEntity(entity: bigint): void {
    if (entity === NULL_ENTITY || entity >= this.nextEntity) {
      throw new EngineError("UnknownEntity", `unknown entity ${entity}`, { entity });
    }
  }

  // --- component data ---

  record(component: number): Record {
    return Record.zeroed(this.schemas.get(component));
  }

  set(component: number, entity: bigint, bytes: Uint8Array): void {
    const schema = this.schemas.get(component);
    if (bytes.length !== schema.size()) {
      throw new EngineError("BadRecordSize", "record size mismatch", {
        component,
        expected: schema.size(),
        got: bytes.length,
      });
    }
    this.checkEntity(entity);
    let inner = this.store.get(component);
    if (!inner) {
      inner = new Map();
      this.store.set(component, inner);
    }
    inner.set(entity, bytes);
    this.dirtySet.add(`${component}:${entity}`);
  }

  get(component: number, entity: bigint): Uint8Array | undefined {
    return this.store.get(component)?.get(entity);
  }

  read(component: number, entity: bigint): Record | undefined {
    const bytes = this.get(component, entity);
    if (!bytes) return undefined;
    return Record.fromBytes(this.schemas.get(component), bytes.slice());
  }

  has(component: number, entity: bigint): boolean {
    return this.store.get(component)?.has(entity) ?? false;
  }

  remove(component: number, entity: bigint): boolean {
    const existed = this.store.get(component)?.delete(entity) ?? false;
    if (existed) this.dirtySet.add(`${component}:${entity}`);
    return existed;
  }

  address(component: number, entity: bigint): Address {
    return componentAddress(this.id, entity, component);
  }

  // --- queries ---

  /** Entities that have `component`, ascending. */
  entitiesWith(component: number): bigint[] {
    const inner = this.store.get(component);
    if (!inner) return [];
    return [...inner.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  }

  /** Entities that have `component` with id >= from, ascending. */
  entitiesWithFrom(component: number, from: bigint): bigint[] {
    return this.entitiesWith(component).filter((e) => e >= from);
  }

  countWith(component: number): number {
    return this.store.get(component)?.size ?? 0;
  }

  /** Total bytes of Component data stored; the basis for storage rent. */
  storageBytes(): bigint {
    let total = 0n;
    for (const inner of this.store.values()) {
      for (const bytes of inner.values()) total += BigInt(bytes.length);
    }
    return total;
  }

  // --- dirty set ---

  dirtyPairs(): { component: number; entity: bigint }[] {
    return [...this.dirtySet].map((k) => {
      const i = k.indexOf(":");
      return { component: Number(k.slice(0, i)), entity: BigInt(k.slice(i + 1)) };
    });
  }
  dirtyLen(): number {
    return this.dirtySet.size;
  }
  clearDirty(): void {
    this.dirtySet.clear();
  }

  // --- determinism ---

  /** Canonical FNV-1a digest of world state. */
  stateHash(): bigint {
    const h = new Hasher();
    h.write(utf8("loom:world")).writeU64(this.id).writeU8(this.frozen ? 1 : 0);
    for (const s of this.schemas.iter()) {
      h.writeU32(s.id).writeU64(s.layoutHash());
    }
    const components = [...this.store.keys()].sort((a, b) => a - b);
    for (const c of components) {
      const inner = this.store.get(c)!;
      const entities = [...inner.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
      for (const e of entities) {
        const bytes = inner.get(e)!;
        h.writeU32(c).writeU64(e).writeU32(bytes.length).write(bytes);
      }
    }
    return h.finish();
  }
}
