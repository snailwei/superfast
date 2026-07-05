//! FAST Decimal — scaled number with exponent and mantissa.

use crate::errors::{Error, Result};

/// Represents a scaled decimal number.
#[derive(Debug, PartialEq, Eq, Hash, Clone, serde::Serialize)]
pub struct Decimal {
    pub exponent: i32,
    pub mantissa: i64,
}

impl Decimal {
    #[must_use]
    pub fn new(exponent: i32, mantissa: i64) -> Self {
        Self { exponent, mantissa }
    }

    pub fn from_string(value: &str) -> Result<Self> {
        fn scale_down(mut value: i64) -> (i32, i64) {
            let mut scale = 0;
            if value != 0 {
                while value % 10 == 0 {
                    value /= 10;
                    scale += 1;
                }
            }
            (scale, value)
        }

        let mut parts = value.split('.');
        let Some(mantissa) = parts.next() else {
            return Err(Error::Static(format!("Not a decimal '{value}'")));
        };

        let Some(fractional) = parts.next() else {
            let (exponent, mantissa) = scale_down(mantissa.parse::<i64>()?);
            return Ok(Self::new(exponent, mantissa));
        };
        if parts.next().is_some() {
            return Err(Error::Static(format!("Not a decimal '{value}'")));
        }

        let mantissa = format!("{mantissa}{fractional}").parse::<i64>()?;
        if mantissa == 0 {
            return Ok(Self::new(0, 0));
        }
        let (exponent_fix, mantissa) = scale_down(mantissa);
        let exponent = -(fractional.len() as i32) + exponent_fix;
        Ok(Self::new(exponent, mantissa))
    }

    pub fn from_float(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(Error::Static(format!("Not a finite decimal '{value}'")));
        }
        Self::from_string(&format!("{value}"))
    }

    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn to_float(&self) -> f64 {
        match self.exponent {
            0 => self.mantissa as f64,
            exponent if exponent > 0 => {
                // Multiply in f64 to avoid i64 overflow when mantissa * 10^exponent > i64::MAX
                self.mantissa as f64 * 10f64.powi(exponent)
            }
            exponent => {
                let divisor = 10u64.pow(-exponent as u32);
                self.mantissa as f64 / divisor as f64
            }
        }
    }
}

impl Default for Decimal {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

impl From<Decimal> for f64 {
    fn from(value: Decimal) -> Self {
        value.to_float()
    }
}

impl std::fmt::Display for Decimal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.exponent >= 0 {
            // Use f64 arithmetic to avoid i64 overflow when exponent is large
            write!(f, "{}.0", self.to_float())
        } else {
            write!(f, "{:.*}", -self.exponent as usize, self.to_float())
        }
    }
}

// Implement SeqAccess so Decimal can be visited as a 2-element sequence (exponent, mantissa)
impl<'de> serde::de::SeqAccess<'de> for Decimal {
    type Error = Error;

    fn next_element_seed<T>(
        &mut self,
        seed: T,
    ) -> std::result::Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        seed.deserialize(&mut *self).map(Some)
    }
}

impl<'de> serde::de::Deserializer<'de> for &mut Decimal {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::Static(
            "Decimal: unsupported deserialize_any".to_string(),
        ))
    }

    fn deserialize_i32<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i32(self.exponent)
    }

    fn deserialize_i64<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_i64(self.mantissa)
    }

    // Forward all other types to deserialize_any
    fn deserialize_bool<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_i8<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_i16<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_u8<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_u16<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_u32<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_f32<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_f64<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_char<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_str<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_unit<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_unit_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_seq<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_tuple<V>(
        self,
        _len: usize,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> std::result::Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_any(visitor)
    }
}
