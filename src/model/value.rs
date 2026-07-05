//! ValueData — intermediate representation between FAST decode and serde deserialize.

use std::collections::HashMap;

use serde::de::{DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor};

use crate::errors::Error;
use crate::value::Value;

use super::template::TemplateData;

#[derive(Debug, PartialEq, Clone, Default)]
pub enum ValueData {
    #[default]
    None, // For optional groups and sequences
    Value(Option<Value>),
    Group(HashMap<String, ValueData>),
    Sequence(Vec<ValueData>),
    StaticTemplateRef(String, Box<ValueData>),
    DynamicTemplateRef(Box<TemplateData>),
}

impl<'de> serde::Deserializer<'de> for ValueData {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_none(),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(v) => match v {
                    Value::UInt32(n) => visitor.visit_u32(n),
                    Value::Int32(n) => visitor.visit_i32(n),
                    Value::UInt64(n) => visitor.visit_u64(n),
                    Value::Int64(n) => visitor.visit_i64(n),
                    Value::Decimal(f) => visitor.visit_f64(f.to_float()),
                    Value::AsciiString(s) | Value::UnicodeString(s) => visitor.visit_string(s),
                    Value::Bytes(b) => visitor.visit_byte_buf(b),
                },
            },
            ValueData::Group(_) => self.deserialize_map(visitor),
            ValueData::Sequence(_) => self.deserialize_seq(visitor),
            _ => Err(Error::Runtime(format!(
                "deserialize_any: data model unsupported type: {self:?}"
            ))),
        }
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_i32(0), // Absent from stream; inherited from context
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::Int32(n)) => visitor.visit_i32(n),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_i32: expected Value::Int32, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_i32: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_i64(0),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::Int64(n)) => visitor.visit_i64(n),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_i64: expected Value::Int64, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_i64: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_u32(0),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::UInt32(n)) => visitor.visit_u32(n),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_u32: expected Value::UInt32, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_u32: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_u64(0),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::UInt64(n)) => visitor.visit_u64(n),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_u64: expected Value::UInt64, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_u64: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_f64(0.0),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::Decimal(n)) => visitor.visit_f64(n.to_float()),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_f64: expected Value::Decimal, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_f64: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::AsciiString(s) | Value::UnicodeString(s)) => {
                    if s.len() == 1 {
                        visitor.visit_char(s.chars().next().unwrap())
                    } else {
                        Err(Error::Runtime(
                            "deserialize_char: string length must be 1".to_string(),
                        ))
                    }
                }
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_char: expected string, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_char: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_string(String::new()),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::AsciiString(s) | Value::UnicodeString(s)) => visitor.visit_string(s),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_string: expected string, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_string: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::None => visitor.visit_byte_buf(Vec::new()),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(Value::Bytes(b)) => visitor.visit_byte_buf(b),
                Some(v) => Err(Error::Runtime(format!(
                    "deserialize_byte_buf: expected Value::Bytes, got: {v:?}"
                ))),
            },
            _ => Err(Error::Runtime(format!(
                "deserialize_byte_buf: expected ValueData::Value, got: {self:?}"
            ))),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match &self {
            ValueData::None => visitor.visit_none(),
            ValueData::Value(v) => match v {
                None => visitor.visit_none(),
                Some(_) => visitor.visit_some(self),
            },
            ValueData::Group(_) | ValueData::Sequence(_) => visitor.visit_some(self),
            _ => Err(Error::Runtime(format!(
                "deserialize_option: cannot be optional, got: {self:?}"
            ))),
        }
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::Sequence(q) => visitor.visit_seq(SequenceDeserializer::new(q)),
            _ => Err(Error::Runtime(format!(
                "deserialize_seq: expected ValueData::Sequence, got: {self:?}"
            ))),
        }
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if name == "Decimal" && len == 2 {
            return if let ValueData::Value(Some(Value::Decimal(d))) = self {
                visitor.visit_seq(d)
            } else {
                Err(Error::Runtime(format!(
                    "deserialize_tuple_struct: expected Value::Decimal, got: {self:?}"
                )))
            };
        }
        Err(Error::Runtime(format!(
            "deserialize_tuple_struct: unsupported data model {self:?}"
        )))
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::Group(group) => visitor.visit_map(GroupDeserializer::new(group)),
            _ => Err(Error::Runtime(format!(
                "deserialize_map: expected ValueData::Group, got: {self:?}"
            ))),
        }
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        match self {
            ValueData::DynamicTemplateRef(t) => t.deserialize_enum(name, variants, visitor),
            _ => Err(Error::Runtime(format!(
                "deserialize_enum: expected ValueData::DynamicTemplateRef, got: {self:?}"
            ))),
        }
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        drop(self);
        visitor.visit_unit()
    }

    // Forward unsupported types to a common error
    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("boolean is not supported".to_string()))
    }

    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("i8 is not supported".to_string()))
    }

    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("i16 is not supported".to_string()))
    }

    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("u8 is not supported".to_string()))
    }

    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("u16 is not supported".to_string()))
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("f32 is not supported".to_string()))
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("unit is not supported".to_string()))
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("unit_struct is not supported".to_string()))
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("newtype_struct is not supported".to_string()))
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("tuple is not supported".to_string()))
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("identifier is not supported".to_string()))
    }
}

struct GroupDeserializer {
    items: <HashMap<String, ValueData> as IntoIterator>::IntoIter,
    value: Option<ValueData>,
}

impl GroupDeserializer {
    fn new(values: HashMap<String, ValueData>) -> Self {
        Self {
            items: values.into_iter(),
            value: None,
        }
    }
}

impl<'de> MapAccess<'de> for GroupDeserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: DeserializeSeed<'de>,
    {
        match self.items.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: DeserializeSeed<'de>,
    {
        match self.value.take() {
            Some(value) => seed.deserialize(value),
            None => Err(Error::Runtime(
                "visit_value called before visit_key".to_string(),
            )),
        }
    }

    fn size_hint(&self) -> Option<usize> {
        match self.items.size_hint() {
            (lower, Some(upper)) if lower == upper => Some(upper),
            _ => None,
        }
    }
}

struct SequenceDeserializer {
    items: <Vec<ValueData> as IntoIterator>::IntoIter,
}

impl SequenceDeserializer {
    fn new(values: Vec<ValueData>) -> Self {
        Self {
            items: values.into_iter(),
        }
    }
}

impl<'de> SeqAccess<'de> for SequenceDeserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.items.next() {
            Some(value) => seed.deserialize(value).map(Some),
            None => Ok(None),
        }
    }
}
