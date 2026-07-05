//! Tests for FAST operator strict/loose error paths per spec §4.5.
//!
//! Covers Increment (ERR D6), Delta (empty previous), Tail (ERR D5/D6).
//! Default strict mode = true; set_strict(false) only for loose-mode tests.

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

fn get_string(data: &crate::model::template::TemplateData, field: &str) -> Option<String> {
    match &data.value {
        ValueData::Group(g) => match g.get(field) {
            Some(ValueData::Value(Some(Value::AsciiString(v)))) => Some(v.clone()),
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

// ============================================================================
// Increment: ERR D5 — mandatory, undefined, no initial value
// With increment, the initial value is always defined when value attr is present.
// ERR D5 fires only when increment has no initial value. Testing by manually
// clearing the pmap bit on a message with no prior context.
// ============================================================================

#[test]
fn increment_absent_with_initial_increments() {
    // When field is absent but initial_value exists, increment it
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><increment value="10"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5 → absent

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    // Undefined context + initial_value=10 → increment(10) = 11
    assert_eq!(
        get_uint32(&data, "Seq"),
        Some(11),
        "Increment operator increments initial value when absent"
    );
}

// ============================================================================
// Increment: loose mode D5 — type default incremented
// ============================================================================

#[test]
fn increment_loose_d5_uses_type_default() {
    // Same template, but in loose mode with no initial value the code
    // would use type default. Since we always have value="0", the path is:
    // undefined + initial_value=Some(0) → increment(0) = 1
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><increment value="0"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    dec.set_strict(false);
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    // Strict or loose: initial_value=Some(0), increment(0) = 1
    assert_eq!(
        get_uint32(&data, "Seq"),
        Some(1),
        "initial_value=0 → increment → 1"
    );
}

// ============================================================================
// Increment: chain — present then absent
// ============================================================================

#[test]
fn increment_chain_present_then_absent() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><increment value="0"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Seq = 100 (present)
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(100))));
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert_eq!(get_uint32(&data1, "Seq"), Some(100));

    // Message 2: Seq absent → increment previous (100) → 101
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(0))));
    let mut msg2 = enc.encode_template_data(td).unwrap();
    msg2[0] &= 0xDF; // clear bit 5
    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    assert_eq!(get_uint32(&data2, "Seq"), Some(101));
}

// ============================================================================
// Increment: optional field, undefined + initial value → uses initial
// (Matches Copy operator: initial value takes precedence over optional)
// ============================================================================

#[test]
fn increment_optional_undefined_uses_initial() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq" presence="optional"><increment value="0"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(1))));
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    // Optional + initial_value=Some(0): initial value takes precedence
    // increment(0) = 1
    assert_eq!(
        get_uint32(&data, "Seq"),
        Some(1),
        "Optional with initial value: increment(initial)"
    );
}

// ============================================================================
// Increment: sequence round-trip (matches fast_operators pattern)
// ============================================================================

#[test]
fn increment_sequence_absent() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><increment value="0"/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Seq = 5
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(5))));
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert_eq!(get_uint32(&data1, "Seq"), Some(5));

    // Message 2-4: all absent → 6, 7, 8
    for expected in 6..=8u32 {
        let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(0))));
        let mut msg = enc.encode_template_data(td).unwrap();
        msg[0] &= 0xDF;
        let (data, _) = dec.decode_raw(&msg).unwrap();
        assert_eq!(get_uint32(&data, "Seq"), Some(expected));
    }
}

// ============================================================================
// Tail: ERR D5 — mandatory, undefined previous, no initial value
// ============================================================================

#[test]
fn tail_err_d5_mandatory_no_initial() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt"><tail/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    // Clear field pmap bit → absent
    let mut bytes = bytes.clone();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let result = dec.decode_raw(&bytes);

    assert!(result.is_err(), "Expected ERR D5");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("ERR D5"), "Expected ERR D5, got: {}", err);
}

// ============================================================================
// Tail: ERR D5, loose mode — use type default
// ============================================================================

#[test]
fn tail_loose_d5_uses_type_default() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt"><tail/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    dec.set_strict(false);
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    // Loose mode: type default for string = ""
    assert_eq!(
        get_string(&data, "Txt"),
        Some("".to_string()),
        "Loose mode: Txt should be empty string (type default)"
    );
}

// ============================================================================
// Tail: with initial value — undefined + absent → use initial
// ============================================================================

#[test]
fn tail_initial_value_on_undefined() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt"><tail value="XYZ"/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    assert_eq!(
        get_string(&data, "Txt"),
        Some("XYZ".to_string()),
        "Expected initial value XYZ"
    );
}

// ============================================================================
// Tail: chain — present then absent reuses previous
// ============================================================================

#[test]
fn tail_chain_present_then_absent() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt"><tail/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Txt = "ABCDE" (present, establishes base)
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABCDE".to_string()))),
    );
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert_eq!(get_string(&data1, "Txt"), Some("ABCDE".to_string()));

    // Message 2: Txt absent → copies "ABCDE"
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("".to_string()))),
    );
    let mut msg2 = enc.encode_template_data(td).unwrap();
    msg2[0] &= 0xDF; // clear bit 5
    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    assert_eq!(get_string(&data2, "Txt"), Some("ABCDE".to_string()));
}

// ============================================================================
// Tail: optional field, undefined → Ok(None)
// ============================================================================

#[test]
fn tail_optional_undefined_returns_none() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" presence="optional"><tail/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let (data, _consumed) = dec.decode_raw(&bytes).unwrap();

    assert!(
        get_string(&data, "Txt").is_none(),
        "Optional: Txt should be None"
    );
}

// ============================================================================
// Tail: present + tail data — truncation and append
// ============================================================================

#[test]
fn tail_present_truncates_and_appends() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt"><tail/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Txt = "ABCDEFGHIJ" (full, establishes base)
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABCDEFGHIJ".to_string()))),
    );
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert_eq!(get_string(&data1, "Txt"), Some("ABCDEFGHIJ".to_string()));

    // Message 2: Txt = "XYZ" (tail present, replaces last 3 chars)
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("XYZ".to_string()))),
    );
    let msg2 = enc.encode_template_data(td).unwrap();
    let (data2, _) = dec.decode_raw(&msg2).unwrap();
    assert_eq!(get_string(&data2, "Txt"), Some("ABCDEFGXYZ".to_string()));
}

// ============================================================================
// Tail: empty previous state — NULL then tail present uses initial value as base
// §4.8: "empty → Initial value, or type default"
// ============================================================================

#[test]
fn tail_empty_state_uses_initial_as_base() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" nullable="true"><tail value="PREFIX"/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: NULL (sets previous state to "empty")
    let td = make_td("T", "Txt", ValueData::Value(None));
    let msg1 = enc.encode_template_data(td).unwrap();
    let (data1, _) = dec.decode_raw(&msg1).unwrap();
    assert!(
        get_string(&data1, "Txt").is_none(),
        "Message 1: Txt should be None (NULL/empty)"
    );

    // Message 2: tail present with "SUFFIX" — base should be initial value "PREFIX"
    // Tail semantics: "SUFFIX" (6) >= "PREFIX" (6) → result = "SUFFIX"
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("SUFFIX".to_string()))),
    );
    let msg2 = enc.encode_template_data(td).unwrap();
    let (data2, _) = dec.decode_raw(&msg2).unwrap();

    assert_eq!(
        get_string(&data2, "Txt"),
        Some("SUFFIX".to_string()),
        "Empty state + tail present: base=initial_value, result=SUFFIX"
    );
}

// ============================================================================
// Copy: strict error still works (regression check)
// ============================================================================

#[test]
fn copy_strict_regression() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq"><copy/></uInt32>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml).unwrap();
    let td = make_td("T", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let mut bytes = enc.encode_template_data(td).unwrap();
    bytes[0] &= 0xDF; // clear bit 5

    let mut dec = FastDecoder::new(xml).unwrap();
    let result = dec.decode_raw(&bytes);

    assert!(result.is_err(), "Copy: expected ERR D5 in strict mode");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("ERR D5"),
        "Copy: expected ERR D5, got: {}",
        err
    );
}

// ============================================================================
// Default: ERR S5 — mandatory nullable without default value
// Fires when is_nullable() && !is_optional() && no initial_value.
// Now reachable with nullable="true" on mandatory fields.
// Static error [ERR S5] per spec §4.4.
// ============================================================================

// ============================================================================
// ERR D6 tests now live in tests/fast_nullable_d6.rs
// With nullable attribute parsing (distinct from optional), ERR D6 is
// testable: mandatory+nullable fields can decode as NULL, and a subsequent
// absent message triggers ERR D6. See fast_nullable_d6.rs for copy, int,
// and nullable-vs-optional tests.
// ============================================================================

// ============================================================================
// Increment ERR D5 (no initial value) is untestable: the increment operator
// requires a value attribute per FAST schema. <increment/> without value
// would have initial_value=None, but this is a schema violation.
// ============================================================================
