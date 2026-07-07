//! Integration tests for FAST Decimal type.
//!
//! Covers parsing, formatting, float conversion, and wire format roundtrips.

use crate::{Dictionary, FastDecoder, FastEncoder};
use crate::decimal::Decimal;
use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use std::collections::HashMap;

// =====================================================
// Helpers for wire format tests
// =====================================================

fn decimal_xml(optional: bool) -> String {
    if optional {
        r#"<templates>
  <template id="100" name="DecTest">
    <decimal id="1" name="Price" presence="optional"/>
  </template>
</templates>"#
            .to_string()
    } else {
        r#"<templates>
  <template id="100" name="DecTest">
    <decimal id="1" name="Price"/>
  </template>
</templates>"#
            .to_string()
    }
}

fn make_td(name: &str, value: ValueData) -> TemplateData {
    let mut map = HashMap::new();
    map.insert("Price".to_string(), value);
    TemplateData {
        name: name.to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    }
}

// =====================================================
// from_string — various input formats
// =====================================================

#[test]
fn decimal_from_integer_string() {
    // Pure integer
    let d = Decimal::from_string("942755").unwrap();
    assert_eq!(d, Decimal::new(0, 942755));

    // Integer with trailing zeros → normalized
    let d = Decimal::from_string("94275500").unwrap();
    assert_eq!(d, Decimal::new(2, 942755));

    let d = Decimal::from_string("1000").unwrap();
    assert_eq!(d, Decimal::new(3, 1));

    let d = Decimal::from_string("10").unwrap();
    assert_eq!(d, Decimal::new(1, 1));
}

#[test]
fn decimal_from_fractional_string() {
    let d = Decimal::from_string("9427.55").unwrap();
    assert_eq!(d, Decimal::new(-2, 942755));

    let d = Decimal::from_string(".55").unwrap();
    assert_eq!(d, Decimal::new(-2, 55));

    let d = Decimal::from_string("0.001").unwrap();
    assert_eq!(d, Decimal::new(-3, 1));

    let d = Decimal::from_string("123.456").unwrap();
    assert_eq!(d, Decimal::new(-3, 123456));
}

#[test]
fn decimal_from_negative_string() {
    let d = Decimal::from_string("-9427.55").unwrap();
    assert_eq!(d, Decimal::new(-2, -942755));

    let d = Decimal::from_string("-100").unwrap();
    assert_eq!(d, Decimal::new(2, -1));

    let d = Decimal::from_string("-.5").unwrap();
    assert_eq!(d, Decimal::new(-1, -5));
}

#[test]
fn decimal_from_zero() {
    let d = Decimal::from_string("0").unwrap();
    assert_eq!(d, Decimal::new(0, 0));

    let d = Decimal::from_string("0.00").unwrap();
    assert_eq!(d, Decimal::new(0, 0));

    let d = Decimal::from_string("0.0").unwrap();
    assert_eq!(d, Decimal::new(0, 0));

    // Trailing zeros on zero mantissa → normalized to (0, 0)
    let d = Decimal::from_string("000").unwrap();
    assert_eq!(d, Decimal::new(0, 0));
}

#[test]
fn decimal_invalid_strings() {
    assert!(Decimal::from_string("abc").is_err());
    assert!(Decimal::from_string("1.2.3").is_err());
    assert!(Decimal::from_string("").is_err());
    assert!(Decimal::from_string(".").is_err());
}

// =====================================================
// Normalization — mantissa % 10 != 0
// =====================================================

#[test]
fn decimal_normalization_strips_trailing_zeros() {
    let d = Decimal::from_string("1200").unwrap();
    assert_eq!(d, Decimal::new(2, 12));

    let d = Decimal::from_string("120").unwrap();
    assert_eq!(d, Decimal::new(1, 12));

    let d = Decimal::from_string("1200.00").unwrap();
    assert_eq!(d, Decimal::new(2, 12));
}

#[test]
fn decimal_normalization_zero_exponent_zero() {
    // When mantissa is zero, both exponent and mantissa are zero
    let d = Decimal::from_string("0.0").unwrap();
    assert_eq!(d, Decimal::new(0, 0));

    let d = Decimal::from_string("0000.0000").unwrap();
    assert_eq!(d, Decimal::new(0, 0));
}

// =====================================================
// from_float — roundtrip
// =====================================================

#[test]
fn decimal_from_float() {
    let d = Decimal::from_float(9427.55).unwrap();
    assert_eq!(d.exponent, -2);
    assert_eq!(d.mantissa, 942755);

    let d = Decimal::from_float(0.0).unwrap();
    assert_eq!(d, Decimal::new(0, 0));

    let d = Decimal::from_float(100.0).unwrap();
    assert_eq!(d, Decimal::new(2, 1));

    // Non-finite values should error
    assert!(Decimal::from_float(f64::NAN).is_err());
    assert!(Decimal::from_float(f64::INFINITY).is_err());
    assert!(Decimal::from_float(f64::NEG_INFINITY).is_err());
}

// =====================================================
// to_float — roundtrip
// =====================================================

#[test]
fn decimal_to_float_positive_exponent() {
    let d = Decimal::new(2, 942755);
    assert!((d.to_float() - 94275500.0).abs() < 0.01);

    let d = Decimal::new(0, 942755);
    assert!((d.to_float() - 942755.0).abs() < 0.01);

    let d = Decimal::new(6, 1);
    assert!((d.to_float() - 1000000.0).abs() < 0.01);
}

#[test]
fn decimal_to_float_negative_exponent() {
    let d = Decimal::new(-2, 942755);
    assert!((d.to_float() - 9427.55).abs() < 0.01);

    let d = Decimal::new(-3, 1);
    assert!((d.to_float() - 0.001).abs() < 0.0001);

    let d = Decimal::new(-5, 12345);
    assert!((d.to_float() - 0.12345).abs() < 0.00001);
}

#[test]
fn decimal_to_float_negative_values() {
    let d = Decimal::new(-2, -942755);
    assert!((d.to_float() - (-9427.55)).abs() < 0.01);

    let d = Decimal::new(0, -100);
    assert!((d.to_float() - (-100.0)).abs() < 0.01);
}

// =====================================================
// Display formatting
// =====================================================

#[test]
fn decimal_display() {
    let d = Decimal::new(-2, 942755);
    let s = format!("{}", d);
    assert!(
        s.contains("9427") || s.contains("9427"),
        "unexpected display: {}",
        s
    );

    let d = Decimal::new(2, 942755);
    let s = format!("{}", d);
    assert!(s.contains("94275500"), "unexpected display: {}", s);
}

// =====================================================
// Wire format — encoder/decoder integration
// =====================================================

#[test]
fn decimal_roundtrip_mandatory() {
    let xml = decimal_xml(false);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "DecTest",
        ValueData::Value(Some(Value::Decimal(Decimal::new(-2, 942755)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.parse(&bytes).unwrap();
    let group = if let ValueData::Group(ref g) = tpl.value {
        g
    } else {
        panic!("expected Group")
    };
    let price = group.get("Price").unwrap();
    if let ValueData::Value(Some(Value::Decimal(d))) = price {
        assert_eq!(d.exponent, -2);
        assert_eq!(d.mantissa, 942755);
    } else {
        panic!("expected Decimal, got: {:?}", price);
    }
}

#[test]
fn decimal_roundtrip_positive_exponent() {
    let xml = decimal_xml(false);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "DecTest",
        ValueData::Value(Some(Value::Decimal(Decimal::new(2, 942755)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.parse(&bytes).unwrap();
    let group = if let ValueData::Group(ref g) = tpl.value {
        g
    } else {
        panic!("expected Group")
    };
    let price = group.get("Price").unwrap();
    if let ValueData::Value(Some(Value::Decimal(d))) = price {
        assert_eq!(d.exponent, 2);
        assert_eq!(d.mantissa, 942755);
    } else {
        panic!("expected Decimal, got: {:?}", price);
    }
}

#[test]
fn decimal_roundtrip_negative_value() {
    let xml = decimal_xml(false);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "DecTest",
        ValueData::Value(Some(Value::Decimal(Decimal::new(-2, -942755)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.parse(&bytes).unwrap();
    let group = if let ValueData::Group(ref g) = tpl.value {
        g
    } else {
        panic!("expected Group")
    };
    let price = group.get("Price").unwrap();
    if let ValueData::Value(Some(Value::Decimal(d))) = price {
        assert_eq!(d.exponent, -2);
        assert_eq!(d.mantissa, -942755);
    } else {
        panic!("expected Decimal, got: {:?}", price);
    }
}

#[test]
fn decimal_roundtrip_zero() {
    let xml = decimal_xml(false);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "DecTest",
        ValueData::Value(Some(Value::Decimal(Decimal::new(0, 0)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.parse(&bytes).unwrap();
    let group = if let ValueData::Group(ref g) = tpl.value {
        g
    } else {
        panic!("expected Group")
    };
    let price = group.get("Price").unwrap();
    if let ValueData::Value(Some(Value::Decimal(d))) = price {
        assert_eq!(d.exponent, 0);
        assert_eq!(d.mantissa, 0);
    } else {
        panic!("expected Decimal, got: {:?}", price);
    }
}

#[test]
fn decimal_roundtrip_optional_absent() {
    let xml = decimal_xml(true);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td("DecTest", ValueData::Value(None));
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.parse(&bytes).unwrap();
    let group = if let ValueData::Group(ref g) = tpl.value {
        g
    } else {
        panic!("expected Group")
    };
    let price = group.get("Price").unwrap();
    assert!(
        matches!(price, ValueData::Value(None)),
        "expected None, got: {:?}",
        price
    );
}

#[test]
fn decimal_wire_format_spec_example() {
    // FAST spec example: 94275500 = 942755 × 10²
    // exponent: 0x82 (2)  mantissa: 0x39 0x45 0xA3 (942755)
    let xml = decimal_xml(false);
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "DecTest",
        ValueData::Value(Some(Value::Decimal(Decimal::new(2, 942755)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    // The wire format should be: pmap byte(s) + exponent + mantissa
    // Exponent 2: two's complement 0000010 (7 bits) = 0x82
    // Mantissa 942755: 0x39 0x45 0xA3
    assert!(
        bytes.len() >= 3,
        "expected at least 3 bytes (exponent + mantissa), got: {:?}",
        bytes
    );

    // The body should contain the exponent and mantissa bytes
    // Find 0x82 in the body (after pmap)
    let idx = bytes
        .iter()
        .position(|&b| b == 0x82)
        .expect("exponent byte 0x82 not found");
    assert_eq!(bytes[idx + 1], 0x39, "mantissa first byte mismatch");
    assert_eq!(bytes[idx + 2], 0x45, "mantissa second byte mismatch");
    assert_eq!(bytes[idx + 3], 0xA3, "mantissa third byte mismatch");
}

#[test]
fn f64_field_rejected() {
    #[derive(serde::Serialize)]
    #[serde(rename = "DecTest")]
    struct DecTestF64 {
        #[serde(rename = "Price")]
        price: f64,
    }

    let xml = r#"<templates>
  <template id="100" name="DecTest">
    <decimal id="1" name="Price"/>
  </template>
</templates>"#;

    let mut enc = crate::FastEncoder::new(xml, Dictionary::Global).unwrap();
    let msg = DecTestF64 { price: 9427.55 };
    let result = enc.encode(&msg);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("f32/f64"),
        "expected f32/f64 error, got: {err}"
    );
}

#[test]
fn f32_field_rejected() {
    #[derive(serde::Serialize)]
    #[serde(rename = "DecTest")]
    struct DecTestF32 {
        #[serde(rename = "Price")]
        price: f32,
    }

    let xml = r#"<templates>
  <template id="100" name="DecTest">
    <decimal id="1" name="Price"/>
  </template>
</templates>"#;

    let mut enc = crate::FastEncoder::new(xml, Dictionary::Global).unwrap();
    let msg = DecTestF32 { price: 9427.55 };
    let result = enc.encode(&msg);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("f32/f64"),
        "expected f32/f64 error, got: {err}"
    );
}
