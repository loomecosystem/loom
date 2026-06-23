//! The compute bridge: settle off-chain work on-chain.
//!
//! Heavy work (pathfinding, physics, AI) can't run inside a Solana transaction, so
//! it runs off-chain and only the result is settled on-chain. v1 is optimistic: a
//! worker posts a result with a bond, opening a fraud-proof window. If nobody
//! disproves it before the window closes, it finalizes and a System may consume
//! it. If a challenger re-derives a different result inside the window, the poster
//! is slashed.
//!
//! [`ComputeBridge::post_verified`] is the ZK lane: a SP1 / RISC Zero proof checked
//! by an on-chain verifier finalizes a result immediately with no window. The
//! verifier is injected so the core stays dependency-free.

use crate::error::EngineError;
use crate::hash::fnv1a;
use std::collections::BTreeMap;

/// Lifecycle of a posted result.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClaimStatus {
    /// Posted optimistically; disputable until `until_slot`.
    Challengeable { until_slot: u64 },
    /// Window elapsed unchallenged, or a ZK proof verified. Consumable.
    Finalized,
    /// A valid fraud proof contradicted it; the poster's bond was slashed.
    Disputed,
}

/// One settled (or settling) unit of off-chain compute.
#[derive(Clone, Debug)]
pub struct ComputeClaim {
    pub id: u64,
    /// Opaque job-kind tag (e.g. a pathfinding request type).
    pub task: u64,
    /// Hash of the off-chain inputs, binding this result to a specific request.
    pub input_hash: u64,
    /// The posted result bytes (e.g. a serialized path).
    pub result: Vec<u8>,
    /// FNV-1a of `result`, the quantity a fraud proof contradicts.
    pub result_hash: u64,
    /// Who posted it (a 32-byte pubkey), and their at-risk bond.
    pub poster: [u8; 32],
    pub bond: u64,
    pub status: ClaimStatus,
}

/// Registry of compute claims plus bond accounting.
#[derive(Clone, Debug)]
pub struct ComputeBridge {
    claims: BTreeMap<u64, ComputeClaim>,
    next_id: u64,
    /// Length of the fraud-proof window in slots.
    pub window_slots: u64,
    /// Total bond slashed from posters proven fraudulent.
    pub total_slashed: u64,
}

impl ComputeBridge {
    pub fn new(window_slots: u64) -> Self {
        Self {
            claims: BTreeMap::new(),
            next_id: 0,
            window_slots,
            total_slashed: 0,
        }
    }

    /// Post an optimistic result, opening the fraud-proof window. Returns the
    /// claim id.
    pub fn post_result(
        &mut self,
        task: u64,
        input_hash: u64,
        result: Vec<u8>,
        poster: [u8; 32],
        bond: u64,
        now_slot: u64,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let result_hash = fnv1a(&result);
        self.claims.insert(
            id,
            ComputeClaim {
                id,
                task,
                input_hash,
                result,
                result_hash,
                poster,
                bond,
                status: ClaimStatus::Challengeable {
                    until_slot: now_slot + self.window_slots,
                },
            },
        );
        id
    }

    /// Post a result with a validity proof. If `verify` accepts the proof against
    /// the result, the claim finalizes immediately with no window. This is where an
    /// on-chain SP1/RISC Zero verifier plugs in.
    pub fn post_verified(
        &mut self,
        task: u64,
        input_hash: u64,
        result: Vec<u8>,
        poster: [u8; 32],
        proof: &[u8],
        verify: impl Fn(u64, u64, &[u8]) -> bool,
    ) -> Result<u64, EngineError> {
        let result_hash = fnv1a(&result);
        if !verify(input_hash, result_hash, proof) {
            return Err(EngineError::FraudProofInvalid);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.claims.insert(
            id,
            ComputeClaim {
                id,
                task,
                input_hash,
                result,
                result_hash,
                poster,
                bond: 0,
                status: ClaimStatus::Finalized,
            },
        );
        Ok(id)
    }

    pub fn get(&self, id: u64) -> Result<&ComputeClaim, EngineError> {
        self.claims.get(&id).ok_or(EngineError::ClaimNotFinalized)
    }

    /// Challenge a claim by supplying the correct recomputation. If its hash
    /// differs from the posted result, fraud is proven: the claim is disputed and
    /// the bond slashed. If it matches, the challenge is rejected.
    pub fn challenge(
        &mut self,
        id: u64,
        recomputed_result: &[u8],
        now_slot: u64,
    ) -> Result<(), EngineError> {
        let claim = self.claims.get_mut(&id).ok_or(EngineError::ClaimNotFinalized)?;
        match claim.status {
            ClaimStatus::Challengeable { until_slot } => {
                if now_slot >= until_slot {
                    return Err(EngineError::ClaimWindowOpen); // window already closed
                }
            }
            _ => return Err(EngineError::ClaimAlreadySettled),
        }
        if fnv1a(recomputed_result) == claim.result_hash {
            // No contradiction - the posted result stands.
            return Err(EngineError::FraudProofInvalid);
        }
        claim.status = ClaimStatus::Disputed;
        self.total_slashed += claim.bond;
        Ok(())
    }

    /// Finalize a claim whose window has elapsed without a successful challenge.
    pub fn finalize(&mut self, id: u64, now_slot: u64) -> Result<(), EngineError> {
        let claim = self.claims.get_mut(&id).ok_or(EngineError::ClaimNotFinalized)?;
        match claim.status {
            ClaimStatus::Challengeable { until_slot } => {
                if now_slot < until_slot {
                    return Err(EngineError::ClaimWindowOpen);
                }
                claim.status = ClaimStatus::Finalized;
                Ok(())
            }
            _ => Err(EngineError::ClaimAlreadySettled),
        }
    }

    /// Read a finalized result, checking it answers the expected request. This is
    /// what a System calls to fold off-chain compute back into world state.
    pub fn consume(&self, id: u64, expected_input_hash: u64) -> Result<&[u8], EngineError> {
        let claim = self.claims.get(&id).ok_or(EngineError::ClaimNotFinalized)?;
        if claim.status != ClaimStatus::Finalized {
            return Err(EngineError::ClaimNotFinalized);
        }
        if claim.input_hash != expected_input_hash {
            return Err(EngineError::ClaimInputMismatch {
                expected: expected_input_hash,
                got: claim.input_hash,
            });
        }
        Ok(&claim.result)
    }
}
