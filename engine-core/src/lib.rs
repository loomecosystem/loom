//! Loom engine core: the deterministic ECS state model shared by the on-chain
//! program and the TypeScript SDK. Pure, dependency-free computation over bytes.

pub mod addressing;
pub mod component;
pub mod error;
pub mod hash;
pub mod ids;
pub mod schema;
pub mod system;
pub mod tick;
pub mod world;

pub use error::EngineError;
pub use ids::{ComponentId, EntityId, SystemId, WorldId};
pub use schema::{ComponentSchema, Field, FieldType, SchemaRegistry};
pub use system::{Access, System, SystemCtx};
pub use world::World;

pub type Result<T> = core::result::Result<T, EngineError>;
