//! TemplateData — the root serde deserializer for a decoded FAST message.

use serde::de::{
    DeserializeSeed, EnumAccess, IntoDeserializer, Visitor, value::StringDeserializer,
};
use serde::forward_to_deserialize_any;

use super::value::ValueData;
use crate::errors::Error;

#[derive(Debug, PartialEq, Clone)]
pub struct TemplateData {
    pub name: String,
    pub value: ValueData, // Must be ValueData::Group
    /// Raw pmap bytes from the original message (for round-trip fidelity)
    pub pmap_bytes: Option<Vec<u8>>,
}

impl TemplateData {
    /// Get a field by name as a `&str`, returning `None` if absent or not a string.
    pub fn get_str(&self, field: &str) -> Option<&str> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(
                    crate::value::Value::AsciiString(s) | crate::value::Value::UnicodeString(s),
                )) = vd
                {
                    Some(s.as_str())
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a field by name as an `i32`, returning `None` if absent or not an i32.
    pub fn get_i32(&self, field: &str) -> Option<i32> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(crate::value::Value::Int32(n))) = vd {
                    Some(*n)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a field by name as an `i64`, returning `None` if absent or not an i64.
    pub fn get_i64(&self, field: &str) -> Option<i64> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(crate::value::Value::Int64(n))) = vd {
                    Some(*n)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a field by name as a `u32`, returning `None` if absent or not a u32.
    pub fn get_u32(&self, field: &str) -> Option<u32> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(crate::value::Value::UInt32(n))) = vd {
                    Some(*n)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a field by name as a `u64`, returning `None` if absent or not a u64.
    pub fn get_u64(&self, field: &str) -> Option<u64> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(crate::value::Value::UInt64(n))) = vd {
                    Some(*n)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }

    /// Get a field by name as a `&Decimal`, returning `None` if absent or not a decimal.
    pub fn get_decimal(&self, field: &str) -> Option<&crate::decimal::Decimal> {
        if let ValueData::Group(ref group) = self.value {
            group.get(field).and_then(|vd| {
                if let ValueData::Value(Some(crate::value::Value::Decimal(d))) = vd {
                    Some(d)
                } else {
                    None
                }
            })
        } else {
            None
        }
    }
}

impl<'de> serde::Deserializer<'de> for TemplateData {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("message must be enum".to_string()))
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct
        seq tuple tuple_struct map struct identifier ignored_any
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(EnumDeserializer {
            variant: self.name,
            value: self.value,
        })
    }
}

struct EnumDeserializer {
    variant: String,
    value: ValueData,
}

impl<'de> EnumAccess<'de> for EnumDeserializer {
    type Error = Error;
    type Variant = VariantDeserializer;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, VariantDeserializer), Error>
    where
        V: DeserializeSeed<'de>,
    {
        let variant: StringDeserializer<Error> = self.variant.into_deserializer();
        let visitor = VariantDeserializer { value: self.value };
        let value = seed.deserialize(variant)?;
        Ok((value, visitor))
    }
}

struct VariantDeserializer {
    value: ValueData,
}

impl<'de> serde::de::VariantAccess<'de> for VariantDeserializer {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Err(Error::Static("message body must be struct".to_string()))
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: DeserializeSeed<'de>,
    {
        match self.value {
            ValueData::Group(_) => seed.deserialize(self.value),
            _ => Err(Error::Runtime(
                "message data model must be ValueData::Group".to_string(),
            )),
        }
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("message body must be struct".to_string()))
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        Err(Error::Static("message body must be struct".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::value::Value;

    fn make_group(fields: &[(&str, ValueData)]) -> TemplateData {
        let mut group = HashMap::new();
        for (name, val) in fields {
            group.insert(name.to_string(), val.clone());
        }
        TemplateData {
            name: "Test".into(),
            value: ValueData::Group(group),
            pmap_bytes: None,
        }
    }

    // --- get_str ---

    #[test]
    fn get_str_present_ascii() {
        let td = make_group(&[(
            "Name",
            ValueData::Value(Some(Value::AsciiString("hello".into()))),
        )]);
        assert_eq!(td.get_str("Name"), Some("hello"));
    }

    #[test]
    fn get_str_present_unicode() {
        let td = make_group(&[(
            "Name",
            ValueData::Value(Some(Value::UnicodeString("世界".into()))),
        )]);
        assert_eq!(td.get_str("Name"), Some("世界"));
    }

    #[test]
    fn get_str_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::Int32(42))))]);
        assert_eq!(td.get_str("Name"), None);
    }

    #[test]
    fn get_str_wrong_type_returns_none() {
        let td = make_group(&[("Name", ValueData::Value(Some(Value::Int32(42))))]);
        assert_eq!(td.get_str("Name"), None);
    }

    #[test]
    fn get_str_none_value_returns_none() {
        let td = make_group(&[("Name", ValueData::Value(None))]);
        assert_eq!(td.get_str("Name"), None);
    }

    #[test]
    fn get_str_non_group_returns_none() {
        let td = TemplateData {
            name: "Test".into(),
            value: ValueData::None,
            pmap_bytes: None,
        };
        assert_eq!(td.get_str("anything"), None);
    }

    // --- get_i32 ---

    #[test]
    fn get_i32_present() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::Int32(-42))))]);
        assert_eq!(td.get_i32("Val"), Some(-42));
    }

    #[test]
    fn get_i32_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::Int32(0))))]);
        assert_eq!(td.get_i32("Val"), None);
    }

    #[test]
    fn get_i32_wrong_type_returns_none() {
        let td = make_group(&[(
            "Val",
            ValueData::Value(Some(Value::AsciiString("x".into()))),
        )]);
        assert_eq!(td.get_i32("Val"), None);
    }

    #[test]
    fn get_i32_none_value_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(None))]);
        assert_eq!(td.get_i32("Val"), None);
    }

    // --- get_i64 ---

    #[test]
    fn get_i64_present() {
        let td = make_group(&[(
            "Val",
            ValueData::Value(Some(Value::Int64(9_223_372_036_854_775_807))),
        )]);
        assert_eq!(td.get_i64("Val"), Some(i64::MAX));
    }

    #[test]
    fn get_i64_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::Int64(0))))]);
        assert_eq!(td.get_i64("Val"), None);
    }

    #[test]
    fn get_i64_wrong_type_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::Int32(42))))]);
        assert_eq!(td.get_i64("Val"), None);
    }

    #[test]
    fn get_i64_none_value_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(None))]);
        assert_eq!(td.get_i64("Val"), None);
    }

    // --- get_u32 ---

    #[test]
    fn get_u32_present() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::UInt32(42))))]);
        assert_eq!(td.get_u32("Val"), Some(42));
    }

    #[test]
    fn get_u32_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::UInt32(0))))]);
        assert_eq!(td.get_u32("Val"), None);
    }

    #[test]
    fn get_u32_wrong_type_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::Int32(42))))]);
        assert_eq!(td.get_u32("Val"), None);
    }

    #[test]
    fn get_u32_none_value_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(None))]);
        assert_eq!(td.get_u32("Val"), None);
    }

    // --- get_u64 ---

    #[test]
    fn get_u64_present() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::UInt64(u64::MAX))))]);
        assert_eq!(td.get_u64("Val"), Some(u64::MAX));
    }

    #[test]
    fn get_u64_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::UInt64(0))))]);
        assert_eq!(td.get_u64("Val"), None);
    }

    #[test]
    fn get_u64_wrong_type_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::Int64(42))))]);
        assert_eq!(td.get_u64("Val"), None);
    }

    #[test]
    fn get_u64_none_value_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(None))]);
        assert_eq!(td.get_u64("Val"), None);
    }

    // --- get_decimal ---

    #[test]
    fn get_decimal_present() {
        use crate::decimal::Decimal;
        let td = make_group(&[(
            "Val",
            ValueData::Value(Some(Value::Decimal(Decimal::new(-2, 942755)))),
        )]);
        let d = td.get_decimal("Val").unwrap();
        assert_eq!(d.exponent, -2);
        assert_eq!(d.mantissa, 942755);
    }

    #[test]
    fn get_decimal_absent() {
        let td = make_group(&[("Other", ValueData::Value(Some(Value::Int64(0))))]);
        assert_eq!(td.get_decimal("Val"), None);
    }

    #[test]
    fn get_decimal_wrong_type_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(Some(Value::Int64(42))))]);
        assert_eq!(td.get_decimal("Val"), None);
    }

    #[test]
    fn get_decimal_none_value_returns_none() {
        let td = make_group(&[("Val", ValueData::Value(None))]);
        assert_eq!(td.get_decimal("Val"), None);
    }

    // --- integration: encode then decode_raw, then use accessors ---

    #[test]
    fn get_methods_via_decode_raw() {
        use crate::decimal::Decimal;
        use crate::{FastDecoder, FastEncoder};
        use serde::{Deserialize, Serialize};

        const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<templates version="2.26" xmlns="http://www.fixprotocol.org/ns/template-definition">
    <template name="Tick" id="1">
        <string name="SecurityID" id="48"/>
        <int32 name="Price" id="31"/>
        <int64 name="Qty" id="32"/>
        <uInt32 name="SeqNum" id="1" presence="optional"><increment value="0"/></uInt32>
        <uInt64 name="Timestamp" id="2" presence="optional"><copy value="0"/></uInt64>
        <decimal name="Turnover" id="3" presence="optional"><default value="0"/></decimal>
    </template>
</templates>"#;

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Tick {
            #[serde(rename = "SecurityID")]
            security_id: String,
            #[serde(rename = "Price")]
            price: i32,
            #[serde(rename = "Qty")]
            qty: i64,
            #[serde(rename = "SeqNum", default)]
            seq_num: Option<u32>,
            #[serde(rename = "Timestamp", default)]
            timestamp: Option<u64>,
            #[serde(rename = "Turnover", default)]
            turnover: Option<Decimal>,
        }

        let msg = Tick {
            security_id: "600519".into(),
            price: 42,
            qty: 999_999,
            seq_num: Some(1),
            timestamp: Some(1_700_000_000),
            turnover: Some(Decimal::new(-2, 942755)),
        };

        let mut enc = FastEncoder::new(XML).unwrap();
        let bytes = enc.encode(&msg).unwrap();

        let mut dec = FastDecoder::new(XML).unwrap();
        let (td, _consumed) = dec.decode_raw(&bytes).expect("decode raw");

        assert_eq!(td.name, "Tick");
        assert_eq!(td.get_str("SecurityID"), Some("600519"));
        assert_eq!(td.get_i32("Price"), Some(42));
        assert_eq!(td.get_i64("Qty"), Some(999_999));
        assert_eq!(td.get_u32("SeqNum"), Some(1));
        assert_eq!(td.get_u64("Timestamp"), Some(1_700_000_000));
        {
            let d = td.get_decimal("Turnover").unwrap();
            assert_eq!(d.exponent, -2);
            assert_eq!(d.mantissa, 942755);
        }
        assert_eq!(td.get_str("NonExistent"), None);
        assert_eq!(td.get_i32("NonExistent"), None);
        assert_eq!(td.get_i64("NonExistent"), None);
        assert_eq!(td.get_u32("NonExistent"), None);
        assert_eq!(td.get_u64("NonExistent"), None);
        assert_eq!(td.get_decimal("NonExistent"), None);
    }
}
