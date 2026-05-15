// Indexer - reconstructs renderable world state from Component records.
// On-chain this reads Component accounts by their PDAs; here it reads the
// local runtime's store, joining on the Component address either way.

import type { World } from "./world.ts";
import type { ComponentSchema, Field } from "./schema.ts";
import { Record as LoomRecord } from "./record.ts";
import { toHex } from "./addressing.ts";

export type FieldValue = number | bigint | boolean | Uint8Array;
export type DecodedRecord = { [field: string]: FieldValue };
/** A single Component record reconstructed from its account. */
export interface ComponentRow {
  entity: bigint;
  address: string;
  fields: DecodedRecord;
}
/** All Components attached to one entity, keyed by Component name. */
export type EntityView = { [component: string]: DecodedRecord };

function readField(rec: LoomRecord, f: Field): FieldValue {
  const ty = f.ty;
  if (typeof ty === "object") return rec.getBytes(f.name);
  switch (ty) {
    case "u8":
      return rec.getU8(f.name);
    case "u16":
      return rec.getU16(f.name);
    case "u32":
      return rec.getU32(f.name);
    case "u64":
      return rec.getU64(f.name);
    case "i64":
      return rec.getI64(f.name);
    case "bool":
      return rec.getBool(f.name);
    case "entity":
      return rec.getEntity(f.name);
    case "pubkey":
      return rec.getPubkey(f.name);
  }
}

export function decodeRecord(schema: ComponentSchema, bytes: Uint8Array): DecodedRecord {
  const rec = LoomRecord.fromBytes(schema, bytes.slice());
  const out: DecodedRecord = {};
  for (const f of schema.fields) out[f.name] = readField(rec, f);
  return out;
}

export class Indexer {
  private world: World;

  constructor(world: World) {
    this.world = world;
  }

  /** Every Component attached to `entity`, decoded. */
  entityView(entity: bigint): EntityView {
    const view: EntityView = {};
    for (const schema of this.world.registry().iter()) {
      const bytes = this.world.get(schema.id, entity);
      if (bytes) view[schema.name] = decodeRecord(schema, bytes);
    }
    return view;
  }

  /** Every record of one Component, with its on-chain address. */
  componentTable(component: number): ComponentRow[] {
    const schema = this.world.schema(component);
    return this.world.entitiesWith(component).map((entity) => ({
      entity,
      address: toHex(this.world.address(component, entity)),
      fields: decodeRecord(schema, this.world.get(component, entity)!),
    }));
  }

  /** Full snapshot keyed by Component name. */
  snapshot(): { [component: string]: ComponentRow[] } {
    const out: { [component: string]: ComponentRow[] } = {};
    for (const schema of this.world.registry().iter()) {
      out[schema.name] = this.componentTable(schema.id);
    }
    return out;
  }
}
