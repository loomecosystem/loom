//! Deterministic Component addressing.
//!
//! Solana stores state in accounts at Program-Derived Addresses (PDAs), where a
//! PDA is `find_program_address(seeds, program_id)`. Loom fixes the seed scheme so
//! every Component account in every world is addressable from
//! `(world_id, entity_id, component_id)` alone, with no external index:
//!
//! ```text
//! seeds = [ b"loom", b"cmp", world_id.le, entity_id.le, component_id.le ]
//! ```
//!
//! The on-chain program passes these seeds to `find_program_address` for the real
//! account address. Off-chain (indexer, SDK, this core) we want the same identity
//! without ed25519 curve math, so we derive a 32-byte **logical address** by
//! hashing the seeds with FNV-1a, domain-separated per 8-byte lane. The Rust core
//! and TypeScript SDK compute it identically; it is the join key the indexer uses
//! to reconstruct world state from raw Component accounts.

use crate::hash::Hasher;
use crate::ids::{ComponentId, EntityId, WorldId};

/// Domain tag mixed into every Component address derivation.
pub const COMPONENT_TAG: &[u8] = b"loom:cmp";

/// A 32-byte logical address identifying one Component record.
pub type Address = [u8; 32];

/// Derive the logical address of the Component record at
/// `(world_id, entity_id, component_id)`.
///
/// Mirrored exactly by `deriveComponentAddress` in the TypeScript SDK.
pub fn component_address(
    world: WorldId,
    entity: EntityId,
    component: ComponentId,
) -> Address {
    let mut out = [0u8; 32];
    // Four independent 8-byte lanes, each domain-separated by its lane index, so
    // the full 32 bytes carry 256 bits of derived identity rather than a repeated
    // 64-bit value.
    for (lane, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = Hasher::new();
        h.write(COMPONENT_TAG)
            .write_u8(lane as u8)
            .write_u64(world)
            .write_u64(entity)
            .write_u32(component);
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    out
}

/// Lowercase hex rendering of an [`Address`], for logs and the indexer's keys.
pub fn to_hex(addr: &Address) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(64);
    for &b in addr {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_distinct() {
        let a = component_address(1, 1, 1);
        assert_eq!(a, component_address(1, 1, 1), "must be deterministic");
        assert_ne!(a, component_address(2, 1, 1), "world distinguishes");
        assert_ne!(a, component_address(1, 2, 1), "entity distinguishes");
        assert_ne!(a, component_address(1, 1, 2), "component distinguishes");
    }

    #[test]
    fn lanes_are_independent() {
        // If lane domain separation were missing, all 4 lanes would be equal.
        let a = component_address(7, 42, 3);
        let lane0 = &a[0..8];
        let lane1 = &a[8..16];
        assert_ne!(lane0, lane1);
    }

    #[test]
    fn hex_is_64_chars() {
        let a = component_address(1, 1, 1);
        assert_eq!(to_hex(&a).len(), 64);
    }
}
