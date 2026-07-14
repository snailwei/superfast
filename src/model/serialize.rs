//! Serialize any `serde::Serialize` value into a `ValueData` tree.
//!
//! Used by the encoder to convert Rust structs into the intermediate
//! representation before binary encoding.

use serde::ser::{self, Serialize};
use std::rc::Rc;

use super::template::TemplateData;
use super::value::ValueData;
use crate::decimal::Decimal;
use crate::errors::{Error, Result};
use crate::value::Value;

/// Convert a `serde::Serialize` value into a `TemplateData` (enum variant + ValueData tree).
///
/// The template name is extracted from `#[serde(rename = "...")]` on the struct or enum variant.
/// Returns an error if no rename attribute is present — the struct must declare its template name.
pub fn to_template_data<T: Serialize>(value: &T) -> Result<TemplateData> {
    let mut serializer = ValueDataSerializer::default();
    value.serialize(&mut serializer)?;

    // For struct serialization, the struct name comes from #[serde(rename = "...")]
    // captured by serialize_struct into pending_name.
    if let Some(sname) = serializer.pending_name.take() {
        return Ok(TemplateData {
            name: sname,
            value: serializer.value,
            pmap_bytes: None,
        });
    }

    // For enum serialization via tuple variants, the pending_key holds the variant name.
    // Also unwrap Sequence([Group]) → Group for enum tuple variants.
    if let Some(variant) = serializer.pending_key.take() {
        let unwrapped = match serializer.value {
            ValueData::Sequence(items) if items.len() == 1 => items.into_iter().next().unwrap(),
            v => v,
        };
        return Ok(TemplateData {
            name: variant,
            value: unwrapped,
            pmap_bytes: None,
        });
    }

    // For newtype variant enums (Message::Variant(SingleValue)), serialize_newtype_variant
    // wraps the inner value in a DynamicTemplateRef carrying the variant name.
    if let ValueData::DynamicTemplateRef(td) = &serializer.value {
        return Ok(TemplateData {
            name: td.name.clone(),
            value: td.value.clone(),
            pmap_bytes: None,
        });
    }

    Err(Error::Runtime(
        "cannot determine template name: struct must have #[serde(rename = \"<template_name>\")]"
            .to_string(),
    ))
}

struct ValueDataSerializer {
    value: ValueData,
    /// Temporary storage for map keys
    pending_key: Option<String>,
    /// Struct name from serialize_struct (#[serde(rename = "...")])
    pending_name: Option<String>,
}

impl Default for ValueDataSerializer {
    fn default() -> Self {
        Self {
            value: ValueData::None,
            pending_key: None,
            pending_name: None,
        }
    }
}

impl<'de> ser::Serializer for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<()> {
        self.value = ValueData::Value(Some(Value::UInt32(if v { 1 } else { 0 })));
        Ok(())
    }
    fn serialize_i8(self, v: i8) -> Result<()> {
        self.serialize_i32(i32::from(v))
    }
    fn serialize_i16(self, v: i16) -> Result<()> {
        self.serialize_i32(i32::from(v))
    }
    fn serialize_i32(self, v: i32) -> Result<()> {
        self.value = ValueData::Value(Some(Value::Int32(v)));
        Ok(())
    }
    fn serialize_i64(self, v: i64) -> Result<()> {
        self.value = ValueData::Value(Some(Value::Int64(v)));
        Ok(())
    }
    fn serialize_u8(self, v: u8) -> Result<()> {
        self.serialize_u32(u32::from(v))
    }
    fn serialize_u16(self, v: u16) -> Result<()> {
        self.serialize_u32(u32::from(v))
    }
    fn serialize_u32(self, v: u32) -> Result<()> {
        self.value = ValueData::Value(Some(Value::UInt32(v)));
        Ok(())
    }
    fn serialize_u64(self, v: u64) -> Result<()> {
        self.value = ValueData::Value(Some(Value::UInt64(v)));
        Ok(())
    }
    fn serialize_f32(self, _v: f32) -> Result<()> {
        Err(Error::Runtime(
            "f32/f64 is not supported: use Decimal for FAST decimal fields".to_string(),
        ))
    }
    fn serialize_f64(self, _v: f64) -> Result<()> {
        Err(Error::Runtime(
            "f32/f64 is not supported: use Decimal for FAST decimal fields".to_string(),
        ))
    }
    fn serialize_char(self, v: char) -> Result<()> {
        self.serialize_str(&v.to_string())
    }
    fn serialize_str(self, v: &str) -> Result<()> {
        self.value = ValueData::Value(Some(Value::AsciiString(v.to_string())));
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        self.value = ValueData::Value(Some(Value::Bytes(v.to_vec())));
        Ok(())
    }
    fn serialize_none(self) -> Result<()> {
        self.value = ValueData::None;
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<()> {
        value.serialize(self)?;
        Ok(())
    }
    fn serialize_unit(self) -> Result<()> {
        self.value = ValueData::None;
        Ok(())
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
    ) -> Result<()> {
        self.value = ValueData::Value(Some(Value::AsciiString(variant.to_string())));
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        let mut inner = ValueDataSerializer::default();
        value.serialize(&mut inner)?;
        let td = TemplateData {
            name: variant.to_string(),
            value: inner.value,
            pmap_bytes: None,
        };
        self.value = ValueData::DynamicTemplateRef(Box::new(td));
        Ok(())
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.value = ValueData::Sequence(Vec::new());
        Ok(self)
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        self.value = ValueData::Sequence(Vec::new());
        Ok(self)
    }
    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        self.value = ValueData::Sequence(Vec::new());
        Ok(self)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        self.value = ValueData::Group(Vec::new());
        Ok(self)
    }
    fn serialize_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        self.value = ValueData::Group(Vec::new());
        self.pending_name = Some(name.to_string());
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _idx: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.pending_key = Some(variant.to_string());
        self.value = ValueData::Sequence(Vec::new());
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _idx: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.value = ValueData::Group(Vec::new());
        Ok(self)
    }
}

// --- Seq, Tuple, TupleStruct, TupleVariant ---

impl<'de> ser::SerializeSeq for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let mut inner = ValueDataSerializer::default();
        value.serialize(&mut inner)?;
        if let ValueData::Sequence(s) = &mut self.value {
            s.push(inner.value);
        }
        Ok(())
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<'de> ser::SerializeTuple for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

impl<'de> ser::SerializeTupleStruct for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

impl<'de> ser::SerializeTupleVariant for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

// --- Map ---

impl<'de> ser::SerializeMap for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<()> {
        let mut inner = ValueDataSerializer::default();
        key.serialize(&mut inner)?;
        self.pending_key = match inner.value {
            ValueData::Value(Some(Value::AsciiString(s))) => Some(s),
            ValueData::Value(Some(Value::UnicodeString(s))) => Some(s),
            other => Some(format!("{:?}", other)),
        };
        Ok(())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        let key = self.pending_key.take().unwrap_or_default();
        let mut inner = ValueDataSerializer::default();
        value.serialize(&mut inner)?;
        if let ValueData::Group(g) = &mut self.value {
            g.push((Rc::from(key), inner.value));
        }
        Ok(())
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

// --- Struct ---

impl<'de> ser::SerializeStruct for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        let mut inner = ValueDataSerializer::default();
        value.serialize(&mut inner)?;

        // Special handling for Option<Decimal> serialization:
        // Decimal derives Serialize which produces a struct with exponent+mantissa.
        // When we see an Option with Some containing a Decimal struct, convert to Value::Decimal.
        if let ValueData::Value(Some(ref v)) = inner.value {
            if matches!(v, Value::Int32(_) | Value::Int64(_)) {
                // This is fine, keep as is
            }
        }

        if let ValueData::Group(g) = &mut self.value {
            g.push((Rc::from(key), inner.value));
        }
        Ok(())
    }

    fn end(self) -> Result<()> {
        // After collecting all fields, check if this is a Decimal struct
        // (exponent: i32, mantissa: i64) and convert to Value::Decimal
        if let ValueData::Group(ref g) = self.value {
            let exp = g.iter().find(|(k, _)| k.as_ref() == "exponent").map(|(_, v)| v);
            let mant = g.iter().find(|(k, _) | k.as_ref() == "mantissa").map(|(_, v)| v);
            if let (
                Some(ValueData::Value(Some(Value::Int32(exp)))),
                Some(ValueData::Value(Some(Value::Int64(mant)))),
            ) = (exp.cloned(), mant.cloned())
            {
                let d = Decimal::new(exp, mant);
                self.value = ValueData::Value(Some(Value::Decimal(d)));
                return Ok(());
            }
        }
        Ok(())
    }
}

impl<'de> ser::SerializeStructVariant for &'de mut ValueDataSerializer {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }
    fn end(self) -> Result<()> {
        ser::SerializeStruct::end(self)
    }
}
