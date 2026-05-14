// Typed Component records over raw bytes. Little-endian throughout to match
// Solana account encoding; 64-bit fields use BigInt so values round-trip exactly.

import { ComponentSchema, type FieldType, fieldTag } from "./schema.ts";
import { EngineError } from "./errors.ts";

export class Record {
  readonly schema: ComponentSchema;
  private bytes: Uint8Array;
  private dv: DataView;

  constructor(schema: ComponentSchema, bytes: Uint8Array) {
    this.schema = schema;
    this.bytes = bytes;
    this.dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  }

  static zeroed(schema: ComponentSchema): Record {
    return new Record(schema, new Uint8Array(schema.size()));
  }

  static fromBytes(schema: ComponentSchema, bytes: Uint8Array): Record {
    if (bytes.length !== schema.size()) {
      throw new EngineError("BadRecordSize", "record size mismatch", {
        component: schema.id,
        expected: schema.size(),
        got: bytes.length,
      });
    }
    return new Record(schema, bytes);
  }

  /** The underlying bytes. */
  toBytes(): Uint8Array {
    return this.bytes;
  }

  private slot(name: string, want: FieldType): number {
    const f = this.schema.field(name);
    if (!f) throw new EngineError("UnknownField", `unknown field \`${name}\``, { name });
    if (fieldTag(f.ty) !== fieldTag(want) || typeof f.ty === "object") {
      throw new EngineError("FieldTypeMismatch", `field \`${name}\` has a different type`, { name });
    }
    return f.offset;
  }

  private bytesSlot(name: string): { offset: number; len: number } {
    const f = this.schema.field(name);
    if (!f) throw new EngineError("UnknownField", `unknown field \`${name}\``, { name });
    if (typeof f.ty !== "object") {
      throw new EngineError("FieldTypeMismatch", `field \`${name}\` is not bytes`, { name });
    }
    return { offset: f.offset, len: f.ty.bytes };
  }

  setU8(name: string, v: number): this {
    this.dv.setUint8(this.slot(name, "u8"), v & 0xff);
    return this;
  }
  getU8(name: string): number {
    return this.dv.getUint8(this.slot(name, "u8"));
  }

  setU16(name: string, v: number): this {
    this.dv.setUint16(this.slot(name, "u16"), v & 0xffff, true);
    return this;
  }
  getU16(name: string): number {
    return this.dv.getUint16(this.slot(name, "u16"), true);
  }

  setU32(name: string, v: number): this {
    this.dv.setUint32(this.slot(name, "u32"), v >>> 0, true);
    return this;
  }
  getU32(name: string): number {
    return this.dv.getUint32(this.slot(name, "u32"), true);
  }

  setU64(name: string, v: bigint): this {
    this.dv.setBigUint64(this.slot(name, "u64"), v, true);
    return this;
  }
  getU64(name: string): bigint {
    return this.dv.getBigUint64(this.slot(name, "u64"), true);
  }

  setI64(name: string, v: bigint): this {
    this.dv.setBigInt64(this.slot(name, "i64"), v, true);
    return this;
  }
  getI64(name: string): bigint {
    return this.dv.getBigInt64(this.slot(name, "i64"), true);
  }

  setBool(name: string, v: boolean): this {
    this.dv.setUint8(this.slot(name, "bool"), v ? 1 : 0);
    return this;
  }
  getBool(name: string): boolean {
    return this.dv.getUint8(this.slot(name, "bool")) !== 0;
  }

  setEntity(name: string, v: bigint): this {
    this.dv.setBigUint64(this.slot(name, "entity"), v, true);
    return this;
  }
  getEntity(name: string): bigint {
    return this.dv.getBigUint64(this.slot(name, "entity"), true);
  }

  setPubkey(name: string, v: Uint8Array): this {
    const o = this.slot(name, "pubkey");
    if (v.length !== 32) {
      throw new EngineError("FieldTypeMismatch", `pubkey \`${name}\` must be 32 bytes`, { name });
    }
    this.bytes.set(v, o);
    return this;
  }
  getPubkey(name: string): Uint8Array {
    const o = this.slot(name, "pubkey");
    return this.bytes.slice(o, o + 32);
  }

  setBytes(name: string, data: Uint8Array): this {
    const { offset, len } = this.bytesSlot(name);
    if (data.length !== len) {
      throw new EngineError("FieldTypeMismatch", `bytes \`${name}\` must be ${len} bytes`, { name });
    }
    this.bytes.set(data, offset);
    return this;
  }
  getBytes(name: string): Uint8Array {
    const { offset, len } = this.bytesSlot(name);
    return this.bytes.slice(offset, offset + len);
  }
}
