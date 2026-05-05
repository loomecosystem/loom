//! FNV-1a 64-bit hashing.
//!
//! Used for the state digest rather than a cryptographic hash: it is dependency-free,
//! deterministic, and trivially re-implementable byte-for-byte in the TypeScript SDK.
//! The digest is a conformance/integrity check, not a security primitive; fraud-proof
//! security comes from recomputation and slashing, not collision resistance.

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A streaming FNV-1a 64-bit hasher with little-endian integer helpers.
///
/// The byte order is part of the wire contract and is mirrored by the TypeScript SDK.
#[derive(Clone, Copy, Debug)]
pub struct Hasher {
    state: u64,
}

impl Default for Hasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Hasher {
    /// Start a fresh hasher seeded with the FNV offset basis.
    pub fn new() -> Self {
        Self {
            state: FNV_OFFSET_BASIS,
        }
    }

    /// Absorb raw bytes.
    pub fn write(&mut self, bytes: &[u8]) -> &mut Self {
        for &b in bytes {
            self.state ^= b as u64;
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
        self
    }

    /// Absorb a single byte.
    pub fn write_u8(&mut self, v: u8) -> &mut Self {
        self.write(&[v])
    }

    /// Absorb a `u32` in little-endian order.
    pub fn write_u32(&mut self, v: u32) -> &mut Self {
        self.write(&v.to_le_bytes())
    }

    /// Absorb a `u64` in little-endian order.
    pub fn write_u64(&mut self, v: u64) -> &mut Self {
        self.write(&v.to_le_bytes())
    }

    /// Finish and return the 64-bit digest.
    pub fn finish(&self) -> u64 {
        self.state
    }
}

/// One-shot convenience over a byte slice.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h = Hasher::new();
    h.write(bytes);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        // Canonical FNV-1a 64 test vectors.
        assert_eq!(fnv1a(b""), FNV_OFFSET_BASIS);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn streaming_matches_oneshot() {
        let mut h = Hasher::new();
        h.write(b"foo").write(b"bar");
        assert_eq!(h.finish(), fnv1a(b"foobar"));
    }
}
