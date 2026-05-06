//! Reading and writing typed Component records over raw bytes.
//!
//! On-chain, a Component value is the data region of its account: a flat buffer of
//! exactly `schema.size()` bytes. [`Record`] is a typed view over such a buffer,
//! validating field names/types against a [`ComponentSchema`] using the same
//! little-endian encoding as the TypeScript SDK.

use crate::error::EngineError;
use crate::ids::EntityId;
use crate::schema::{ComponentSchema, FieldType};

/// A typed, mutable view over one Component record's bytes.
#[derive(Clone, Debug)]
pub struct Record<'a> {
    schema: &'a ComponentSchema,
    bytes: Vec<u8>,
}

impl<'a> Record<'a> {
    /// A new zeroed record sized for `schema`.
    pub fn zeroed(schema: &'a ComponentSchema) -> Self {
        Self {
            schema,
            bytes: vec![0u8; schema.size()],
        }
    }

    /// Wrap existing bytes, validating the length against the schema.
    pub fn from_bytes(schema: &'a ComponentSchema, bytes: Vec<u8>) -> Result<Self, EngineError> {
        if bytes.len() != schema.size() {
            return Err(EngineError::BadRecordSize {
                component: schema.id,
                expected: schema.size(),
                got: bytes.len(),
            });
        }
        Ok(Self { schema, bytes })
    }

    /// Consume the record, returning the underlying bytes (what gets stored).
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn slot(&self, name: &str, want: FieldType) -> Result<usize, EngineError> {
        let (offset, ty) = self
            .schema
            .field(name)
            .ok_or_else(|| EngineError::UnknownField(name.to_string()))?;
        if ty != want {
            return Err(EngineError::FieldTypeMismatch {
                field: name.to_string(),
            });
        }
        Ok(offset)
    }

    /// Offset and length of a `Bytes(n)` field.
    fn bytes_slot(&self, name: &str) -> Result<(usize, usize), EngineError> {
        let (offset, ty) = self
            .schema
            .field(name)
            .ok_or_else(|| EngineError::UnknownField(name.to_string()))?;
        match ty {
            FieldType::Bytes(n) => Ok((offset, n as usize)),
            _ => Err(EngineError::FieldTypeMismatch {
                field: name.to_string(),
            }),
        }
    }

    // --- unsigned / signed integers -------------------------------------------------

    pub fn set_u8(&mut self, name: &str, v: u8) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::U8)?;
        self.bytes[o] = v;
        Ok(self)
    }

    pub fn get_u8(&self, name: &str) -> Result<u8, EngineError> {
        let o = self.slot(name, FieldType::U8)?;
        Ok(self.bytes[o])
    }

    pub fn set_u16(&mut self, name: &str, v: u16) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::U16)?;
        self.bytes[o..o + 2].copy_from_slice(&v.to_le_bytes());
        Ok(self)
    }

    pub fn get_u16(&self, name: &str) -> Result<u16, EngineError> {
        let o = self.slot(name, FieldType::U16)?;
        Ok(u16::from_le_bytes(self.bytes[o..o + 2].try_into().unwrap()))
    }

    pub fn set_u32(&mut self, name: &str, v: u32) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::U32)?;
        self.bytes[o..o + 4].copy_from_slice(&v.to_le_bytes());
        Ok(self)
    }

    pub fn get_u32(&self, name: &str) -> Result<u32, EngineError> {
        let o = self.slot(name, FieldType::U32)?;
        Ok(u32::from_le_bytes(self.bytes[o..o + 4].try_into().unwrap()))
    }

    pub fn set_u64(&mut self, name: &str, v: u64) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::U64)?;
        self.bytes[o..o + 8].copy_from_slice(&v.to_le_bytes());
        Ok(self)
    }

    pub fn get_u64(&self, name: &str) -> Result<u64, EngineError> {
        let o = self.slot(name, FieldType::U64)?;
        Ok(u64::from_le_bytes(self.bytes[o..o + 8].try_into().unwrap()))
    }

    pub fn set_i64(&mut self, name: &str, v: i64) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::I64)?;
        self.bytes[o..o + 8].copy_from_slice(&v.to_le_bytes());
        Ok(self)
    }

    pub fn get_i64(&self, name: &str) -> Result<i64, EngineError> {
        let o = self.slot(name, FieldType::I64)?;
        Ok(i64::from_le_bytes(self.bytes[o..o + 8].try_into().unwrap()))
    }

    // --- bool ------------------------------------------------------------------------

    pub fn set_bool(&mut self, name: &str, v: bool) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::Bool)?;
        self.bytes[o] = v as u8;
        Ok(self)
    }

    pub fn get_bool(&self, name: &str) -> Result<bool, EngineError> {
        let o = self.slot(name, FieldType::Bool)?;
        Ok(self.bytes[o] != 0)
    }

    // --- pubkey ----------------------------------------------------------------------

    pub fn set_pubkey(&mut self, name: &str, v: &[u8; 32]) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::Pubkey)?;
        self.bytes[o..o + 32].copy_from_slice(v);
        Ok(self)
    }

    pub fn get_pubkey(&self, name: &str) -> Result<[u8; 32], EngineError> {
        let o = self.slot(name, FieldType::Pubkey)?;
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.bytes[o..o + 32]);
        Ok(out)
    }

    // --- entity reference ------------------------------------------------------------

    pub fn set_entity(&mut self, name: &str, v: EntityId) -> Result<&mut Self, EngineError> {
        let o = self.slot(name, FieldType::Entity)?;
        self.bytes[o..o + 8].copy_from_slice(&v.to_le_bytes());
        Ok(self)
    }

    pub fn get_entity(&self, name: &str) -> Result<EntityId, EngineError> {
        let o = self.slot(name, FieldType::Entity)?;
        Ok(EntityId::from_le_bytes(
            self.bytes[o..o + 8].try_into().unwrap(),
        ))
    }

    // --- fixed-length byte blob ------------------------------------------------------

    /// Write into a `Bytes(n)` field. `data` must be exactly `n` bytes.
    pub fn set_bytes(&mut self, name: &str, data: &[u8]) -> Result<&mut Self, EngineError> {
        let (o, n) = self.bytes_slot(name)?;
        if data.len() != n {
            return Err(EngineError::FieldTypeMismatch {
                field: name.to_string(),
            });
        }
        self.bytes[o..o + n].copy_from_slice(data);
        Ok(self)
    }

    /// Read a copy of a `Bytes(n)` field.
    pub fn get_bytes(&self, name: &str) -> Result<Vec<u8>, EngineError> {
        let (o, n) = self.bytes_slot(name)?;
        Ok(self.bytes[o..o + n].to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Field;

    fn unit_schema() -> ComponentSchema {
        ComponentSchema::new(
            3,
            "Unit",
            vec![
                Field::new("hp", FieldType::U32),
                Field::new("alive", FieldType::Bool),
                Field::new("target", FieldType::Entity),
                Field::new("dx", FieldType::I64),
            ],
        )
    }

    #[test]
    fn round_trip_all_types() {
        let s = unit_schema();
        let mut r = Record::zeroed(&s);
        r.set_u32("hp", 120)
            .unwrap()
            .set_bool("alive", true)
            .unwrap()
            .set_entity("target", 99)
            .unwrap()
            .set_i64("dx", -7)
            .unwrap();

        let bytes = r.as_bytes().to_vec();
        let r2 = Record::from_bytes(&s, bytes).unwrap();
        assert_eq!(r2.get_u32("hp").unwrap(), 120);
        assert!(r2.get_bool("alive").unwrap());
        assert_eq!(r2.get_entity("target").unwrap(), 99);
        assert_eq!(r2.get_i64("dx").unwrap(), -7);
    }

    #[test]
    fn type_mismatch_is_rejected() {
        let s = unit_schema();
        let mut r = Record::zeroed(&s);
        assert!(matches!(
            r.set_u64("hp", 1),
            Err(EngineError::FieldTypeMismatch { .. })
        ));
    }

    #[test]
    fn unknown_field_is_rejected() {
        let s = unit_schema();
        let r = Record::zeroed(&s);
        assert!(matches!(
            r.get_u32("nope"),
            Err(EngineError::UnknownField(_))
        ));
    }

    #[test]
    fn bad_size_is_rejected() {
        let s = unit_schema();
        assert!(matches!(
            Record::from_bytes(&s, vec![0u8; 3]),
            Err(EngineError::BadRecordSize { .. })
        ));
    }
}
