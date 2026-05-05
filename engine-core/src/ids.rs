//! Typed identifiers used across the engine.
//!
//! Plain integer aliases: cheap to store in account data, and the seeds of the
//! deterministic addressing scheme in [`crate::addressing`].

/// Identifies a world (a self-contained ECS state space + policy).
pub type WorldId = u64;

/// Identifies an entity *within a world*. Allocated monotonically by the world;
/// `0` is reserved as the null entity.
pub type EntityId = u64;

/// Identifies a Component *schema* within a world. Allocated monotonically by the
/// world's schema registry.
pub type ComponentId = u32;

/// Identifies a System (on-chain instruction handler) registered against a world.
pub type SystemId = u32;

/// The null entity. Used as a sentinel for "no reference" in `Entity` fields.
pub const NULL_ENTITY: EntityId = 0;
