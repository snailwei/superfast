//! Integration tests for FAST EBNF elements:
//!
//! Stream Structure (EBNF):
//! ```text
//! stream ::= message* | block*
//! block  ::= BlockSize message+
//! message ::= segment
//! segment ::= PresenceMap TemplateIdentifier? (field | segment)*
//! field   ::= integer | string | delta | ScaledNumber | ByteVector
//! integer ::= UnsignedInteger | SignedInteger
//! string  ::= ASCIIString | UnicodeString
//! delta   ::= IntegerDelta | ScaledNumberDelta | ASCIIStringDelta | ByteVectorDelta
//! ```

use crate::FastDecoder;
use crate::FastEncoder;
use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use std::collections::HashMap;

// ============================================================
// Helpers
// ============================================================

fn make_td(name: &str, field: &str, value: ValueData) -> TemplateData {
    let mut map = HashMap::new();
    map.insert(field.to_string(), value);
    TemplateData {
        name: name.to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    }
}

fn roundtrip(xml: &str, td: TemplateData) -> TemplateData {
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();
    let bytes = enc.encode_template_data(td).unwrap();
    let (tpl, consumed) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(consumed, bytes.len(), "decoder did not consume all bytes");
    tpl
}

fn get_field<'a>(tpl: &'a TemplateData, field: &str) -> &'a ValueData {
    if let ValueData::Group(ref g) = tpl.value {
        g.get(field)
            .unwrap_or_else(|| panic!("field '{}' not found in decoded template", field))
    } else {
        panic!("expected ValueData::Group, got: {:?}", tpl.value)
    }
}

/// Encode a non-nullable unsigned varint (stop-bit, MSB-first).
fn encode_uint(value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    if value == 0 {
        buf.push(0x80);
        return buf;
    }
    let bits = 64 - value.leading_zeros();
    let chunks = (bits + 6) / 7;
    for i in 0..chunks {
        let shift = (chunks - 1 - i) * 7;
        let byte = ((value >> shift) & 0x7F) as u8;
        if i == chunks - 1 {
            buf.push(byte | 0x80);
        } else {
            buf.push(byte);
        }
    }
    buf
}

/// Decode a non-nullable unsigned varint from a byte slice, returning (value, bytes_consumed).
fn decode_uint(data: &[u8]) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut pos = 0;
    for byte in data.iter() {
        if pos >= 7 {
            return None; // overflow
        }
        value = (value << 7) | (*byte as u64 & 0x7F);
        pos += 1;
        if *byte & 0x80 != 0 {
            break;
        }
    }
    Some((value, pos))
}

// ============================================================
// XML helpers
// ============================================================

fn int_xml(tag: &str, optional: bool) -> String {
    if optional {
        format!(
            r#"<templates>
  <template id="1" name="IntTest">
    <{tag} id="1" name="Val" presence="optional"/>
  </template>
</templates>"#
        )
    } else {
        format!(
            r#"<templates>
  <template id="1" name="IntTest">
    <{tag} id="1" name="Val"/>
  </template>
</templates>"#
        )
    }
}

fn ascii_xml(optional: bool) -> String {
    if optional {
        r#"<templates>
  <template id="2" name="StrTest">
    <string id="2" name="Txt" presence="optional"/>
  </template>
</templates>"#
            .to_string()
    } else {
        r#"<templates>
  <template id="2" name="StrTest">
    <string id="2" name="Txt"/>
  </template>
</templates>"#
            .to_string()
    }
}

fn unicode_xml(optional: bool) -> String {
    if optional {
        r#"<templates>
  <template id="3" name="UnicodeTest">
    <string id="3" name="Txt" presence="optional" charset="unicode"/>
  </template>
</templates>"#
            .to_string()
    } else {
        r#"<templates>
  <template id="3" name="UnicodeTest">
    <string id="3" name="Txt" charset="unicode"/>
  </template>
</templates>"#
            .to_string()
    }
}

fn bytevector_xml(optional: bool) -> String {
    if optional {
        r#"<templates>
  <template id="4" name="BVTest">
    <byteVector id="4" name="Data" presence="optional"/>
  </template>
</templates>"#
            .to_string()
    } else {
        r#"<templates>
  <template id="4" name="BVTest">
    <byteVector id="4" name="Data"/>
  </template>
</templates>"#
            .to_string()
    }
}

fn delta_int_xml() -> String {
    r#"<templates>
  <template id="5" name="DeltaTest">
    <int32 id="5" name="Val"><delta/></int32>
  </template>
</templates>"#
        .to_string()
}

fn delta_string_xml() -> String {
    r#"<templates>
  <template id="6" name="DeltaStrTest">
    <string id="6" name="Txt"><delta/></string>
  </template>
</templates>"#
        .to_string()
}

fn delta_bv_xml() -> String {
    r#"<templates>
  <template id="7" name="DeltaBVTest">
    <byteVector id="7" name="Data"><delta/></byteVector>
  </template>
</templates>"#
        .to_string()
}

fn delta_decimal_xml() -> String {
    r#"<templates>
  <template id="8" name="DeltaDecTest">
    <decimal id="8" name="Price"><delta/></decimal>
  </template>
</templates>"#
        .to_string()
}

// ============================================================
// 1. INTEGER — integer ::= UnsignedInteger | SignedInteger
// ============================================================

#[test]
fn int32_mandatory() {
    let xml = int_xml("int32", false);
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::Int32(942755))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::Int32(942755)))
    );
}

#[test]
fn u_int32_mandatory() {
    let xml = int_xml("uInt32", false);
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::UInt32(4294967295))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::UInt32(4294967295)))
    );
}

#[test]
fn int64_mandatory() {
    let xml = int_xml("int64", false);
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::Int64(-2147483648i64))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::Int64(-2147483648i64)))
    );
}

#[test]
fn u_int64_mandatory() {
    let xml = int_xml("uInt64", false);
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::UInt64(18446744073709551615u64))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::UInt64(18446744073709551615u64)))
    );
}

#[test]
fn int32_optional_present() {
    let xml = int_xml("int32", true);
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::Int32(-42))));
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::Int32(-42)))
    );
}

#[test]
fn int32_optional_absent() {
    let xml = int_xml("int32", true);
    let td = make_td("IntTest", "Val", ValueData::Value(None));
    let tpl = roundtrip(&xml, td);
    assert_eq!(*get_field(&tpl, "Val"), ValueData::Value(None));
}

#[test]
fn int32_zero() {
    let xml = int_xml("int32", false);
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::Int32(0))));
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::Int32(0)))
    );
}

#[test]
fn u_int32_zero() {
    let xml = int_xml("uInt32", false);
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(0))));
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::UInt32(0)))
    );
}

#[test]
fn int32_negative() {
    let xml = int_xml("int32", false);
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::Int32(-100000))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::Int32(-100000)))
    );
}

// ============================================================
// 2. STRING — string ::= ASCIIString | UnicodeString
// ============================================================

#[test]
fn ascii_empty() {
    let xml = ascii_xml(false);
    let td = make_td(
        "StrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString(String::new()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::AsciiString(String::new())))
    );
}

#[test]
fn ascii_basic() {
    let xml = ascii_xml(false);
    let td = make_td(
        "StrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::AsciiString("ABC".to_string())))
    );
}

#[test]
fn ascii_optional_present() {
    let xml = ascii_xml(true);
    let td = make_td(
        "StrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::AsciiString("hello".to_string())))
    );
}

#[test]
fn ascii_optional_absent() {
    let xml = ascii_xml(true);
    let td = make_td("StrTest", "Txt", ValueData::Value(None));
    let tpl = roundtrip(&xml, td);
    assert_eq!(*get_field(&tpl, "Txt"), ValueData::Value(None));
}

#[test]
fn unicode_basic() {
    let xml = unicode_xml(false);
    let td = make_td(
        "UnicodeTest",
        "Txt",
        ValueData::Value(Some(Value::UnicodeString("\u{4e00}\u{4e01}".to_string()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::UnicodeString("\u{4e00}\u{4e01}".to_string())))
    );
}

#[test]
fn unicode_empty() {
    let xml = unicode_xml(false);
    let td = make_td(
        "UnicodeTest",
        "Txt",
        ValueData::Value(Some(Value::UnicodeString(String::new()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::UnicodeString(String::new())))
    );
}

#[test]
fn unicode_optional_absent() {
    let xml = unicode_xml(true);
    let td = make_td("UnicodeTest", "Txt", ValueData::Value(None));
    let tpl = roundtrip(&xml, td);
    assert_eq!(*get_field(&tpl, "Txt"), ValueData::Value(None));
}

// ============================================================
// 3. BYTEVECTOR — field ::= ByteVector
// ============================================================

#[test]
fn bytevector_empty() {
    let xml = bytevector_xml(false);
    let td = make_td(
        "BVTest",
        "Data",
        ValueData::Value(Some(Value::Bytes(Vec::new()))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Data"),
        ValueData::Value(Some(Value::Bytes(Vec::new())))
    );
}

#[test]
fn bytevector_basic() {
    let xml = bytevector_xml(false);
    let td = make_td(
        "BVTest",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![0x00, 0x41, 0xFF, 0x80]))),
    );
    let tpl = roundtrip(&xml, td);
    assert_eq!(
        *get_field(&tpl, "Data"),
        ValueData::Value(Some(Value::Bytes(vec![0x00, 0x41, 0xFF, 0x80])))
    );
}

#[test]
fn bytevector_optional_absent() {
    let xml = bytevector_xml(true);
    let td = make_td("BVTest", "Data", ValueData::Value(None));
    let tpl = roundtrip(&xml, td);
    assert_eq!(*get_field(&tpl, "Data"), ValueData::Value(None));
}

// ============================================================
// 4. DELTA — delta ::= IntegerDelta | ScaledNumberDelta | ASCIIStringDelta | ByteVectorDelta
// ============================================================

#[test]
fn delta_integer_basic() {
    let xml = delta_int_xml();
    // First message: delta from default (0)
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "DeltaTest",
        "Val",
        ValueData::Value(Some(Value::Int32(100))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    // Second message: delta from 100
    let td = make_td(
        "DeltaTest",
        "Val",
        ValueData::Value(Some(Value::Int32(105))),
    );
    let bytes2 = enc.encode_template_data(td).unwrap();

    // Decode both with same decoder
    let mut dec = FastDecoder::new(&xml).unwrap();
    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(
        get_field(&tpl, "Val"),
        &ValueData::Value(Some(Value::Int32(100)))
    );
    let (tpl, _) = dec.decode_raw(&bytes2).unwrap();
    assert_eq!(
        get_field(&tpl, "Val"),
        &ValueData::Value(Some(Value::Int32(105)))
    );
}

#[test]
fn delta_string_basic() {
    let xml = delta_string_xml();
    let mut enc = FastEncoder::new(&xml).unwrap();
    // First: full string (base is empty default)
    let td = make_td(
        "DeltaStrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    // Second: new string
    let td = make_td(
        "DeltaStrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("world".to_string()))),
    );
    let bytes2 = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml).unwrap();
    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(
        get_field(&tpl, "Txt"),
        &ValueData::Value(Some(Value::AsciiString("hello".to_string())))
    );
    let (tpl, _) = dec.decode_raw(&bytes2).unwrap();
    assert_eq!(
        get_field(&tpl, "Txt"),
        &ValueData::Value(Some(Value::AsciiString("world".to_string())))
    );
}

#[test]
fn delta_bytevector_basic() {
    let xml = delta_bv_xml();
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "DeltaBVTest",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![0x41, 0x42, 0x43]))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    let mut dec = FastDecoder::new(&xml).unwrap();
    let (tpl, _consumed) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(
        *get_field(&tpl, "Data"),
        ValueData::Value(Some(Value::Bytes(vec![0x41, 0x42, 0x43])))
    );
}

#[test]
fn delta_decimal_basic() {
    use crate::decimal::Decimal;
    let xml = delta_decimal_xml();
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "DeltaDecTest",
        "Price",
        ValueData::Value(Some(Value::Decimal(Decimal::new(0, 100000)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    // Second: delta from first
    let td = make_td(
        "DeltaDecTest",
        "Price",
        ValueData::Value(Some(Value::Decimal(Decimal::new(0, 100500)))),
    );
    let bytes2 = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml).unwrap();
    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(
        *get_field(&tpl, "Price"),
        ValueData::Value(Some(Value::Decimal(Decimal::new(0, 100000))))
    );
    let (tpl, _) = dec.decode_raw(&bytes2).unwrap();
    assert_eq!(
        *get_field(&tpl, "Price"),
        ValueData::Value(Some(Value::Decimal(Decimal::new(0, 100500))))
    );
}

// ============================================================
// 5. WIRE FORMAT — spec encoding examples
// ============================================================

#[test]
fn wire_format_signed_int_942755() {
    // Spec example: 942755 -> 0x39 0x45 0xA3
    let xml = int_xml("int32", false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "IntTest",
        "Val",
        ValueData::Value(Some(Value::Int32(942755))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    assert!(
        bytes.windows(3).any(|w| w == [0x39, 0x45, 0xA3]),
        "expected 0x39 0x45 0xA3 in wire bytes: {:02X?}",
        bytes
    );
}

#[test]
fn wire_format_signed_int_64_sign_extension() {
    // Spec example: +64 requires sign-bit extension -> 0x00 0xC0
    let xml = int_xml("int32", false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::Int32(64))));
    let bytes = enc.encode_template_data(td).unwrap();
    assert!(
        bytes.windows(2).any(|w| w == [0x00, 0xC0]),
        "expected 0x00 0xC0 for +64 with sign extension: {:02X?}",
        bytes
    );
}

#[test]
fn wire_format_ascii_abc() {
    // Spec example: "ABC" -> 0x41 0x42 0xC3
    let xml = ascii_xml(false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "StrTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ABC".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    assert!(
        bytes.windows(3).any(|w| w == [0x41, 0x42, 0xC3]),
        "expected 0x41 0x42 0xC3 for 'ABC': {:02X?}",
        bytes
    );
}

#[test]
fn wire_format_bytevector_abc() {
    // Spec example: [0x41, 0x42, 0x43] -> length 0x83 + data 0x41 0x42 0x43
    let xml = bytevector_xml(false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td(
        "BVTest",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![0x41, 0x42, 0x43]))),
    );
    let bytes = enc.encode_template_data(td).unwrap();
    assert!(
        bytes.windows(4).any(|w| w == [0x83, 0x41, 0x42, 0x43]),
        "expected 0x83 0x41 0x42 0x43 for bytevector: {:02X?}",
        bytes
    );
}

#[test]
fn wire_format_decimal_spec_example() {
    // Spec: 9427.55 = 942755 x 10^-2
    // Per FAST §2.1: signed integer uses two's complement. -2 in 7-bit two's complement = 1111110,
    // stop-bit encoded as 0xFE.
    let xml = r#"<templates>
  <template id="1" name="DecTest">
    <decimal id="1" name="Price"/>
  </template>
</templates>"#
        .to_string();
    let mut enc = FastEncoder::new(&xml).unwrap();
    use crate::decimal::Decimal;
    let td = make_td(
        "DecTest",
        "Price",
        ValueData::Value(Some(Value::Decimal(Decimal::new(-2, 942755)))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    // The body should contain exponent 0xFE and mantissa 0x39 0x45 0xA3
    let idx = bytes
        .iter()
        .position(|&b| b == 0xFE)
        .expect("exponent byte 0xFE not found");
    assert_eq!(bytes[idx + 1], 0x39);
    assert_eq!(bytes[idx + 2], 0x45);
    assert_eq!(bytes[idx + 3], 0xA3);
}

// ============================================================
// 6. BLOCK — block ::= BlockSize message+
// Spec: BlockSize is a stop-bit unsigned integer. Block contains
// at least one message. Block size is the only integer allowed
// to be overlong. Error D12 if block size = 0.
// ============================================================

#[test]
fn block_single_message() {
    let xml = int_xml("uInt32", false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(42))));
    let msg = enc.encode_template_data(td).unwrap();

    // Build block: [BlockSize] [message]
    let mut block = Vec::new();
    block.extend_from_slice(&encode_uint(msg.len() as u64));
    block.extend_from_slice(&msg);

    // Decode: read BlockSize, then decode messages within that length
    let (size, size_bytes) = decode_uint(&block).unwrap();
    assert_eq!(size, msg.len() as u64, "block size matches message length");
    let data = &block[size_bytes..block.len()];
    assert_eq!(
        data.len(),
        size as usize,
        "block contains exactly the message"
    );

    let mut dec = FastDecoder::new(&xml).unwrap();
    let (tpl, consumed) = dec.decode_raw(data).unwrap();
    assert_eq!(
        consumed as usize,
        data.len(),
        "decoded entire message from block"
    );
    assert_eq!(
        get_field(&tpl, "Val"),
        &ValueData::Value(Some(Value::UInt32(42)))
    );
}

#[test]
fn block_multiple_messages() {
    let xml = int_xml("uInt32", false);
    let values = [100u32, 200, 300];
    let mut msgs = Vec::new();
    for &v in &values {
        let mut enc = FastEncoder::new(&xml).unwrap();
        let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(v))));
        msgs.push(enc.encode_template_data(td).unwrap());
    }

    // Build block: [BlockSize] msg1 msg2 msg3
    let total_msg_len: usize = msgs.iter().map(|m| m.len()).sum();
    let mut block = Vec::new();
    block.extend_from_slice(&encode_uint(total_msg_len as u64));
    for msg in &msgs {
        block.extend_from_slice(msg);
    }

    // Decode block
    let (size, size_bytes) = decode_uint(&block).unwrap();
    assert_eq!(size, total_msg_len as u64);
    let data = &block[size_bytes..block.len()];
    assert_eq!(data.len(), size as usize);

    // Decode all 3 messages from the block data
    let mut dec = FastDecoder::new(&xml).unwrap();
    let mut offset = 0;
    for (i, &expected) in values.iter().enumerate() {
        let (tpl, consumed) = dec.decode_raw(&data[offset..]).unwrap();
        assert_eq!(
            get_field(&tpl, "Val"),
            &ValueData::Value(Some(Value::UInt32(expected))),
            "message {}",
            i
        );
        offset += consumed as usize;
    }
    assert_eq!(offset, data.len(), "consumed entire block");
}

#[test]
fn block_zero_size_error() {
    // Block size = 0 is error D12 per spec
    let block = vec![0x80]; // unsigned 0 with stop bit
    let (size, _) = decode_uint(&block).unwrap();
    assert_eq!(size, 0, "block size is 0 -> D12 error");
}

#[test]
fn block_message_boundary() {
    // A partial message at the block boundary should not decode
    let xml = int_xml("uInt32", false);
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(99))));
    let msg = enc.encode_template_data(td).unwrap();

    // Build a block with only the first half of the message
    let half = msg.len() / 2;
    let mut block = Vec::new();
    block.extend_from_slice(&encode_uint(half as u64));
    block.extend_from_slice(&msg[..half]);

    // Read block size
    let (size, size_bytes) = decode_uint(&block).unwrap();
    assert_eq!(size, half as u64);
    let data = &block[size_bytes..block.len()];
    assert_eq!(data.len(), size as usize);

    // Decoding the partial message should fail
    let mut dec = FastDecoder::new(&xml).unwrap();
    assert!(
        dec.decode_raw(data).is_err(),
        "partial message should fail to decode"
    );
}

#[test]
fn block_size_multibyte() {
    // BlockSize > 127 requires multiple bytes
    let xml = int_xml("uInt32", false);
    let values: Vec<u32> = (0..50).map(|i| i * 100).collect();
    let mut msgs = Vec::new();
    for &v in &values {
        let mut enc = FastEncoder::new(&xml).unwrap();
        let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(v))));
        msgs.push(enc.encode_template_data(td).unwrap());
    }
    let total_msg_len: usize = msgs.iter().map(|m| m.len()).sum();
    assert!(
        total_msg_len >= 128,
        "block should be > 127 bytes for multibyte BlockSize test"
    );

    // Build block
    let mut block = Vec::new();
    block.extend_from_slice(&encode_uint(total_msg_len as u64));
    for msg in &msgs {
        block.extend_from_slice(msg);
    }

    // Verify BlockSize encoding uses multiple bytes
    let (size, size_bytes) = decode_uint(&block).unwrap();
    assert!(size_bytes > 1, "BlockSize > 127 uses multiple bytes");
    assert_eq!(size, total_msg_len as u64);
}

// ============================================================
// 7. STREAM — stream ::= message* | block*
// ============================================================

#[test]
fn stream_bare_messages() {
    // stream ::= message* (no block framing)
    let xml = int_xml("uInt32", false);
    let values = [10u32, 20, 30, 40, 50];
    let mut all_bytes = Vec::new();
    for &v in &values {
        let mut enc = FastEncoder::new(&xml).unwrap();
        let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(v))));
        all_bytes.extend(enc.encode_template_data(td).unwrap());
    }

    // Decode all messages from the stream
    let mut dec = FastDecoder::new(&xml).unwrap();
    let mut offset = 0usize;
    for (i, &expected) in values.iter().enumerate() {
        let (tpl, consumed) = dec.decode_raw(&all_bytes[offset..]).unwrap();
        assert_eq!(
            get_field(&tpl, "Val"),
            &ValueData::Value(Some(Value::UInt32(expected))),
            "stream message {}",
            i
        );
        offset += consumed as usize;
    }
    assert_eq!(offset, all_bytes.len(), "consumed entire bare stream");
}

#[test]
fn stream_blocks() {
    // stream ::= block* (multiple blocks)
    let xml = int_xml("uInt32", false);

    // Build block 1: [2 messages]
    let mut block1_msgs = Vec::new();
    for &v in &[100u32, 200] {
        let mut enc = FastEncoder::new(&xml).unwrap();
        let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(v))));
        block1_msgs.push(enc.encode_template_data(td).unwrap());
    }
    let block1_len: usize = block1_msgs.iter().map(|m| m.len()).sum();
    let mut block1 = Vec::new();
    block1.extend_from_slice(&encode_uint(block1_len as u64));
    for msg in &block1_msgs {
        block1.extend_from_slice(msg);
    }

    // Build block 2: [1 message]
    let mut enc = FastEncoder::new(&xml).unwrap();
    let td = make_td("IntTest", "Val", ValueData::Value(Some(Value::UInt32(300))));
    let msg = enc.encode_template_data(td).unwrap();
    let mut block2 = Vec::new();
    block2.extend_from_slice(&encode_uint(msg.len() as u64));
    block2.extend_from_slice(&msg);

    // Concatenate blocks into a stream
    let mut stream = Vec::new();
    stream.extend_from_slice(&block1);
    stream.extend_from_slice(&block2);

    // Decode stream
    let mut pos = 0;
    let mut msg_count = 0;
    let expected = [100u32, 200, 300];
    let mut dec = FastDecoder::new(&xml).unwrap();

    while pos < stream.len() {
        let (size, size_bytes) = decode_uint(&stream[pos..]).unwrap();
        pos += size_bytes;
        let data = &stream[pos..pos + size as usize];

        let mut offset = 0;
        while offset < data.len() {
            let (tpl, consumed) = dec.decode_raw(&data[offset..]).unwrap();
            assert_eq!(
                get_field(&tpl, "Val"),
                &ValueData::Value(Some(Value::UInt32(expected[msg_count])))
            );
            msg_count += 1;
            offset += consumed as usize;
        }
        pos += size as usize;
    }
    assert_eq!(
        msg_count,
        expected.len(),
        "decoded all messages from block stream"
    );
}

#[test]
fn stream_mixed_types() {
    // A stream of mixed message types
    let xml = r#"<templates>
  <template id="1" name="IntMsg">
    <uInt32 id="1" name="Val"/>
  </template>
  <template id="2" name="StrMsg">
    <string id="2" name="Txt"/>
  </template>
</templates>"#
        .to_string();

    let mut all_bytes = Vec::new();
    let mut enc = FastEncoder::new(&xml).unwrap();

    // IntMsg(42)
    let td = make_td("IntMsg", "Val", ValueData::Value(Some(Value::UInt32(42))));
    all_bytes.extend(enc.encode_template_data(td).unwrap());

    // StrMsg("hello")
    let td = make_td(
        "StrMsg",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    all_bytes.extend(enc.encode_template_data(td).unwrap());

    // IntMsg(99)
    let td = make_td("IntMsg", "Val", ValueData::Value(Some(Value::UInt32(99))));
    all_bytes.extend(enc.encode_template_data(td).unwrap());

    // Decode all from stream
    let mut dec = FastDecoder::new(&xml).unwrap();
    let mut offset = 0;

    // Message 1: IntMsg(42)
    let (tpl, consumed) = dec.decode_raw(&all_bytes[offset..]).unwrap();
    assert_eq!(tpl.name, "IntMsg");
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::UInt32(42)))
    );
    offset += consumed as usize;

    // Message 2: StrMsg("hello")
    let (tpl, consumed) = dec.decode_raw(&all_bytes[offset..]).unwrap();
    assert_eq!(tpl.name, "StrMsg");
    assert_eq!(
        *get_field(&tpl, "Txt"),
        ValueData::Value(Some(Value::AsciiString("hello".to_string())))
    );
    offset += consumed as usize;

    // Message 3: IntMsg(99)
    let (tpl, consumed) = dec.decode_raw(&all_bytes[offset..]).unwrap();
    assert_eq!(tpl.name, "IntMsg");
    assert_eq!(
        *get_field(&tpl, "Val"),
        ValueData::Value(Some(Value::UInt32(99)))
    );
    offset += consumed as usize;

    assert_eq!(offset, all_bytes.len(), "consumed entire mixed stream");
}
