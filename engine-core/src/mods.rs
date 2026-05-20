//! Composability: third-party Systems and cross-world references.
//!
//! A mod is a new System operating on a world's existing Components, with no
//! redeploy of the world. The world's [`ModPolicy`] declares which Components
//! external Systems may write (and optionally read); [`admit_mod`] checks a
//! candidate System's declared [`Access`] against that policy before it cranks.
//! Per-access enforcement ([`crate::system::SystemCtx`]) then keeps the mod from
//! exceeding what it declared.
//!
//! [`CrossWorldRef`] lets world B reference a Component in world A: because layouts
//! live in an on-chain schema registry, the reference holds as long as A still
//! exposes a byte-compatible layout.
//! [`ComponentSchema::layout_hash`](crate::schema::ComponentSchema::layout_hash)
//! is the version check that keeps it safe across upgrades.

use crate::error::EngineError;
use crate::ids::{ComponentId, EntityId, WorldId};
use crate::system::{Access, System};
use crate::world::World;

/// What a world grants to external Systems.
#[derive(Clone, Default, Debug)]
pub struct ModPolicy {
    /// Components mods may write (and therefore also read).
    pub writable: Vec<ComponentId>,
    /// Components mods may read when [`ModPolicy::open_read`] is false.
    pub readable: Vec<ComponentId>,
    /// If true, mods may read any Component in the world.
    pub open_read: bool,
}

impl ModPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn allow_write(mut self, c: ComponentId) -> Self {
        self.writable.push(c);
        self
    }

    pub fn allow_read(mut self, c: ComponentId) -> Self {
        self.readable.push(c);
        self
    }

    pub fn with_open_read(mut self) -> Self {
        self.open_read = true;
        self
    }

    /// Check a declared [`Access`] against this policy.
    pub fn permits(&self, access: &Access) -> Result<(), EngineError> {
        for &w in &access.writes {
            if !self.writable.contains(&w) {
                return Err(EngineError::ModPermissionDenied { component: w });
            }
        }
        if !self.open_read {
            for &r in &access.reads {
                if !self.readable.contains(&r) && !self.writable.contains(&r) {
                    return Err(EngineError::ModPermissionDenied { component: r });
                }
            }
        }
        Ok(())
    }
}

/// Admit an external System to a world under its policy. Returns the System's id
/// on success so the world can record which mods are live. Errors with
/// [`EngineError::ModPermissionDenied`] naming the first Component the mod is not
/// allowed to touch.
pub fn admit_mod<S: System + ?Sized>(
    policy: &ModPolicy,
    system: &S,
) -> Result<crate::ids::SystemId, EngineError> {
    policy.permits(&system.access())?;
    Ok(system.id())
}

/// A reference from one world to a Component in another, validated by layout.
#[derive(Clone, Copy, Debug)]
pub struct CrossWorldRef {
    pub world: WorldId,
    pub component: ComponentId,
    /// The layout world B expects world A's Component to have.
    pub expected_layout_hash: u64,
}

impl CrossWorldRef {
    /// Resolve this reference against the foreign world, returning the referenced
    /// entity's Component bytes. Fails if the foreign world is the wrong one, the
    /// Component's layout has drifted, or the entity lacks the Component.
    pub fn resolve<'a>(
        &self,
        foreign: &'a World,
        entity: EntityId,
    ) -> Result<&'a [u8], EngineError> {
        if foreign.id != self.world {
            return Err(EngineError::CrossWorldMismatch {
                world: self.world,
                component: self.component,
            });
        }
        let schema = foreign.schema(self.component)?;
        if schema.layout_hash() != self.expected_layout_hash {
            return Err(EngineError::CrossWorldMismatch {
                world: self.world,
                component: self.component,
            });
        }
        foreign
            .get(self.component, entity)
            .ok_or(EngineError::UnknownEntity(entity))
    }
}
