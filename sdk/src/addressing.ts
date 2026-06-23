// Deterministic Component addressing. `componentAddress` is the join key the
// indexer uses to reconstruct world state from raw Component accounts, derived
// from the PDA seed scheme:
//
//   seeds = [ "loom", "cmp", world_id.le, entity_id.le, component_id.le ]

import { Hasher } from "./hash.ts";

const COMPONENT_TAG = new TextEncoder().encode("loom:cmp");

/** A 32-byte logical Component address. */
export type Address = Uint8Array;

/** Derive the logical address of the Component at (world, entity, component). */
export function componentAddress(
  world: bigint,
  entity: bigint,
  component: number,
): Address {
  const out = new Uint8Array(32);
  const dv = new DataView(out.buffer);
  for (let lane = 0; lane < 4; lane++) {
    const word = new Hasher()
      .write(COMPONENT_TAG)
      .writeU8(lane)
      .writeU64(world)
      .writeU64(entity)
      .writeU32(component)
      .finish();
    dv.setBigUint64(lane * 8, word, true); // little-endian
  }
  return out;
}

/** Lowercase hex of an address (64 chars). */
export function toHex(addr: Address): string {
  let s = "";
  for (const b of addr) s += b.toString(16).padStart(2, "0");
  return s;
}

/**
 * Parse a 64-char lowercase hex string back into an address - the inverse of
 * `toHex`. Returns undefined unless the input is exactly 32 bytes of hex.
 */
export function fromHex(s: string): Address | undefined {
  if (!/^[0-9a-f]{64}$/.test(s)) return undefined;
  const out = new Uint8Array(32);
  for (let i = 0; i < 32; i++) out[i] = Number.parseInt(s.slice(2 * i, 2 * i + 2), 16);
  return out;
}
