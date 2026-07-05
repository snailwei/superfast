//! FAST value types and runtime values.

use crate::decimal::Decimal;
use crate::errors::Result;

/// Represents the type of a FAST field instruction.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ValueType {
    UInt32,
    Int32,
    UInt64,
    Int64,
    Length,
    Exponent,
    Mantissa,
    Decimal,
    AsciiString,
    UnicodeString,
    Bytes,
    Sequence,
    Group,
    TemplateReference,
}

impl ValueType {
    /// Parse from XML tag name.
    pub fn from_tag(tag: &str, unicode: bool) -> Result<Self> {
        match tag {
            "uInt32" => Ok(Self::UInt32),
            "int32" => Ok(Self::Int32),
            "uInt64" => Ok(Self::UInt64),
            "int64" => Ok(Self::Int64),
            "length" => Ok(Self::Length),
            "exponent" => Ok(Self::Exponent),
            "mantissa" => Ok(Self::Mantissa),
            "decimal" => Ok(Self::Decimal),
            "string" => {
                if unicode {
                    Ok(Self::UnicodeString)
                } else {
                    Ok(Self::AsciiString)
                }
            }
            "byteVector" => Ok(Self::Bytes),
            "sequence" => Ok(Self::Sequence),
            "group" => Ok(Self::Group),
            "templateRef" => Ok(Self::TemplateReference),
            _ => Err(crate::errors::Error::Static(format!("Unknown type: {tag}"))),
        }
    }

    /// Get a default value for this type (used by delta/tail when no previous value).
    pub fn default_value(&self) -> Result<Value> {
        match self {
            Self::Int32 | Self::Exponent => Ok(Value::Int32(0)),
            Self::Int64 | Self::Mantissa => Ok(Value::Int64(0)),
            Self::UInt32 | Self::Length => Ok(Value::UInt32(0)),
            Self::UInt64 => Ok(Value::UInt64(0)),
            Self::Decimal => Ok(Value::Decimal(Decimal::default())),
            Self::AsciiString => Ok(Value::AsciiString(String::new())),
            Self::UnicodeString => Ok(Value::UnicodeString(String::new())),
            Self::Bytes => Ok(Value::Bytes(Vec::new())),
            _ => Err(crate::errors::Error::Runtime(format!(
                "{:?} has no default value",
                self
            ))),
        }
    }

    /// Check if a Value matches this type.
    pub fn matches(&self, v: &Value) -> bool {
        match (self, v) {
            (Self::UInt32 | Self::Length, Value::UInt32(_)) => true,
            (Self::Int32 | Self::Exponent, Value::Int32(_)) => true,
            (Self::UInt64, Value::UInt64(_)) => true,
            (Self::Int64 | Self::Mantissa, Value::Int64(_)) => true,
            (Self::Decimal, Value::Decimal(_)) => true,
            (Self::AsciiString, Value::AsciiString(_)) => true,
            (Self::UnicodeString, Value::UnicodeString(_)) => true,
            (Self::Bytes, Value::Bytes(_)) => true,
            _ => false,
        }
    }

    /// Parse an initial value string into a Value of this type.
    pub fn parse_initial(&self, s: &str) -> Result<Value> {
        match self {
            Self::UInt32 | Self::Length => s
                .parse::<u32>()
                .map(Value::UInt32)
                .map_err(|e| crate::errors::Error::Static(format!("parse uInt32: {e}"))),
            Self::Int32 | Self::Exponent => s
                .parse::<i32>()
                .map(Value::Int32)
                .map_err(|e| crate::errors::Error::Static(format!("parse int32: {e}"))),
            Self::UInt64 => s
                .parse::<u64>()
                .map(Value::UInt64)
                .map_err(|e| crate::errors::Error::Static(format!("parse uInt64: {e}"))),
            Self::Int64 | Self::Mantissa => s
                .parse::<i64>()
                .map(Value::Int64)
                .map_err(|e| crate::errors::Error::Static(format!("parse int64: {e}"))),
            Self::AsciiString | Self::UnicodeString => Ok(Value::AsciiString(s.to_string())),
            Self::Bytes => {
                let hex: String = s.chars().filter(|c| !c.is_whitespace()).collect();
                let mut bytes = Vec::with_capacity(hex.len() / 2);
                for i in (0..hex.len()).step_by(2) {
                    let b = u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| {
                        crate::errors::Error::Static(format!("parse hex byte: {e}"))
                    })?;
                    bytes.push(b);
                }
                Ok(Value::Bytes(bytes))
            }
            _ => Err(crate::errors::Error::Static(format!(
                "cannot parse initial value for {:?}",
                self
            ))),
        }
    }
}

/// A runtime FAST field value.
#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    UInt32(u32),
    Int32(i32),
    UInt64(u64),
    Int64(i64),
    Decimal(Decimal),
    AsciiString(String),
    UnicodeString(String),
    Bytes(Vec<u8>),
}

impl Value {
    /// Increment integer value by 1. Overflows wrap (max → min) per FAST §4.6.
    pub fn increment(self) -> Result<Self> {
        match self {
            Self::UInt32(v) => Ok(Self::UInt32(v.wrapping_add(1))),
            Self::Int32(v) => Ok(Self::Int32(v.wrapping_add(1))),
            Self::UInt64(v) => Ok(Self::UInt64(v.wrapping_add(1))),
            Self::Int64(v) => Ok(Self::Int64(v.wrapping_add(1))),
            _ => Err(crate::errors::Error::Runtime(format!(
                "cannot increment {:?}",
                self
            ))),
        }
    }

    /// Apply delta to produce new value.
    /// Reportable error [ERR R4] if combined value overflows the declared type.
    pub fn apply_delta(self, delta: &Self) -> Result<Self> {
        match (self, delta) {
            (Self::UInt32(base), Self::Int64(d)) => {
                if *d < 0 {
                    let sub = -*d as u32;
                    base.checked_sub(sub).map(Self::UInt32).ok_or_else(|| {
                        crate::errors::Error::Dynamic(
                            "integer delta underflow: result would be negative".to_string(),
                        )
                    })
                } else {
                    base.checked_add(*d as u32)
                        .map(Self::UInt32)
                        .ok_or_else(|| {
                            crate::errors::Error::Dynamic(
                                "integer delta overflow: result exceeds uInt32 range".to_string(),
                            )
                        })
                }
            }
            (Self::Int32(base), Self::Int64(d)) => {
                if *d > i64::from(i32::MAX) || *d < i64::from(i32::MIN) {
                    return Err(crate::errors::Error::Dynamic(
                        "integer delta overflow: delta exceeds int32 range".to_string(),
                    ));
                }
                base.checked_add(*d as i32).map(Self::Int32).ok_or_else(|| {
                    crate::errors::Error::Dynamic(
                        "integer delta overflow: result exceeds int32 range".to_string(),
                    )
                })
            }
            (Self::UInt64(base), Self::Int64(d)) => {
                if *d < 0 {
                    let sub = -*d as u64;
                    base.checked_sub(sub).map(Self::UInt64).ok_or_else(|| {
                        crate::errors::Error::Dynamic(
                            "integer delta underflow: result would be negative".to_string(),
                        )
                    })
                } else {
                    base.checked_add(*d as u64)
                        .map(Self::UInt64)
                        .ok_or_else(|| {
                            crate::errors::Error::Dynamic(
                                "integer delta overflow: result exceeds uInt64 range".to_string(),
                            )
                        })
                }
            }
            (Self::Int64(base), Self::Int64(d)) => {
                base.checked_add(*d).map(Self::Int64).ok_or_else(|| {
                    crate::errors::Error::Dynamic(
                        "integer delta overflow: result exceeds int64 range".to_string(),
                    )
                })
            }
            (Self::Decimal(base), Self::Int64(d)) => {
                // Decimal delta: the delta is on the mantissa only
                let mantissa = base.mantissa.checked_add(*d).ok_or_else(|| {
                    crate::errors::Error::Dynamic(
                        "decimal delta overflow: mantissa overflow".to_string(),
                    )
                })?;
                Ok(Self::Decimal(Decimal::new(base.exponent, mantissa)))
            }
            // String/byte delta: the new value is the reconstructed string
            // (read_delta already applies the subtraction length + new data).
            (Self::AsciiString(_base), Self::AsciiString(new)) => {
                Ok(Self::AsciiString(new.clone()))
            }
            (Self::UnicodeString(_base), Self::UnicodeString(new)) => {
                Ok(Self::UnicodeString(new.clone()))
            }
            (Self::Bytes(_base), Self::Bytes(new)) => Ok(Self::Bytes(new.clone())),
            _ => Err(crate::errors::Error::Runtime(format!(
                "cannot apply delta {:?}",
                delta
            ))),
        }
    }
}
