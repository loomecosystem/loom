//! Systems and engine-enforced access control.
//!
//! A System declares the Components it reads and writes ([`Access`]), names the
//! Component whose entities it iterates ([`System::query`]), and does bounded
//! per-entity work ([`System::run`]).
//!
//! Access is enforced by the engine, not the System: a [`SystemCtx`] refuses any
//! read or write of a Component the System did not declare. That is what lets a
//! mod run safely against a world it did not author ([`crate::mods`]).

use crate::component::Record;
use crate::error::EngineError;
use crate::ids::{ComponentId, EntityId, SystemId, WorldId};
use crate::world::World;

/// The Components a System is allowed to touch.
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub struct Access {
    pub reads: Vec<ComponentId>,
    pub writes: Vec<ComponentId>,
}

impl Access {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reads(mut self, components: impl IntoIterator<Item = ComponentId>) -> Self {
        self.reads.extend(components);
        self
    }

    pub fn writes(mut self, components: impl IntoIterator<Item = ComponentId>) -> Self {
        self.writes.extend(components);
        self
    }

    pub fn can_read(&self, c: ComponentId) -> bool {
        // A declared write implies the right to read it back.
        self.reads.contains(&c) || self.writes.contains(&c)
    }

    pub fn can_write(&self, c: ComponentId) -> bool {
        self.writes.contains(&c)
    }
}

/// A unit of game logic. Implementors are typically tiny structs holding the
/// Component ids they were codegen'd or configured with.
pub trait System {
    fn id(&self) -> SystemId;

    /// The Components this System may read and write.
    fn access(&self) -> Access;

    /// The Component whose entity set this System iterates. The tick coordinator
    /// runs [`System::run`] once per entity that has this Component.
    fn query(&self) -> ComponentId;

    /// Per-entity work. Must be bounded (no unbounded loops over other entities).
    fn run(&self, ctx: &mut SystemCtx<'_>, entity: EntityId) -> Result<(), EngineError>;
}

/// The handle a running System uses to touch world state. Every access is checked
/// against the System's declared [`Access`].
pub struct SystemCtx<'a> {
    world: &'a mut World,
    access: Access,
    /// The current slot/tick - the System's only notion of time. Time is an
    /// explicit input, never read from a clock, so runs stay deterministic.
    pub slot: u64,
}

impl<'a> SystemCtx<'a> {
    pub fn new(world: &'a mut World, access: Access, slot: u64) -> Self {
        Self {
            world,
            access,
            slot,
        }
    }

    pub fn world_id(&self) -> WorldId {
        self.world.id
    }

    fn guard_read(&self, c: ComponentId) -> Result<(), EngineError> {
        if !self.access.can_read(c) {
            return Err(EngineError::AccessDenied {
                component: c,
                write: false,
            });
        }
        Ok(())
    }

    fn guard_write(&self, c: ComponentId) -> Result<(), EngineError> {
        if !self.access.can_write(c) {
            return Err(EngineError::AccessDenied {
                component: c,
                write: true,
            });
        }
        Ok(())
    }

    /// Does `entity` have Component `c`? (Counts as a read.)
    pub fn has(&self, c: ComponentId, entity: EntityId) -> Result<bool, EngineError> {
        self.guard_read(c)?;
        Ok(self.world.has(c, entity))
    }

    /// Spawn a fresh entity (e.g. a production System minting a new unit).
    pub fn spawn(&mut self) -> EntityId {
        self.world.spawn_entity()
    }

    // --- typed reads -----------------------------------------------------------------
    //
    // Read a single field from an entity's Component. A missing record reads as
    // zeroed, so Systems need no "does it exist yet" branch for fresh entities.

    fn read_field<T>(
        &self,
        c: ComponentId,
        entity: EntityId,
        f: impl FnOnce(&Record<'_>) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        self.guard_read(c)?;
        let schema = self.world.schema(c)?;
        let rec = match self.world.get(c, entity) {
            Some(bytes) => Record::from_bytes(schema, bytes.to_vec())?,
            None => Record::zeroed(schema),
        };
        f(&rec)
    }

    pub fn read_u8(&self, c: ComponentId, e: EntityId, field: &str) -> Result<u8, EngineError> {
        self.read_field(c, e, |r| r.get_u8(field))
    }
    pub fn read_u32(&self, c: ComponentId, e: EntityId, field: &str) -> Result<u32, EngineError> {
        self.read_field(c, e, |r| r.get_u32(field))
    }
    pub fn read_u64(&self, c: ComponentId, e: EntityId, field: &str) -> Result<u64, EngineError> {
        self.read_field(c, e, |r| r.get_u64(field))
    }
    pub fn read_i64(&self, c: ComponentId, e: EntityId, field: &str) -> Result<i64, EngineError> {
        self.read_field(c, e, |r| r.get_i64(field))
    }
    pub fn read_bool(&self, c: ComponentId, e: EntityId, field: &str) -> Result<bool, EngineError> {
        self.read_field(c, e, |r| r.get_bool(field))
    }
    pub fn read_entity(
        &self,
        c: ComponentId,
        e: EntityId,
        field: &str,
    ) -> Result<EntityId, EngineError> {
        self.read_field(c, e, |r| r.get_entity(field))
    }
    pub fn read_bytes(
        &self,
        c: ComponentId,
        e: EntityId,
        field: &str,
    ) -> Result<Vec<u8>, EngineError> {
        self.read_field(c, e, |r| r.get_bytes(field))
    }

    /// Read-modify-write a Component record under one access check. The closure
    /// receives a typed [`Record`] (zeroed if the entity did not have the
    /// Component yet); whatever it leaves is stored back.
    pub fn mutate<F>(&mut self, c: ComponentId, entity: EntityId, f: F) -> Result<(), EngineError>
    where
        F: FnOnce(&mut Record<'_>) -> Result<(), EngineError>,
    {
        self.guard_write(c)?;
        let bytes = {
            let schema = self.world.schema(c)?;
            let mut rec = match self.world.get(c, entity) {
                Some(b) => Record::from_bytes(schema, b.to_vec())?,
                None => Record::zeroed(schema),
            };
            f(&mut rec)?;
            rec.into_bytes()
        };
        self.world.set(c, entity, bytes)
    }
}
