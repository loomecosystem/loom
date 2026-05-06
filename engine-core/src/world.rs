//! One ECS state space plus its schema registry and governance flags.
//!
//! State is a single ordered map keyed by `(component_id, entity_id)`. The
//! ordering matters: it gives the tick coordinator a stable iteration order for
//! resumable cursors ([`crate::tick`]) and gives [`World::state_hash`] a canonical
//! encoding the TypeScript SDK reproduces byte-for-byte.

use crate::addressing::{component_address, Address};
use crate::component::Record;
use crate::error::EngineError;
use crate::hash::Hasher;
use crate::ids::{ComponentId, EntityId, WorldId, NULL_ENTITY};
use crate::schema::{ComponentSchema, Field, SchemaRegistry};
use std::collections::{BTreeMap, BTreeSet};

/// The full state of one world.
#[derive(Clone, Debug)]
pub struct World {
    pub id: WorldId,
    /// World admin (32-byte pubkey). Only the authority registers schemas or
    /// freezes the world; game-state mutation is governed per-System.
    pub authority: [u8; 32],
    /// Once frozen, schemas are locked permanently. Game state can still change
    /// via Systems; the schemas cannot.
    frozen: bool,

    schemas: SchemaRegistry,
    next_entity: EntityId,

    /// `(component_id, entity_id) -> encoded record`.
    store: BTreeMap<(ComponentId, EntityId), Vec<u8>>,
    /// Entities whose Components changed since the last [`World::clear_dirty`].
    /// The tick coordinator uses this to touch only changed entities.
    dirty: BTreeSet<(ComponentId, EntityId)>,
}

impl World {
    /// Create an empty world owned by `authority`.
    pub fn new(id: WorldId, authority: [u8; 32]) -> Self {
        Self {
            id,
            authority,
            frozen: false,
            schemas: SchemaRegistry::new(),
            next_entity: 1, // entity 0 is the null entity
            store: BTreeMap::new(),
            dirty: BTreeSet::new(),
        }
    }

    // --- governance ------------------------------------------------------------------

    /// Lock the schemas permanently. Idempotent.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    // --- schema ----------------------------------------------------------------------

    /// Register a Component schema, returning its id. Rejected if frozen.
    pub fn register_component(
        &mut self,
        name: impl Into<String>,
        fields: Vec<Field>,
    ) -> Result<ComponentId, EngineError> {
        if self.frozen {
            return Err(EngineError::WorldFrozen);
        }
        Ok(self.schemas.register(name, fields))
    }

    pub fn schema(&self, component: ComponentId) -> Result<&ComponentSchema, EngineError> {
        self.schemas.get(component)
    }

    pub fn schemas(&self) -> &SchemaRegistry {
        &self.schemas
    }

    // --- entities --------------------------------------------------------------------

    /// Allocate a fresh entity id. An entity is just an id; its data is whatever
    /// Components are attached to it.
    pub fn spawn_entity(&mut self) -> EntityId {
        let e = self.next_entity;
        self.next_entity += 1;
        e
    }

    /// Highest entity id that could exist (exclusive upper bound).
    pub fn entity_bound(&self) -> EntityId {
        self.next_entity
    }

    fn check_entity(&self, entity: EntityId) -> Result<(), EngineError> {
        if entity == NULL_ENTITY || entity >= self.next_entity {
            return Err(EngineError::UnknownEntity(entity));
        }
        Ok(())
    }

    // --- component data --------------------------------------------------------------

    /// A zeroed [`Record`] ready to be filled and [`World::set`]-stored.
    pub fn record(&self, component: ComponentId) -> Result<Record<'_>, EngineError> {
        Ok(Record::zeroed(self.schemas.get(component)?))
    }

    /// Store a Component record for an entity. Raw, no access control - Systems
    /// go through [`crate::system::SystemCtx`] instead. Marks the entity dirty.
    pub fn set(
        &mut self,
        component: ComponentId,
        entity: EntityId,
        bytes: Vec<u8>,
    ) -> Result<(), EngineError> {
        let schema = self.schemas.get(component)?;
        if bytes.len() != schema.size() {
            return Err(EngineError::BadRecordSize {
                component,
                expected: schema.size(),
                got: bytes.len(),
            });
        }
        self.check_entity(entity)?;
        self.store.insert((component, entity), bytes);
        self.dirty.insert((component, entity));
        Ok(())
    }

    /// Read a Component record's bytes for an entity, if present.
    pub fn get(&self, component: ComponentId, entity: EntityId) -> Option<&[u8]> {
        self.store.get(&(component, entity)).map(|v| v.as_slice())
    }

    /// Read and decode a Component into a typed [`Record`].
    pub fn read(
        &self,
        component: ComponentId,
        entity: EntityId,
    ) -> Result<Option<Record<'_>>, EngineError> {
        let schema = self.schemas.get(component)?;
        match self.store.get(&(component, entity)) {
            Some(bytes) => Ok(Some(Record::from_bytes(schema, bytes.clone())?)),
            None => Ok(None),
        }
    }

    pub fn has(&self, component: ComponentId, entity: EntityId) -> bool {
        self.store.contains_key(&(component, entity))
    }

    /// Remove a Component from an entity. Marks it dirty.
    pub fn remove(&mut self, component: ComponentId, entity: EntityId) -> bool {
        let existed = self.store.remove(&(component, entity)).is_some();
        if existed {
            self.dirty.insert((component, entity));
        }
        existed
    }

    /// The on-chain logical address of a Component record (see [`crate::addressing`]).
    pub fn address(&self, component: ComponentId, entity: EntityId) -> Address {
        component_address(self.id, entity, component)
    }

    // --- queries ---------------------------------------------------------------------

    /// Entities that have `component`, in ascending id order.
    pub fn entities_with(&self, component: ComponentId) -> impl Iterator<Item = EntityId> + '_ {
        self.store
            .range((component, EntityId::MIN)..=(component, EntityId::MAX))
            .map(|(&(_, e), _)| e)
    }

    /// Entities that have `component` with id `>= from`, in ascending order.
    /// Backs resumable cursors.
    pub fn entities_with_from(
        &self,
        component: ComponentId,
        from: EntityId,
    ) -> impl Iterator<Item = EntityId> + '_ {
        self.store
            .range((component, from)..=(component, EntityId::MAX))
            .map(|(&(_, e), _)| e)
    }

    /// How many entities have `component`.
    pub fn count_with(&self, component: ComponentId) -> usize {
        self.entities_with(component).count()
    }

    /// Total bytes of Component data stored. Storage rent ([`crate::economy`]) is
    /// charged against this.
    pub fn storage_bytes(&self) -> u64 {
        self.store.values().map(|b| b.len() as u64).sum()
    }

    // --- dirty set -------------------------------------------------------------------

    /// `(component, entity)` pairs changed since the last [`World::clear_dirty`].
    pub fn dirty(&self) -> impl Iterator<Item = (ComponentId, EntityId)> + '_ {
        self.dirty.iter().copied()
    }

    pub fn dirty_len(&self) -> usize {
        self.dirty.len()
    }

    /// Forget the dirty set (called at the start of a tick epoch).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    // --- determinism -----------------------------------------------------------------

    /// Canonical FNV-1a digest of the whole world: id, frozen flag, schema
    /// layouts (id order), then every record in `(component, entity)` order. The
    /// TypeScript SDK computes the identical value from the same logical state.
    pub fn state_hash(&self) -> u64 {
        let mut h = Hasher::new();
        h.write(b"loom:world").write_u64(self.id);
        h.write_u8(self.frozen as u8);
        for s in self.schemas.iter() {
            h.write_u32(s.id).write_u64(s.layout_hash());
        }
        for (&(component, entity), bytes) in &self.store {
            h.write_u32(component)
                .write_u64(entity)
                .write_u32(bytes.len() as u32)
                .write(bytes);
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::FieldType;

    fn pos_schema(w: &mut World) -> ComponentId {
        w.register_component(
            "Position",
            vec![
                Field::new("x", FieldType::I64),
                Field::new("y", FieldType::I64),
            ],
        )
        .unwrap()
    }

    #[test]
    fn spawn_set_get() {
        let mut w = World::new(1, [0u8; 32]);
        let pos = pos_schema(&mut w);
        let e = w.spawn_entity();
        let mut r = w.record(pos).unwrap();
        r.set_i64("x", 3).unwrap().set_i64("y", 4).unwrap();
        w.set(pos, e, r.into_bytes()).unwrap();

        let read = w.read(pos, e).unwrap().unwrap();
        assert_eq!(read.get_i64("x").unwrap(), 3);
        assert_eq!(read.get_i64("y").unwrap(), 4);
    }

    #[test]
    fn set_unknown_entity_rejected() {
        let mut w = World::new(1, [0u8; 32]);
        let pos = pos_schema(&mut w);
        let bytes = w.record(pos).unwrap().into_bytes();
        assert!(matches!(
            w.set(pos, 999, bytes),
            Err(EngineError::UnknownEntity(999))
        ));
    }

    #[test]
    fn frozen_blocks_schema_changes() {
        let mut w = World::new(1, [0u8; 32]);
        w.freeze();
        assert!(matches!(
            w.register_component("X", vec![]),
            Err(EngineError::WorldFrozen)
        ));
    }

    #[test]
    fn entities_with_is_ordered() {
        let mut w = World::new(1, [0u8; 32]);
        let pos = pos_schema(&mut w);
        let mut ids = vec![];
        for _ in 0..5 {
            let e = w.spawn_entity();
            ids.push(e);
            let bytes = w.record(pos).unwrap().into_bytes();
            w.set(pos, e, bytes).unwrap();
        }
        let got: Vec<_> = w.entities_with(pos).collect();
        assert_eq!(got, ids);
        let from3: Vec<_> = w.entities_with_from(pos, ids[2]).collect();
        assert_eq!(from3, ids[2..].to_vec());
    }

    #[test]
    fn dirty_tracks_changes() {
        let mut w = World::new(1, [0u8; 32]);
        let pos = pos_schema(&mut w);
        let e = w.spawn_entity();
        w.set(pos, e, w.record(pos).unwrap().into_bytes()).unwrap();
        assert_eq!(w.dirty_len(), 1);
        w.clear_dirty();
        assert_eq!(w.dirty_len(), 0);
    }

    #[test]
    fn state_hash_changes_with_state() {
        let mut w = World::new(1, [0u8; 32]);
        let pos = pos_schema(&mut w);
        let h0 = w.state_hash();
        let e = w.spawn_entity();
        w.set(pos, e, w.record(pos).unwrap().into_bytes()).unwrap();
        assert_ne!(h0, w.state_hash());
    }
}
