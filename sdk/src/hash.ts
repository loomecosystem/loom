// FNV-1a 64-bit. Arithmetic is BigInt masked to 64 bits to match wrapping u64.

const MASK = (1n << 64n) - 1n;
const OFFSET_BASIS = 0xcbf29ce484222325n;
const PRIME = 0x100000001b3n;

const encoder = new TextEncoder();

/** UTF-8 encode a string. */
export function utf8(s: string): Uint8Array {
  return encoder.encode(s);
}

/** Streaming FNV-1a 64 hasher with little-endian integer helpers. */
export class Hasher {
  private state = OFFSET_BASIS;

  write(bytes: Uint8Array): this {
    let h = this.state;
    for (const b of bytes) {
      h = (h ^ BigInt(b)) & MASK;
      h = (h * PRIME) & MASK;
    }
    this.state = h;
    return this;
  }

  writeU8(v: number): this {
    return this.write(Uint8Array.of(v & 0xff));
  }

  writeU16(v: number): this {
    const b = new Uint8Array(2);
    new DataView(b.buffer).setUint16(0, v & 0xffff, true);
    return this.write(b);
  }

  writeU32(v: number): this {
    const b = new Uint8Array(4);
    new DataView(b.buffer).setUint32(0, v >>> 0, true);
    return this.write(b);
  }

  writeU64(v: bigint | number): this {
    const b = new Uint8Array(8);
    new DataView(b.buffer).setBigUint64(0, BigInt(v) & MASK, true);
    return this.write(b);
  }

  finish(): bigint {
    return this.state;
  }
}

/** One-shot FNV-1a 64 over a byte slice. */
export function fnv1a(bytes: Uint8Array): bigint {
  return new Hasher().write(bytes).finish();
}

/** Lowercase `0x`-prefixed hex of a 64-bit digest, zero-padded to 16 nibbles. */
export function toHex64(h: bigint): string {
  return "0x" + h.toString(16).padStart(16, "0");
}
