//! Tests for FAST operators: Copy, Default, Increment, Tail.
//!
//! These are the compression operators that determine whether a field's
//! value is written or omitted on the wire.
//!
//! All round-trips use serde deserialization (via `decode_buffer`),
//! which reconstructs omitted values from context/defaults — matching
//! real-world usage.

use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{Dictionary, FastDecoder, FastEncoder};
use std::rc::Rc;

fn make_td(name: &str, field: &str, value: ValueData) -> TemplateData {
    let mut vec = Vec::new();
    vec.push((Rc::from(field), value));
    TemplateData {
        name: name.to_string(),
        value: ValueData::Group(vec),
        pmap_bytes: None,
    }
}

// ============================================================
// 1. COPY — value is written only when it changes from previous
// ============================================================

fn copy_xml() -> String {
    r#"<templates>
  <template id="1" name="CopyTest">
    <uInt32 id="1" name="Seq" presence="optional"><copy value="0"/></uInt32>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum CopyMessage {
    #[serde(rename = "CopyTest")]
    CopyTest(CopyTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct CopyTestMsg {
    #[serde(rename = "Seq", default)]
    seq: Option<u32>,
}

#[test]
fn copy_first_message_writes_value() {
    let xml = copy_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td("CopyTest", "Seq", ValueData::Value(Some(Value::UInt32(42))));
    let bytes = enc.encode_template_data(td).unwrap();

    eprintln!("Copy first (42): {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (CopyMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        CopyMessage::CopyTest(m) => assert_eq!(m.seq, Some(42)),
    }
}

#[test]
fn copy_unchanged_omits_field() {
    let xml = copy_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "CopyTest",
        "Seq",
        ValueData::Value(Some(Value::UInt32(100))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "CopyTest",
        "Seq",
        ValueData::Value(Some(Value::UInt32(100))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Copy first (100):  {:02x?}", first);
    eprintln!("Copy second (100): {:02x?}", second);
    assert!(
        second.len() < first.len(),
        "unchanged copy field should produce shorter message ({} < {})",
        second.len(),
        first.len()
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (CopyMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (CopyMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (CopyMessage::CopyTest(m1), CopyMessage::CopyTest(m2)) => {
            assert_eq!(m1.seq, Some(100));
            assert_eq!(m2.seq, Some(100));
        }
    }
}

#[test]
fn copy_changed_writes_field() {
    let xml = copy_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "CopyTest",
        "Seq",
        ValueData::Value(Some(Value::UInt32(100))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "CopyTest",
        "Seq",
        ValueData::Value(Some(Value::UInt32(200))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Copy first (100):  {:02x?}", first);
    eprintln!("Copy changed (200): {:02x?}", second);

    let (msg1, _): (CopyMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (CopyMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (CopyMessage::CopyTest(m1), CopyMessage::CopyTest(m2)) => {
            assert_eq!(m1.seq, Some(100));
            assert_eq!(m2.seq, Some(200));
        }
    }
}

#[test]
fn copy_context_reset() {
    let xml = copy_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td("CopyTest", "Seq", ValueData::Value(Some(Value::UInt32(50))));
    enc.encode_template_data(td).unwrap();

    enc.reset();

    let td = make_td("CopyTest", "Seq", ValueData::Value(Some(Value::UInt32(50))));
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    dec.reset();
    let (msg, _): (CopyMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        CopyMessage::CopyTest(m) => assert_eq!(m.seq, Some(50)),
    }
}

#[test]
fn copy_with_zero_value() {
    let xml = r#"<templates>
  <template id="1" name="CopyZero">
    <uInt32 id="1" name="Val" presence="optional"><copy value="10"/></uInt32>
  </template>
</templates>"#
        .to_string();

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    enum CopyZeroMessage {
        #[serde(rename = "CopyZero")]
        CopyZero(CopyZeroMsg),
    }

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    struct CopyZeroMsg {
        #[serde(rename = "Val", default)]
        val: Option<u32>,
    }

    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td("CopyZero", "Val", ValueData::Value(Some(Value::UInt32(0))));
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (CopyZeroMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        CopyZeroMessage::CopyZero(m) => assert_eq!(m.val, Some(0)),
    }
}

// ============================================================
// 2. DEFAULT — value is written only when it differs from default
// ============================================================

fn default_xml() -> String {
    r#"<templates>
  <template id="2" name="DefaultTest">
    <int32 id="2" name="Status" presence="optional"><default value="0"/></int32>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum DefaultMessage {
    #[serde(rename = "DefaultTest")]
    DefaultTest(DefaultTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct DefaultTestMsg {
    #[serde(rename = "Status", default)]
    status: Option<i32>,
}

#[test]
fn default_value_omits_field() {
    let xml = default_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultTest",
        "Status",
        ValueData::Value(Some(Value::Int32(0))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    eprintln!("Default (value=0): {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultMessage::DefaultTest(m) => assert_eq!(m.status, Some(0)),
    }
}

#[test]
fn default_non_zero_writes_field() {
    let xml = default_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultTest",
        "Status",
        ValueData::Value(Some(Value::Int32(5))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultMessage::DefaultTest(m) => assert_eq!(m.status, Some(5)),
    }
}

#[test]
fn default_negative_value_writes_field() {
    let xml = default_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultTest",
        "Status",
        ValueData::Value(Some(Value::Int32(-1))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultMessage::DefaultTest(m) => assert_eq!(m.status, Some(-1)),
    }
}

// ============================================================
// 3. INCREMENT — value is omitted when it equals previous + 1
// ============================================================

fn increment_xml() -> String {
    r#"<templates>
  <template id="3" name="IncTest">
    <uInt32 id="3" name="SeqNum" presence="optional"><increment value="0"/></uInt32>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum IncMessage {
    #[serde(rename = "IncTest")]
    IncTest(IncTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct IncTestMsg {
    #[serde(rename = "SeqNum", default)]
    seq_num: Option<u32>,
}

#[test]
fn increment_first_message_writes_value() {
    let xml = increment_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "IncTest",
        "SeqNum",
        ValueData::Value(Some(Value::UInt32(10))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    eprintln!("Inc first (10): {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (IncMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        IncMessage::IncTest(m) => assert_eq!(m.seq_num, Some(10)),
    }
}

#[test]
fn increment_expected_omits_value() {
    let xml = increment_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "IncTest",
        "SeqNum",
        ValueData::Value(Some(Value::UInt32(10))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "IncTest",
        "SeqNum",
        ValueData::Value(Some(Value::UInt32(11))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Inc first (10):  {:02x?}", first);
    eprintln!("Inc second (11): {:02x?}", second);
    assert!(
        second.len() < first.len(),
        "incremented field should produce shorter message ({} < {})",
        second.len(),
        first.len()
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (IncMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (IncMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (IncMessage::IncTest(m1), IncMessage::IncTest(m2)) => {
            assert_eq!(m1.seq_num, Some(10));
            assert_eq!(m2.seq_num, Some(11));
        }
    }
}

#[test]
fn increment_gap_writes_value() {
    let xml = increment_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "IncTest",
        "SeqNum",
        ValueData::Value(Some(Value::UInt32(10))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "IncTest",
        "SeqNum",
        ValueData::Value(Some(Value::UInt32(15))),
    );
    let second = enc.encode_template_data(td).unwrap();

    let (msg1, _): (IncMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (IncMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (IncMessage::IncTest(m1), IncMessage::IncTest(m2)) => {
            assert_eq!(m1.seq_num, Some(10));
            assert_eq!(m2.seq_num, Some(15));
        }
    }
}

#[test]
fn increment_sequence_roundtrip() {
    let xml = increment_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let mut all_bytes = Vec::new();
    for i in 1..=5u32 {
        let td = make_td(
            "IncTest",
            "SeqNum",
            ValueData::Value(Some(Value::UInt32(i))),
        );
        let bytes = enc.encode_template_data(td).unwrap();
        eprintln!("Inc seq {}: {:02x?}", i, bytes);
        all_bytes.extend(&bytes);
    }

    let mut offset = 0;
    for expected in 1..=5u32 {
        let (msg, consumed): (IncMessage, u64) = dec.decode(&all_bytes[offset..]).unwrap();
        match msg {
            IncMessage::IncTest(m) => {
                assert_eq!(m.seq_num, Some(expected), "expected {}", expected);
            }
        }
        offset += consumed as usize;
    }
}

// ============================================================
// 4. TAIL — for strings, only the new suffix is written
// ============================================================

fn tail_xml() -> String {
    r#"<templates>
  <template id="4" name="TailTest">
    <string id="4" name="Txt" presence="optional"><tail/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum TailMessage {
    #[serde(rename = "TailTest")]
    TailTest(TailTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct TailTestMsg {
    #[serde(rename = "Txt", default)]
    txt: Option<String>,
}

#[test]
fn tail_first_message_writes_full() {
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    eprintln!("Tail first: {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (TailMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        TailMessage::TailTest(m) => assert_eq!(m.txt, Some("hello".to_string())),
    }
}

#[test]
fn tail_unchanged_omits_field() {
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Tail first:  {:02x?}", first);
    eprintln!("Tail second: {:02x?}", second);
    assert!(
        second.len() < first.len(),
        "unchanged tail field should be shorter"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (TailMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailMessage::TailTest(m1), TailMessage::TailTest(m2)) => {
            assert_eq!(m1.txt, Some("hello".to_string()));
            assert_eq!(m2.txt, Some("hello".to_string()));
        }
    }
}

#[test]
fn tail_extended_writes_suffix_only() {
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello world".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Tail first:     {:02x?}", first);
    eprintln!("Tail extended:  {:02x?}", second);

    let (msg1, _): (TailMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailMessage::TailTest(m1), TailMessage::TailTest(m2)) => {
            assert_eq!(m1.txt, Some("hello".to_string()));
            assert_eq!(m2.txt, Some("hello world".to_string()));
        }
    }
}

#[test]
fn tail_completely_different_writes_full() {
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("abc".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("xyz".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    let (msg1, _): (TailMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailMessage::TailTest(m1), TailMessage::TailTest(m2)) => {
            assert_eq!(m1.txt, Some("abc".to_string()));
            assert_eq!(m2.txt, Some("xyz".to_string()));
        }
    }
}

#[test]
fn tail_longer_than_base_returns_tail_value() {
    // §4.8: "If tail length ≥ base length, result = tail value"
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // Message 1: short base "ab" (2 chars)
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("ab".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();
    let (msg1, _): (TailMessage, u64) = dec.decode(&first).unwrap();
    match &msg1 {
        TailMessage::TailTest(m) => assert_eq!(m.txt, Some("ab".to_string())),
    }

    // Message 2: longer value "xyzw" (4 chars) — tail_len > base_len
    // Result should be "xyzw" (tail value replaces everything)
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("xyzw".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();
    let (msg2, _): (TailMessage, u64) = dec.decode(&second).unwrap();
    match &msg2 {
        TailMessage::TailTest(m) => {
            assert_eq!(
                m.txt,
                Some("xyzw".to_string()),
                "tail_len (4) > base_len (2): result should be tail value"
            );
        }
    }
}

#[test]
fn tail_equal_length_full_replacement() {
    // §4.8 edge: tail_len == base_len — full replacement
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // Message 1: base "abc" (3 chars)
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("abc".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();
    let (msg1, _): (TailMessage, u64) = dec.decode(&first).unwrap();
    match &msg1 {
        TailMessage::TailTest(m) => assert_eq!(m.txt, Some("abc".to_string())),
    }

    // Message 2: same length "xyz" — result must be "xyz"
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("xyz".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();
    let (msg2, _): (TailMessage, u64) = dec.decode(&second).unwrap();
    match &msg2 {
        TailMessage::TailTest(m) => assert_eq!(m.txt, Some("xyz".to_string())),
    }
}

#[test]
fn tail_with_empty_string() {
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("hello".to_string()))),
    );
    enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString("".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (msg, _): (TailMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        TailMessage::TailTest(m) => assert_eq!(m.txt, Some("".to_string())),
    }
}

// ============================================================
// 5. MULTI-FIELD — multiple operators in one message
// ============================================================

fn multi_xml() -> String {
    r#"<templates>
  <template id="10" name="MultiTest">
    <uInt32 id="10" name="Seq" presence="optional"><copy value="0"/></uInt32>
    <int32 id="11" name="Status" presence="optional"><default value="0"/></int32>
    <uInt32 id="12" name="OrderNum" presence="optional"><increment value="0"/></uInt32>
    <string id="13" name="Symbol" presence="optional"><tail/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum MultiMessage {
    #[serde(rename = "MultiTest")]
    MultiTest(MultiTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct MultiTestMsg {
    #[serde(rename = "Seq", default)]
    seq: Option<u32>,
    #[serde(rename = "Status", default)]
    status: Option<i32>,
    #[serde(rename = "OrderNum", default)]
    order_num: Option<u32>,
    #[serde(rename = "Symbol", default)]
    symbol: Option<String>,
}

fn make_multi(seq: u32, status: i32, order: u32, symbol: &str) -> TemplateData {
    let mut vec = Vec::new();
    vec.push((
        Rc::from("Seq"),
        ValueData::Value(Some(Value::UInt32(seq))),
    ));
    vec.push((
        Rc::from("Status"),
        ValueData::Value(Some(Value::Int32(status))),
    ));
    vec.push((
        Rc::from("OrderNum"),
        ValueData::Value(Some(Value::UInt32(order))),
    ));
    vec.push((
        Rc::from("Symbol"),
        ValueData::Value(Some(Value::AsciiString(symbol.to_string()))),
    ));
    TemplateData {
        name: "MultiTest".to_string(),
        value: ValueData::Group(vec),
        pmap_bytes: None,
    }
}

#[test]
fn multi_field_all_compressed() {
    let xml = multi_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let first = enc
        .encode_template_data(make_multi(1, 0, 100, "AAPL"))
        .unwrap();

    // All fields compressed:
    // Seq=1 (copy, unchanged) / Status=0 (default) / OrderNum=101 (increment) / Symbol="AAPL" (tail)
    let second = enc
        .encode_template_data(make_multi(1, 0, 101, "AAPL"))
        .unwrap();

    eprintln!("Multi first:  {:02x?}", first);
    eprintln!("Multi second: {:02x?}", second);

    assert!(
        second.len() < first.len(),
        "all fields compressed should be shorter"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (MultiMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (MultiMessage, u64) = dec.decode(&second).unwrap();

    match (&msg1, &msg2) {
        (MultiMessage::MultiTest(m1), MultiMessage::MultiTest(m2)) => {
            assert_eq!(m1.seq, Some(1));
            assert_eq!(m2.seq, Some(1));
            assert_eq!(m1.order_num, Some(100));
            assert_eq!(m2.order_num, Some(101));
            assert_eq!(m1.symbol, Some("AAPL".to_string()));
            assert_eq!(m2.symbol, Some("AAPL".to_string()));
        }
    }
}

#[test]
fn multi_field_all_changed() {
    let xml = multi_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let first = enc
        .encode_template_data(make_multi(1, 0, 100, "AAPL"))
        .unwrap();
    eprintln!("Multi first: {:02x?}", first);

    // All changed: Seq=2 / Status=5 / OrderNum=105 / Symbol="GOOG"
    let second = enc
        .encode_template_data(make_multi(2, 5, 105, "GOOG"))
        .unwrap();
    eprintln!("Multi second: {:02x?}", second);

    let (msg1, _): (MultiMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (MultiMessage, u64) = dec.decode(&second).unwrap();

    match (&msg1, &msg2) {
        (MultiMessage::MultiTest(m1), MultiMessage::MultiTest(m2)) => {
            assert_eq!(m1.seq, Some(1));
            assert_eq!(m2.seq, Some(2));
            assert_eq!(m2.status, Some(5));
            assert_eq!(m1.order_num, Some(100));
            assert_eq!(m2.order_num, Some(105));
            assert_eq!(m2.symbol, Some("GOOG".to_string()));
        }
    }
}

// ============================================================
// 6. WIRE-BYTE SIZE COMPARISONS
// ============================================================

#[test]
fn copy_compression_size() {
    // 100 identical messages — first writes value, rest omit it
    let xml = copy_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let first = enc
        .encode_template_data(make_td(
            "CopyTest",
            "Seq",
            ValueData::Value(Some(Value::UInt32(42))),
        ))
        .unwrap();
    let mut all_bytes = first.clone();

    for _ in 1..100 {
        let td = make_td("CopyTest", "Seq", ValueData::Value(Some(Value::UInt32(42))));
        let bytes = enc.encode_template_data(td).unwrap();
        all_bytes.extend(&bytes);
    }

    let avg = all_bytes.len() as f64 / 100.0;
    eprintln!(
        "100 copy messages: first={} bytes, total={} bytes, avg={:.1} bytes/msg",
        first.len(),
        all_bytes.len(),
        avg
    );
    assert!(first.len() > 2, "first message should write the value");
    assert!(
        avg < first.len() as f64,
        "average should be less than first message size"
    );
}

#[test]
fn increment_compression_size() {
    // 100 sequential increments — first writes value (doesn't match initial+1),
    // subsequent increments are omitted
    let xml = increment_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    // Start with 5 (not initial 0 + 1), so first message writes the value
    let first = enc
        .encode_template_data(make_td(
            "IncTest",
            "SeqNum",
            ValueData::Value(Some(Value::UInt32(5))),
        ))
        .unwrap();
    let mut all_bytes = first.clone();

    for i in 6..=104u32 {
        let td = make_td(
            "IncTest",
            "SeqNum",
            ValueData::Value(Some(Value::UInt32(i))),
        );
        let bytes = enc.encode_template_data(td).unwrap();
        all_bytes.extend(&bytes);
    }

    let avg = all_bytes.len() as f64 / 100.0;
    eprintln!(
        "100 increment messages: first={} bytes, total={} bytes, avg={:.1} bytes/msg",
        first.len(),
        all_bytes.len(),
        avg
    );
    assert!(first.len() > 2, "first message should write the value");
    assert!(
        avg < first.len() as f64,
        "average should be less than first message size"
    );
}

// ============================================================
// 7. OPTIONAL FIELDS (no operator)
// ============================================================

#[test]
fn optional_field_present() {
    let xml = r#"<templates>
  <template id="20" name="OptTest">
    <uInt32 id="20" name="Val" presence="optional"/>
  </template>
</templates>"#
        .to_string();

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    enum OptMessage {
        #[serde(rename = "OptTest")]
        OptTest(OptTestMsg),
    }

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    struct OptTestMsg {
        #[serde(rename = "Val", default)]
        val: Option<u32>,
    }

    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td("OptTest", "Val", ValueData::Value(Some(Value::UInt32(42))));
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (OptMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        OptMessage::OptTest(m) => assert_eq!(m.val, Some(42)),
    }
}

#[test]
fn optional_field_absent() {
    let xml = r#"<templates>
  <template id="20" name="OptTest">
    <uInt32 id="20" name="Val" presence="optional"/>
  </template>
</templates>"#
        .to_string();

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    enum OptMessage {
        #[serde(rename = "OptTest")]
        OptTest(OptTestMsg),
    }

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    struct OptTestMsg {
        #[serde(rename = "Val", default)]
        val: Option<u32>,
    }

    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    // Don't include "Val" in the template data — it's absent
    let td = TemplateData {
        name: "OptTest".to_string(),
        value: ValueData::Group(Vec::new()),
        pmap_bytes: None,
    };
    let bytes = enc.encode_template_data(td).unwrap();
    eprintln!("Optional absent: {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (OptMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        OptMessage::OptTest(m) => assert_eq!(m.val, None),
    }
}

// ============================================================
// 8. ENCODING SIZE TESTS (no decode, just wire format)
// ============================================================

#[test]
fn tail_compression_ratio() {
    // Long string that gets compressed by tail operator
    let xml = tail_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let long = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString(long.to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    // Unchanged — should be tiny (pmap + template id only)
    let td = make_td(
        "TailTest",
        "Txt",
        ValueData::Value(Some(Value::AsciiString(long.to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("Tail long first:  {} bytes {:02x?}", first.len(), first);
    eprintln!("Tail long second: {} bytes {:02x?}", second.len(), second);

    assert_eq!(
        first.len(),
        long.len() + 2,
        "first message: pmap(1) + tid(1) + string({})",
        long.len()
    );
    assert!(
        second.len() <= 2,
        "unchanged tail should be just pmap + tid"
    );
}

#[test]
fn multi_operator_compression() {
    // Realistic scenario: order book updates where most fields don't change
    let xml = multi_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    // Initial full snapshot
    let first = enc
        .encode_template_data(make_multi(1, 0, 1, "AAPL"))
        .unwrap();
    eprintln!("Multi full:   {} bytes {:02x?}", first.len(), first);

    // Incremental update: only Seq stays same (copy), Status=0 (default),
    // OrderNum increments (increment), Symbol same (tail)
    let second = enc
        .encode_template_data(make_multi(1, 0, 2, "AAPL"))
        .unwrap();
    eprintln!("Multi delta:  {} bytes {:02x?}", second.len(), second);

    assert!(
        second.len() <= 2,
        "all-fields-compressed should be just pmap + tid"
    );
}

// ============================================================
// 9. TAIL on UnicodeString and Bytes (distinct code paths)
// ============================================================

fn tail_unicode_xml() -> String {
    r#"<templates>
  <template id="30" name="TailUnicode">
    <string id="30" name="Txt" charset="unicode" presence="optional"><tail/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum TailUnicodeMessage {
    #[serde(rename = "TailUnicode")]
    TailUnicode(TailUnicodeMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct TailUnicodeMsg {
    #[serde(rename = "Txt", default)]
    txt: Option<String>,
}

#[test]
fn tail_unicode_first_writes_full() {
    let xml = tail_unicode_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "TailUnicode",
        "Txt",
        ValueData::Value(Some(Value::UnicodeString("hello".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (TailUnicodeMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        TailUnicodeMessage::TailUnicode(m) => assert_eq!(m.txt, Some("hello".to_string())),
    }
}

#[test]
fn tail_unicode_unchanged_omits() {
    let xml = tail_unicode_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "TailUnicode",
        "Txt",
        ValueData::Value(Some(Value::UnicodeString("hello".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailUnicode",
        "Txt",
        ValueData::Value(Some(Value::UnicodeString("hello".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    assert!(
        second.len() <= 2,
        "unchanged unicode tail should be just pmap + tid"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (TailUnicodeMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailUnicodeMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailUnicodeMessage::TailUnicode(m1), TailUnicodeMessage::TailUnicode(m2)) => {
            assert_eq!(m1.txt, Some("hello".to_string()));
            assert_eq!(m2.txt, Some("hello".to_string()));
        }
    }
}

fn tail_bytes_xml() -> String {
    r#"<templates>
  <template id="31" name="TailBytes">
    <byteVector id="31" name="Data" presence="optional"><tail/></byteVector>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum TailBytesMessage {
    #[serde(rename = "TailBytes")]
    TailBytes(TailBytesMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct TailBytesMsg {
    #[serde(rename = "Data", default, deserialize_with = "deserialize_bytes")]
    data: Option<Vec<u8>>,
}

fn deserialize_bytes<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_bytes::Deserialize as _;
    Ok(serde_bytes::ByteBuf::deserialize(deserializer)
        .map(|b| Some(b.to_vec()))
        .unwrap_or(None))
}

#[test]
fn tail_bytes_first_writes_full() {
    let xml = tail_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "TailBytes",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![0x01, 0x02, 0x03, 0xFF]))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (TailBytesMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        TailBytesMessage::TailBytes(m) => assert_eq!(m.data, Some(vec![0x01, 0x02, 0x03, 0xFF])),
    }
}

#[test]
fn tail_bytes_unchanged_omits() {
    let xml = tail_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let data = vec![0xDE, 0xAD, 0xBE, 0xEF];
    let td = make_td(
        "TailBytes",
        "Data",
        ValueData::Value(Some(Value::Bytes(data.clone()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "TailBytes",
        "Data",
        ValueData::Value(Some(Value::Bytes(data.clone()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    assert!(
        second.len() <= 2,
        "unchanged bytes tail should be just pmap + tid"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (TailBytesMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailBytesMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailBytesMessage::TailBytes(m1), TailBytesMessage::TailBytes(m2)) => {
            assert_eq!(m1.data, Some(data.clone()));
            assert_eq!(m2.data, Some(data));
        }
    }
}

#[test]
fn tail_bytes_suffix_replacement() {
    // Verify tail semantics: replace last N bytes where N = tail length
    let xml = tail_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // Base: [0x01, 0x02, 0x03, 0x04, 0x05] (5 bytes)
    let td = make_td(
        "TailBytes",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![1, 2, 3, 4, 5]))),
    );
    let first = enc.encode_template_data(td).unwrap();

    // New: [0x01, 0x02, 0xAB, 0xCD] — common prefix [01,02], tail [AB,CD] (2 bytes)
    // Decoder: keep base[0..5-2]=base[0..3]=[01,02,03], append [AB,CD] → [01,02,03,AB,CD]
    // BUT encoder uses longest-common-prefix approach, so tail = [AB,CD]
    // Decoder removes 2 from end of base: [01,02,03] + [AB,CD] = [01,02,03,AB,CD]
    // This doesn't match [01,02,AB,CD]!
    // For tail to work, the replacement must match FAST semantics:
    // New = [01,02,03,AB,CD] would give tail [AB,CD] → decoder: [01,02,03]+[AB,CD] = [01,02,03,AB,CD] ✓
    let td = make_td(
        "TailBytes",
        "Data",
        ValueData::Value(Some(Value::Bytes(vec![1, 2, 3, 0xAB, 0xCD]))),
    );
    let second = enc.encode_template_data(td).unwrap();
    eprintln!("Tail bytes base: {:02x?}", first);
    eprintln!("Tail bytes modified: {:02x?}", second);

    let (msg1, _): (TailBytesMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (TailBytesMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (TailBytesMessage::TailBytes(m1), TailBytesMessage::TailBytes(m2)) => {
            assert_eq!(m1.data, Some(vec![1, 2, 3, 4, 5]));
            assert_eq!(m2.data, Some(vec![1, 2, 3, 0xAB, 0xCD]));
        }
    }
}

// ============================================================
// 10. CONSTANT — value is always the constant defined in schema
// ============================================================

fn constant_xml() -> String {
    r#"<templates>
  <template id="40" name="ConstTest">
    <uInt32 id="40" name="MsgType"><constant value="42"/></uInt32>
    <int32 id="41" name="Status"><constant value="-1"/></int32>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum ConstMessage {
    #[serde(rename = "ConstTest")]
    ConstTest(ConstTestMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct ConstTestMsg {
    #[serde(rename = "MsgType")]
    msg_type: u32,
    #[serde(rename = "Status")]
    status: i32,
}

#[test]
fn constant_mandatory_fields() {
    let xml = constant_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    // Constant fields don't need values in input — encoder uses schema constant
    let td = TemplateData {
        name: "ConstTest".to_string(),
        value: ValueData::Group(Vec::new()),
        pmap_bytes: None,
    };
    let bytes = enc.encode_template_data(td).unwrap();
    eprintln!("Constant message: {:02x?}", bytes);

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (ConstMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        ConstMessage::ConstTest(m) => {
            assert_eq!(m.msg_type, 42);
            assert_eq!(m.status, -1);
        }
    }
}

fn constant_optional_xml() -> String {
    r#"<templates>
  <template id="41" name="ConstOpt">
    <uInt32 id="40" name="MsgType"><constant value="99"/></uInt32>
    <uInt32 id="41" name="Seq" presence="optional"><copy value="0"/></uInt32>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum ConstOptMessage {
    #[serde(rename = "ConstOpt")]
    ConstOpt(ConstOptMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct ConstOptMsg {
    #[serde(rename = "MsgType")]
    msg_type: u32,
    #[serde(rename = "Seq", default)]
    seq: Option<u32>,
}

#[test]
fn constant_with_optional() {
    let xml = constant_optional_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // First message with optional field present
    let td = make_td("ConstOpt", "Seq", ValueData::Value(Some(Value::UInt32(10))));
    let first = enc.encode_template_data(td).unwrap();

    // Second message: optional field omitted (copy = unchanged)
    let td = make_td("ConstOpt", "Seq", ValueData::Value(Some(Value::UInt32(10))));
    let second = enc.encode_template_data(td).unwrap();

    eprintln!("ConstOpt first:  {:02x?}", first);
    eprintln!("ConstOpt second: {:02x?}", second);

    let (msg1, _): (ConstOptMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (ConstOptMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (ConstOptMessage::ConstOpt(m1), ConstOptMessage::ConstOpt(m2)) => {
            // Constant field always present
            assert_eq!(m1.msg_type, 99);
            assert_eq!(m2.msg_type, 99);
            // Copy field
            assert_eq!(m1.seq, Some(10));
            assert_eq!(m2.seq, Some(10));
        }
    }
}

fn constant_string_xml() -> String {
    r#"<templates>
  <template id="42" name="ConstStr">
    <string id="42" name="Type"><constant value="MARKET_DATA"/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum ConstStrMessage {
    #[serde(rename = "ConstStr")]
    ConstStr(ConstStrMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct ConstStrMsg {
    #[serde(rename = "Type")]
    type_field: String,
}

#[test]
fn constant_string_field() {
    let xml = constant_string_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = TemplateData {
        name: "ConstStr".to_string(),
        value: ValueData::Group(Vec::new()),
        pmap_bytes: None,
    };
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (ConstStrMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        ConstStrMessage::ConstStr(m) => assert_eq!(m.type_field, "MARKET_DATA"),
    }
}

#[test]
fn constant_negative_integer() {
    let xml = r#"<templates>
  <template id="43" name="ConstNeg">
    <int32 id="43" name="Code"><constant value="-999"/></int32>
  </template>
</templates>"#
        .to_string();

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    enum ConstNegMessage {
        #[serde(rename = "ConstNeg")]
        ConstNeg(ConstNegMsg),
    }

    #[derive(Debug, Clone, PartialEq, serde::Deserialize)]
    struct ConstNegMsg {
        #[serde(rename = "Code")]
        code: i32,
    }

    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = TemplateData {
        name: "ConstNeg".to_string(),
        value: ValueData::Group(Vec::new()),
        pmap_bytes: None,
    };
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (ConstNegMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        ConstNegMessage::ConstNeg(m) => assert_eq!(m.code, -999),
    }
}

// ============================================================
// 11. COPY on Strings (distinct extraction path from integers)
// ============================================================

fn copy_string_xml() -> String {
    r#"<templates>
  <template id="50" name="CopyStr">
    <string id="50" name="Symbol" presence="optional"><copy value=""/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum CopyStrMessage {
    #[serde(rename = "CopyStr")]
    CopyStr(CopyStrMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct CopyStrMsg {
    #[serde(rename = "Symbol", default)]
    symbol: Option<String>,
}

#[test]
fn copy_string_unchanged_omits() {
    let xml = copy_string_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "CopyStr",
        "Symbol",
        ValueData::Value(Some(Value::AsciiString("AAPL".to_string()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "CopyStr",
        "Symbol",
        ValueData::Value(Some(Value::AsciiString("AAPL".to_string()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    assert!(
        second.len() < first.len(),
        "copy on string should omit unchanged value"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (CopyStrMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (CopyStrMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (CopyStrMessage::CopyStr(m1), CopyStrMessage::CopyStr(m2)) => {
            assert_eq!(m1.symbol, Some("AAPL".to_string()));
            assert_eq!(m2.symbol, Some("AAPL".to_string()));
        }
    }
}

#[test]
fn copy_string_changed_writes() {
    let xml = copy_string_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    let td = make_td(
        "CopyStr",
        "Symbol",
        ValueData::Value(Some(Value::AsciiString("AAPL".to_string()))),
    );
    enc.encode_template_data(td).unwrap();

    let td = make_td(
        "CopyStr",
        "Symbol",
        ValueData::Value(Some(Value::AsciiString("GOOG".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (msg, _): (CopyStrMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        CopyStrMessage::CopyStr(m) => assert_eq!(m.symbol, Some("GOOG".to_string())),
    }
}

// ============================================================
// 12. COPY on Bytes (distinct extraction path)
// ============================================================

fn copy_bytes_xml() -> String {
    r#"<templates>
  <template id="51" name="CopyBV">
    <byteVector id="51" name="Data" presence="optional"><copy value=""/></byteVector>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum CopyBVMessage {
    #[serde(rename = "CopyBV")]
    CopyBV(CopyBVMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct CopyBVMsg {
    #[serde(rename = "Data", default, deserialize_with = "deserialize_bytes")]
    data: Option<Vec<u8>>,
}

#[test]
fn copy_bytes_unchanged_omits() {
    let xml = copy_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();

    let data = vec![0xCA, 0xFE];
    let td = make_td(
        "CopyBV",
        "Data",
        ValueData::Value(Some(Value::Bytes(data.clone()))),
    );
    let first = enc.encode_template_data(td).unwrap();

    let td = make_td(
        "CopyBV",
        "Data",
        ValueData::Value(Some(Value::Bytes(data.clone()))),
    );
    let second = enc.encode_template_data(td).unwrap();

    assert!(
        second.len() < first.len(),
        "copy on bytes should omit unchanged value"
    );

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg1, _): (CopyBVMessage, u64) = dec.decode(&first).unwrap();
    let (msg2, _): (CopyBVMessage, u64) = dec.decode(&second).unwrap();
    match (&msg1, &msg2) {
        (CopyBVMessage::CopyBV(m1), CopyBVMessage::CopyBV(m2)) => {
            assert_eq!(m1.data, Some(vec![0xCA, 0xFE]));
            assert_eq!(m2.data, Some(vec![0xCA, 0xFE]));
        }
    }
}

// ============================================================
// 13. DEFAULT on Strings (distinct from default on integers)
// ============================================================

fn default_string_xml() -> String {
    r#"<templates>
  <template id="60" name="DefaultStr">
    <string id="60" name="Region" presence="optional"><default value="US"/></string>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum DefaultStrMessage {
    #[serde(rename = "DefaultStr")]
    DefaultStr(DefaultStrMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct DefaultStrMsg {
    #[serde(rename = "Region", default)]
    region: Option<String>,
}

#[test]
fn default_string_value_omits() {
    let xml = default_string_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultStr",
        "Region",
        ValueData::Value(Some(Value::AsciiString("US".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultStrMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultStrMessage::DefaultStr(m) => assert_eq!(m.region, Some("US".to_string())),
    }
}

#[test]
fn default_string_non_default_writes() {
    let xml = default_string_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultStr",
        "Region",
        ValueData::Value(Some(Value::AsciiString("EU".to_string()))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultStrMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultStrMessage::DefaultStr(m) => assert_eq!(m.region, Some("EU".to_string())),
    }
}

// ============================================================
// 14. DEFAULT on Bytes
// ============================================================

fn default_bytes_xml() -> String {
    r#"<templates>
  <template id="61" name="DefaultBV">
    <byteVector id="61" name="Flag" presence="optional"><default value="AA"/></byteVector>
  </template>
</templates>"#
        .to_string()
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
enum DefaultBVMessage {
    #[serde(rename = "DefaultBV")]
    DefaultBV(DefaultBVMsg),
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
struct DefaultBVMsg {
    #[serde(rename = "Flag", default, deserialize_with = "deserialize_bytes")]
    flag: Option<Vec<u8>>,
}

#[test]
fn default_bytes_value_omits() {
    let xml = default_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultBV",
        "Flag",
        ValueData::Value(Some(Value::Bytes(vec![0xAA]))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultBVMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultBVMessage::DefaultBV(m) => assert_eq!(m.flag, Some(vec![0xAA])),
    }
}

#[test]
fn default_bytes_non_default_writes() {
    let xml = default_bytes_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let td = make_td(
        "DefaultBV",
        "Flag",
        ValueData::Value(Some(Value::Bytes(vec![0xBB, 0xCC]))),
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();
    let (msg, _): (DefaultBVMessage, u64) = dec.decode(&bytes).unwrap();
    match msg {
        DefaultBVMessage::DefaultBV(m) => assert_eq!(m.flag, Some(vec![0xBB, 0xCC])),
    }
}
