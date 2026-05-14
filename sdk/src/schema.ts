// Schema registry. Components are fixed-size scalar records.

import { Hasher, utf8 } from "./hash.ts";
import { EngineError } from "./errors.ts";

/** Scalar field types. `{ bytes: n }` is a fixed-length blob of `n` bytes. */
export type FieldType =
  | "u8"
  | "u16"
  | "u32"
  | "u64"
  | "i64"
  | "bool"
  | "pubkey"
  | "entity"
  | { bytes: number };

/** Fixed encoded width of a field type, in bytes. */
export function fieldSize(ty: FieldType): number {
  if (typeof ty === "object") return ty.bytes;
  switch (ty) {
    case "u8":
    case "bool":
      return 1;
    case "u16":
      return 2;
    case "u32":
      return 4;
    case "u64":
    case "i64":
    case "entity":
      return 8;
    case "pubkey":
      return 32;
  }
}

/** Stable wire discriminant for a field type. */
export function fieldTag(ty: FieldType): number {
  if (typeof ty === "object") return 9; // bytes
  switch (ty) {
    case "u8":
      return 1;
    case "u16":
      return 2;
    case "u32":
      return 3;
    case "u64":
      return 4;
    case "i64":
      return 5;
    case "bool":
      return 6;
    case "pubkey":
      return 7;
    case "entity":
      return 8;
  }
}

export interface Field {
  name: string;
  ty: FieldType;
}

export function field(name: string, ty: FieldType): Field {
  return { name, ty };
}

export class ComponentSchema {
  readonly id: number;
  readonly name: string;
  readonly fields: Field[];

  constructor(id: number, name: string, fields: Field[]) {
    this.id = id;
    this.name = name;
    this.fields = fields;
  }

  /** Total fixed size of one record. */
  size(): number {
    return this.fields.reduce((acc, f) => acc + fieldSize(f.ty), 0);
  }

  /** Byte offset and type of a field by name. */
  field(name: string): { offset: number; ty: FieldType } | undefined {
    let offset = 0;
    for (const f of this.fields) {
      if (f.name === name) return { offset, ty: f.ty };
      offset += fieldSize(f.ty);
    }
    return undefined;
  }

  /** Structural layout digest. */
  layoutHash(): bigint {
    const h = new Hasher();
    h.write(utf8("loom:schema")).writeU32(this.id).write(utf8(this.name));
    for (const f of this.fields) {
      h.writeU8(0xff).write(utf8(f.name)).writeU8(fieldTag(f.ty));
      if (typeof f.ty === "object") h.writeU16(f.ty.bytes);
    }
    return h.finish();
  }
}

/** A world's registry of Component schemas, ordered by id. */
export class SchemaRegistry {
  private schemas = new Map<number, ComponentSchema>();
  private nextId = 0;

  register(name: string, fields: Field[]): number {
    const id = this.nextId++;
    this.schemas.set(id, new ComponentSchema(id, name, fields));
    return id;
  }

  registerWithId(id: number, name: string, fields: Field[]): void {
    if (this.schemas.has(id)) {
      throw new EngineError("DuplicateComponent", `component ${id} already registered`, { id });
    }
    this.schemas.set(id, new ComponentSchema(id, name, fields));
    this.nextId = Math.max(this.nextId, id + 1);
  }

  get(id: number): ComponentSchema {
    const s = this.schemas.get(id);
    if (!s) throw new EngineError("UnknownComponent", `unknown component ${id}`, { id });
    return s;
  }

  has(id: number): boolean {
    return this.schemas.has(id);
  }

  get length(): number {
    return this.schemas.size;
  }

  /** Schemas in ascending id order. */
  iter(): ComponentSchema[] {
    return [...this.schemas.keys()].sort((a, b) => a - b).map((id) => this.schemas.get(id)!);
  }
}
