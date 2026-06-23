//! The single error type for the engine core.

use crate::ids::{ComponentId, EntityId};

/// The error type for all fallible engine operations. Variants map 1:1 to on-chain
/// program error codes so the SDK surfaces the same failures locally and on-chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// A mutating operation was attempted on a world whose rules are locked
    /// (see [`crate::world::World::freeze`]).
    WorldFrozen,
    /// Tried to register a component schema id that already exists.
    DuplicateComponent(ComponentId),
    /// Referenced a component schema that was never registered.
    UnknownComponent(ComponentId),
    /// Referenced an entity that was never spawned.
    UnknownEntity(EntityId),
    /// A record's byte length did not match its schema's fixed size.
    BadRecordSize {
        component: ComponentId,
        expected: usize,
        got: usize,
    },
    /// A field name was not present in the component schema.
    UnknownField(String),
    /// A typed accessor was used against a field of a different type.
    FieldTypeMismatch { field: String },
    /// A System touched a component it did not declare in its [`crate::system::Access`].
    AccessDenied { component: ComponentId, write: bool },
    /// A System tried to consume a compute claim that is not yet finalized.
    ClaimNotFinalized,
    /// Tried to finalize a claim whose fraud-proof window is still open.
    ClaimWindowOpen,
    /// Tried to act on a claim that was already finalized or disputed.
    ClaimAlreadySettled,
    /// A finalized claim was consumed against a request it does not answer: its
    /// bound input hash differs from the one the consumer expected.
    ClaimInputMismatch { expected: u64, got: u64 },
    /// A fraud proof did not actually contradict the posted result.
    FraudProofInvalid,
    /// An external System (mod) tried to access a component the world policy
    /// does not grant it. `write` distinguishes a denied write from a denied read.
    ModPermissionDenied { component: ComponentId, write: bool },
    /// A cross-world reference pointed at the wrong world, or the referenced
    /// Component's on-chain layout has drifted from what the reference expects.
    CrossWorldMismatch {
        world: crate::ids::WorldId,
        component: ComponentId,
    },
    /// A world's prepaid engine-fee balance cannot cover a charge.
    InsufficientBalance {
        world: crate::ids::WorldId,
        needed: u64,
        have: u64,
    },
    /// The grants pool cannot cover a requested disbursement.
    InsufficientGrants { needed: u64, have: u64 },
}

impl core::fmt::Display for EngineError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use EngineError::*;
        match self {
            WorldFrozen => write!(f, "world rules are frozen"),
            DuplicateComponent(c) => write!(f, "component {c} already registered"),
            UnknownComponent(c) => write!(f, "unknown component {c}"),
            UnknownEntity(e) => write!(f, "unknown entity {e}"),
            BadRecordSize {
                component,
                expected,
                got,
            } => write!(
                f,
                "bad record size for component {component}: expected {expected}, got {got}"
            ),
            UnknownField(name) => write!(f, "unknown field `{name}`"),
            FieldTypeMismatch { field } => write!(f, "field `{field}` has a different type"),
            AccessDenied { component, write } => write!(
                f,
                "access denied: system did not declare {} on component {component}",
                if *write { "write" } else { "read" }
            ),
            ClaimNotFinalized => write!(f, "compute claim is not finalized"),
            ClaimWindowOpen => write!(f, "fraud-proof window is still open"),
            ClaimAlreadySettled => write!(f, "compute claim already settled"),
            ClaimInputMismatch { expected, got } => write!(
                f,
                "compute claim answers a different request: expected input {expected}, claim bound to {got}"
            ),
            FraudProofInvalid => write!(f, "fraud proof does not contradict the result"),
            ModPermissionDenied { component, write } => write!(
                f,
                "mod is not permitted to {} component {component}",
                if *write { "write" } else { "read" }
            ),
            CrossWorldMismatch { world, component } => write!(
                f,
                "cross-world reference to world {world} component {component} does not match"
            ),
            InsufficientBalance {
                world,
                needed,
                have,
            } => write!(
                f,
                "world {world} balance {have} cannot cover engine fee {needed}"
            ),
            InsufficientGrants { needed, have } => {
                write!(f, "grants pool {have} cannot cover disbursement {needed}")
            }
        }
    }
}

impl std::error::Error for EngineError {}
