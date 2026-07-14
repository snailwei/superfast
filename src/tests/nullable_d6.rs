//! Tests for ERR D6 with nullable fields.
//!
//! Now that `nullable` is properly parsed from XML (distinct from `optional`),
//! mandatory+nullable fields can decode as empty (None), triggering ERR D6
//! on subsequent messages where the field is absent.

use crate::model::value::group_get;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{Dictionary, FastDecoder, FastEncoder};
use std::rc::Rc;

fn make_td(name: &str, field: &str, value: ValueData) -> crate::model::template::TemplateData {
    let mut vec = Vec::new();
    vec.push((Rc::from(field), value));
    crate::model::template::TemplateData {
        name: name.to_string(),
        value: ValueData::Group(vec),
        pmap_bytes: None,
    }
}

fn make_td2(
    name: &str,
    f1: &str,
    v1: ValueData,
    f2: &str,
    v2: ValueData,
) -> crate::model::template::TemplateData {
    let mut vec = Vec::new();
    vec.push((Rc::from(f1), v1));
    vec.push((Rc::from(f2), v2));
    crate::model::template::TemplateData {
        name: name.to_string(),
        value: ValueData::Group(vec),
        pmap_bytes: None,
    }
}

fn field_is_empty(data: &crate::model::template::TemplateData, field: &str) -> bool {
    match &data.value {
        ValueData::Group(g) => {
            matches!(group_get(g, field), Some(ValueData::Value(None)))
        }
        _ => false,
    }
}

// ============================================================================
// Copy: ERR D6 — mandatory nullable string, NULL then absent
// Byte layout: [0xE0, 0x81, 0xD8] — pmap | tplid | data
// ============================================================================

#[test]
fn copy_mandatory_nullable_err_d6() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" nullable="true"><copy/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("X".to_string()))),
    );
    let sample = enc.encode_template_data(td).unwrap();

    // Message 1: NULL
    let mut msg1 = sample.clone();
    msg1[2] = 0x80;
    let (data1, _) = dec.parse(&msg1).unwrap();
    assert!(
        field_is_empty(&data1, "Txt"),
        "Message 1: Txt should be empty (NULL)"
    );

    // Message 2: absent → ERR D6
    let absent = [0xC0, 0x81];
    let result = dec.parse(&absent);
    assert!(result.is_err(), "Expected ERR D6");
    assert!(result.unwrap_err().to_string().contains("ERR D6"));
}

// ============================================================================
// Copy: ERR D6, loose mode
// ============================================================================

#[test]
fn copy_mandatory_nullable_err_d6_loose() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" nullable="true"><copy/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("X".to_string()))),
    );
    let sample = enc.encode_template_data(td).unwrap();

    let mut msg1 = sample.clone();
    msg1[2] = 0x80;
    let (_data1, _) = dec.parse(&msg1).unwrap();

    let absent = [0xC0, 0x81];
    dec.set_strict(false);
    let (data2, _) = dec.parse(&absent).unwrap();
    assert!(
        field_is_empty(&data2, "Txt"),
        "Loose mode: Txt should be empty"
    );
}

// ============================================================================
// Copy: mandatory nullable — NULL then present (no error)
// ============================================================================

#[test]
fn copy_mandatory_nullable_null_then_present() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" nullable="true"><copy/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("X".to_string()))),
    );
    let sample = enc.encode_template_data(td).unwrap();

    let mut msg1 = sample.clone();
    msg1[2] = 0x80;
    let (_data1, _) = dec.parse(&msg1).unwrap();

    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("Hello".to_string()))),
    );
    let msg2 = enc.encode_template_data(td).unwrap();
    let (data2, _) = dec.parse(&msg2).unwrap();
    let group = match &data2.value {
        ValueData::Group(g) => g,
        _ => panic!("Expected group"),
    };
    assert!(
        matches!(group_get(group, "Txt"), Some(ValueData::Value(Some(Value::AsciiString(v)))) if v == "Hello"),
        "Txt should be Hello"
    );
}

// ============================================================================
// Copy: ERR D6 — nullable int (multi-field template)
// With 2 fields, we can encode a NULL for one field and then
// send a message without that field → ERR D6.
// ============================================================================

#[test]
fn copy_mandatory_nullable_int_err_d6() {
    let xml = r#"<templates>
  <template id="1" name="T">
    <uInt32 id="1" name="Seq" nullable="true"><copy/></uInt32>
    <string id="2" name="Txt"><copy/></string>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    // Encode Seq=1, Txt="Hi"
    let td = make_td2(
        "T",
        "Seq",
        ValueData::Value(Some(Value::UInt32(1))),
        "Txt",
        ValueData::Value(Some(Value::AsciiString("Hi".to_string()))),
    );
    let sample = enc.encode_template_data(td).unwrap();
    eprintln!("Copy nullable int sample: {:02X?}", sample);

    // Replace Seq value (0x82 = nullable uint entity value 2 → value 1)
    // with NULL (0x80 = entity value 0 → NULL)
    let mut msg1 = sample.clone();
    for byte in msg1.iter_mut() {
        if *byte == 0x82 {
            *byte = 0x80;
            break;
        }
    }
    eprintln!("NULL message: {:02X?}", msg1);

    let (data1, _) = dec.parse(&msg1).unwrap();
    assert!(
        field_is_empty(&data1, "Seq"),
        "Message 1: Seq should be empty (NULL)"
    );

    // Message 2: Seq absent, Txt present → ERR D6
    let td2 = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("Hi".to_string()))),
    );
    let absent = enc.encode_template_data(td2).unwrap();
    eprintln!("Absent message: {:02X?}", absent);

    let result = dec.parse(&absent);
    assert!(result.is_err(), "Expected ERR D6");
    let err = result.unwrap_err().to_string();
    assert!(err.contains("ERR D6"), "Expected ERR D6, got: {}", err);
}

// ============================================================================
// Verify nullable is distinct from optional
// ============================================================================

#[test]
fn nullable_distinct_from_optional() {
    let xml_optional = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" presence="optional"><copy/></string>
  </template>
</templates>"#;

    let xml_nullable = r#"<templates>
  <template id="1" name="T">
    <string id="1" name="Txt" nullable="true"><copy/></string>
  </template>
</templates>"#;

    // Optional: two absent messages — no error
    let mut enc = FastEncoder::new(xml_optional, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml_optional, Dictionary::Global).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("X".to_string()))),
    );
    enc.encode_template_data(td).unwrap();

    let absent = [0xC0, 0x81];
    dec.parse(&absent).unwrap();
    dec.parse(&absent).unwrap(); // No error for optional

    // Nullable: NULL then absent — ERR D6
    let mut enc2 = FastEncoder::new(xml_nullable, Dictionary::Global).unwrap();
    let mut dec2 = FastDecoder::new(xml_nullable, Dictionary::Global).unwrap();
    let td = make_td(
        "T",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("X".to_string()))),
    );
    let sample2 = enc2.encode_template_data(td).unwrap();

    let mut null_msg = sample2.clone();
    null_msg[2] = 0x80;
    dec2.parse(&null_msg).unwrap();

    let result = dec2.parse(&absent);
    assert!(result.is_err(), "Nullable: expected ERR D6");
    assert!(result.unwrap_err().to_string().contains("ERR D6"));
}
