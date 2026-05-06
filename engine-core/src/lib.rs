//! Loom engine core: the deterministic ECS state model shared by the on-chain
//! program and the TypeScript SDK. Pure, dependency-free computation over bytes.

pub mod addressing;
pub mod component;
pub mod error;
pub mod hash;
pub mod ids;
pub mod schema;
