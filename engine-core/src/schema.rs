//! The on-chain schema registry.
//!
//! Component layouts are registered on-chain, which gives the engine reflection
//! (read any Component's bytes given its schema) and enables cross-world
//! composability: world B can reference a Component defined in world A because both
//! speak the same schema.
//!
//! v1 schemas are fixed-size scalar records. A constant size per Component keeps
//! account rent, the compute-budget math in [`crate::tick`], and the encode/decode
//! in [`crate::component`] deterministic. Dynamic/variable fields are a follow-up.

use crate::error::EngineError;
use crate::ids::ComponentId;
use std::collections::BTreeMap;

/// The scalar field types a Component may contain. Each has a fixed byte width and
/// a fixed little-endian encoding shared with the TypeScript SDK.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FieldType {
    U8,
    U16,
    U32,
    U64,
    I64,
    /// A boolean stored as a single `0`/`1` byte.
    Bool,
    /// A 32-byte Solana public key.
    Pubkey,
    /// An [`crate::ids::EntityId`] reference (u64). Distinguished from `U64` so the
    /// indexer and cross-world references know it points at another entity.
    Entity,
    /// A fixed-length byte blob of `n` bytes - for packed data a System consumes
    /// wholesale (e.g. a settled pathfinding route, an inventory bitmap).
    Bytes(u16),
}

impl FieldType {
    /// Fixed encoded width in bytes.
    pub const fn size(self) -> usize {
        match self {
            FieldType::U8 | FieldType::Bool => 1,
            FieldType::U16 => 2,
            FieldType::U32 => 4,
            FieldType::U64 | FieldType::I64 | FieldType::Entity => 8,
            FieldType::Pubkey => 32,
            FieldType::Bytes(n) => n as usize,
        }
    }

    /// Stable wire discriminant, shared with the TypeScript SDK. Not the same as
    /// an `as u8` cast: it must never shift as variants are added.
    pub const fn tag(self) -> u8 {
        match self {
            FieldType::U8 => 1,
            FieldType::U16 => 2,
            FieldType::U32 => 3,
            FieldType::U64 => 4,
            FieldType::I64 => 5,
            FieldType::Bool => 6,
            FieldType::Pubkey => 7,
            FieldType::Entity => 8,
            FieldType::Bytes(_) => 9,
        }
    }
}

/// One named field within a Component schema.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Field {
    pub name: String,
    pub ty: FieldType,
}

impl Field {
    pub fn new(name: impl Into<String>, ty: FieldType) -> Self {
        Self {
            name: name.into(),
            ty,
        }
    }
}

/// A registered Component layout.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ComponentSchema {
    pub id: ComponentId,
    pub name: String,
    pub fields: Vec<Field>,
}

impl ComponentSchema {
    pub fn new(id: ComponentId, name: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            id,
            name: name.into(),
            fields,
        }
    }

    /// Total fixed size of one record of this schema.
    pub fn size(&self) -> usize {
        self.fields.iter().map(|f| f.ty.size()).sum()
    }

    /// Byte offset and type of a field by name, if present.
    pub fn field(&self, name: &str) -> Option<(usize, FieldType)> {
        let mut offset = 0usize;
        for f in &self.fields {
            if f.name == name {
                return Some((offset, f.ty));
            }
            offset += f.ty.size();
        }
        None
    }

    /// A stable digest of the layout. Two schemas with the same digest are
    /// byte-compatible, and thus cross-world referenceable.
    pub fn layout_hash(&self) -> u64 {
        let mut h = crate::hash::Hasher::new();
        h.write(b"loom:schema").write_u32(self.id);
        h.write(self.name.as_bytes());
        for f in &self.fields {
            h.write_u8(0xff); // field separator
            h.write(f.name.as_bytes());
            h.write_u8(f.ty.tag());
            if let FieldType::Bytes(n) = f.ty {
                h.write(&n.to_le_bytes());
            }
        }
        h.finish()
    }
}

/// A world's registry of Component schemas, keyed by id and ordered for
/// deterministic iteration and hashing.
#[derive(Clone, Default, Debug)]
pub struct SchemaRegistry {
    schemas: BTreeMap<ComponentId, ComponentSchema>,
    next_id: ComponentId,
}

impl SchemaRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a schema under the next free id and return that id.
    pub fn register(&mut self, name: impl Into<String>, fields: Vec<Field>) -> ComponentId {
        let id = self.next_id;
        self.next_id += 1;
        self.schemas
            .insert(id, ComponentSchema::new(id, name, fields));
        id
    }

    /// Register a schema with an explicit id (used when replaying on-chain state).
    pub fn register_with_id(
        &mut self,
        id: ComponentId,
        name: impl Into<String>,
        fields: Vec<Field>,
    ) -> Result<(), EngineError> {
        if self.schemas.contains_key(&id) {
            return Err(EngineError::DuplicateComponent(id));
        }
        self.schemas
            .insert(id, ComponentSchema::new(id, name, fields));
        self.next_id = self.next_id.max(id + 1);
        Ok(())
    }

    pub fn get(&self, id: ComponentId) -> Result<&ComponentSchema, EngineError> {
        self.schemas
            .get(&id)
            .ok_or(EngineError::UnknownComponent(id))
    }

    pub fn contains(&self, id: ComponentId) -> bool {
        self.schemas.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// Iterate schemas in ascending id order.
    pub fn iter(&self) -> impl Iterator<Item = &ComponentSchema> {
        self.schemas.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position() -> Vec<Field> {
        vec![
            Field::new("x", FieldType::I64),
            Field::new("y", FieldType::I64),
        ]
    }

    #[test]
    fn offsets_and_size() {
        let s = ComponentSchema::new(0, "Position", position());
        assert_eq!(s.size(), 16);
        assert_eq!(s.field("x"), Some((0, FieldType::I64)));
        assert_eq!(s.field("y"), Some((8, FieldType::I64)));
        assert_eq!(s.field("z"), None);
    }

    #[test]
    fn registry_allocates_ids() {
        let mut r = SchemaRegistry::new();
        let pos = r.register("Position", position());
        let vel = r.register("Velocity", position());
        assert_eq!(pos, 0);
        assert_eq!(vel, 1);
        assert_eq!(r.len(), 2);
        assert_eq!(r.get(pos).unwrap().name, "Position");
    }

    #[test]
    fn layout_hash_is_structural() {
        let a = ComponentSchema::new(0, "Position", position());
        let b = ComponentSchema::new(0, "Position", position());
        let c = ComponentSchema::new(
            0,
            "Position",
            vec![Field::new("x", FieldType::I64), Field::new("y", FieldType::U64)],
        );
        assert_eq!(a.layout_hash(), b.layout_hash());
        assert_ne!(a.layout_hash(), c.layout_hash());
    }
}
