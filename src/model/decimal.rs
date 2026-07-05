//! Decimal serde integration.

use serde::{Deserialize, Deserializer, de};

use crate::decimal::Decimal;

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DecimalVisitor;
        impl<'de> de::Visitor<'de> for DecimalVisitor {
            type Value = Decimal;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("expecting String or tuple")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Decimal::from_string(v).map_err(|e| E::custom(e))
            }

            fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Decimal::from_string(&v).map_err(|e| E::custom(e))
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let e = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let m = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                Ok(Decimal::new(e, m))
            }
        }

        deserializer.deserialize_tuple_struct("Decimal", 2, DecimalVisitor)
    }
}
