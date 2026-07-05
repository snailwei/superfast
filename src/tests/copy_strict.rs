//! Tests for FAST copy operator error paths per spec §4.5.
//!
//! Verifies ERR D5, ERR D6, and the optional/initial-value branches
//! by crafting raw bytes that have the copy field absent (pmap bit clear).

use crate::model::value::ValueData;
use crate::value::Value;
use crate::{FastDecoder, FastEncoder};
use std::collections::HashMap;

fn get_uint32(data: &crate::model::template::TemplateData, field: &str) -> Option<u32> {
    match &data.value {
        ValueData::Group(g) => match g.get(field) {
            Some(ValueData::Value(Some(Value::UInt32(v)))) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

fn make_td(name: &str, field: &str, value: ValueData) -> crate::model::template::TemplateData {
    let mut map = HashMap::new();
    map.insert(field.to_string(), value);
    crate::model::template::TemplateData {
        name: name.to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    }
}

// ── ERR D5: mandatory, undefined previous, no initial value ──────────────

#[test]
fn copy_err_d5_mandatory_no_initial() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let bytes = enc.encode_template_data(td).unwrap();

    // Flip field bit to absent
    let mut bytes = bytes.clone();
    bytes[0] &= 0xDF; // clear bit 5 (0xE0 -> 0xC0)

    let mut dec = FastDecoder::new(xml).unwrap();
    let result = dec.decode_raw(&bytes);

    assert!(result.is_err(), "Expected ERR D5");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("ERR D5"), "Expected ERR D5, got: {}", err);
}

// ── ERR D5 with loose mode: uses type default ───────────────────────────

#[test]
fn copy_loose_mode_no_err_d5() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5 (0xE0 -> 0xC0)

    let mut dec = FastDecoder::new(xml).unwrap();
    dec.set_strict(false);
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    let group = match &data.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(0)))) })
            .unwrap_or(false),
        "Loose mode: Seq should be type default 0"
    );
}

// ── Copy with initial value: undefined + absent → use initial value ─────

#[test]
fn copy_initial_value_on_undefined() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy value="10"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let orig = enc.encode_template_data(td).unwrap();
    let mut bytes = orig.clone();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    let group = match &data.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(10)))) })
            .unwrap_or(false),
        "Expected initial value 10"
    );
}

// ── Optional copy, no initial: undefined → empty ────────────────────────

#[test]
fn copy_optional_no_initial_undefined_becomes_empty() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq" presence="optional"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let orig = enc.encode_template_data(td).unwrap();
    let mut bytes = orig.clone();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    let group = match &data.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group
            .get("Seq")
            .map(|v| matches!(v, ValueData::Value(None)))
            .unwrap_or(false),
        "Expected Seq to be absent (None)"
    );
}

// ── ERR D6: mandatory, empty previous ───────────────────────────────────
// ERR D6 fires when a mandatory field has an empty (None) previous value.
// Now testable with nullable="true" on mandatory fields.
// See tests/fast_nullable_d6.rs for ERR D6 tests. This test covers
// copy chain behavior (reuse then override).

#[test]
fn copy_chain_reuse_then_override() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Seq = 10
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(10))));
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert_eq!(get_uint32(&data1, "Seq"), Some(10));

    // Message 2: Seq absent → copies 10
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(0))));
    let mut msg2 = enc.encode_template_data(td).unwrap();
    msg2[0] &= 0xDF; // clear Seq pmap bit
    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    assert_eq!(get_uint32(&data2, "Seq"), Some(10));

    // Message 3: Seq = 20 → overrides
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(20))));
    let msg3 = enc.encode_template_data(td).unwrap();
    let (data3, _) = dec.decode_raw(&msg3).unwrap();
    assert_eq!(get_uint32(&data3, "Seq"), Some(20));

    // Message 4: Seq absent → copies 20
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(0))));
    let mut msg4 = enc.encode_template_data(td).unwrap();
    msg4[0] &= 0xDF;
    let (data4, _) = dec.decode_raw(&msg4).unwrap();
    assert_eq!(get_uint32(&data4, "Seq"), Some(20));
}

// ── Optional + assigned previous → reuse on absent ──────────────────────

#[test]
fn copy_optional_null_then_absent() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq" presence="optional"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();

    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(99))));
    let msg1 = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    let group1 = match &data1.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group1
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(99)))) })
            .unwrap_or(false),
        "Message 1: Seq should be 99"
    );

    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let mut msg2 = enc.encode_template_data(td).unwrap();
    msg2[0] &= 0xDF; // clear bit 5

    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    let group2 = match &data2.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group2
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(99)))) })
            .unwrap_or(false),
        "Message 2: Seq should be copied as 99"
    );
}

// ── Assigned previous → reuse ───────────────────────────────────────────

#[test]
fn copy_assigned_previous_reuse() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    let group1 = match &data1.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group1
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(1)))) })
            .unwrap_or(false),
        "Message 1: Seq should be 1"
    );

    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let mut msg2 = enc.encode_template_data(td).unwrap();
    msg2[0] &= 0xDF; // clear bit 5

    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    let group2 = match &data2.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        group2
            .get("Seq")
            .map(|v| { matches!(v, ValueData::Value(Some(Value::UInt32(1)))) })
            .unwrap_or(false),
        "Message 2: Seq should be copied as 1"
    );
}

// ── Optional field with undefined → empty on absent ─────────────────────

#[test]
fn copy_optional_undefined_then_present() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq" presence="optional"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Seq absent → undefined + optional → empty
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let mut msg1 = enc.encode_template_data(td).unwrap();
    msg1[0] &= 0xDF; // clear Seq bit
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert!(
        get_uint32(&data1, "Seq").is_none(),
        "Seq should be absent/None"
    );

    // Message 2: Seq present → overrides with actual value
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let msg2 = enc.encode_template_data(td).unwrap();
    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    assert_eq!(get_uint32(&data2, "Seq"), Some(42));

    // Message 3: Seq absent → copies 42
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let mut msg3 = enc.encode_template_data(td).unwrap();
    msg3[0] &= 0xDF;
    let (data3, _) = dec.decode_raw(&msg3).unwrap();
    assert_eq!(get_uint32(&data3, "Seq"), Some(42));
}
