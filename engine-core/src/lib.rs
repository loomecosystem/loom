//! # Loom Engine Core
//!
//! The deterministic core of the Loom on-chain game engine: the state model and
//! systems that make a fully on-chain world viable on Solana.
//!
//! - an **ECS** state model where Components are typed records keyed by
//!   `(world_id, entity_id, component_id)` ([`addressing`], [`schema`],
//!   [`component`], [`world`]),
//! - a **System** framework with engine-enforced read/write access control
//!   ([`system`]),
//! - a **tick coordinator** with bounded-work cranks, resumable cursors and a
//!   dirty set, to stay under Solana's compute budget ([`tick`]),
//! - a **compute bridge** that settles off-chain work optimistically behind a
//!   fraud-proof window ([`bridge`]),
//! - **composability**: external Systems (mods) constrained by a world policy
//!   ([`mods`]).
//!
//! Everything here is pure, deterministic computation over plain bytes: no I/O,
//! no clock, no randomness, with slot/time always an explicit input. The same
//! logic therefore runs on-chain and under `cargo test`, and a
//! [`world::World::state_hash`] doubles as a cross-implementation conformance
//! check against the TypeScript SDK.

pub mod addressing;
pub mod bridge;
pub mod component;
pub mod economy;
pub mod error;
pub mod hash;
pub mod ids;
pub mod mods;
pub mod schema;
pub mod system;
pub mod tick;
pub mod world;

pub use error::EngineError;
pub use ids::{ComponentId, EntityId, SystemId, WorldId};
pub use schema::{ComponentSchema, Field, FieldType, SchemaRegistry};
pub use system::{Access, System, SystemCtx};
pub use world::World;

/// Result type used throughout the engine core.
pub type Result<T> = core::result::Result<T, EngineError>;
