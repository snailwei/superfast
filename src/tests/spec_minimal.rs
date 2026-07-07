//! Round-trip tests for the FAST spec's minimal template covering all XML features.
//!
//! The spec defines a "MarketData" template with a "Header" fragment that exercises:
//! templateRef, typeRef, all integer types, decimal (single + individual operators),
//! ASCII/Unicode strings, byte vectors, length handles, groups, sequences,
//! all six operators, and dictionary scoping.

use crate::decimal::Decimal;
use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{Dictionary, FastDecoder, FastEncoder};
use std::collections::HashMap;

fn make_val(v: Value) -> ValueData {
    ValueData::Value(Some(v))
}

fn make_none() -> ValueData {
    ValueData::Value(None)
}

fn make_td(fields: &[(&str, ValueData)]) -> TemplateData {
    let mut map = HashMap::new();
    for (name, value) in fields {
        map.insert(name.to_string(), value.clone());
    }
    TemplateData {
        name: "MarketData".to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    }
}

fn roundtrip(xml: &str, td: TemplateData) -> TemplateData {
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();
    let bytes = enc.encode_template_data(td).unwrap();
    let (tpl, consumed) = dec.parse(&bytes).unwrap();
    assert_eq!(
        consumed,
        bytes.len(),
        "decoder did not consume all bytes (encoded {} bytes)",
        bytes.len()
    );
    tpl
}

// ============================================================
// Minimal spec XML — stripped to what our parser supports.
// Our parser strips typeRef elements, ignores namespaces, and
// doesn't validate templateNs/dictionary attributes.
// ============================================================

fn spec_xml() -> String {
    r#"<templates>
  <!-- Header fragment (reused via static templateRef) -->
  <template id="1" name="Header">
    <string id="1" name="BeginString">
      <constant value="FIX4.4"/>
    </string>
    <string id="2" name="MessageType">
      <constant value="X"/>
    </string>
    <uInt32 id="3" name="MsgSeqNum">
      <increment value="1"/>
    </uInt32>
    <string id="4" name="SenderCompID">
      <copy/>
    </string>
  </template>

  <!-- Main message template -->
  <template id="100" name="MarketData">
    <!-- Reuse header via static template reference -->
    <templateRef name="Header"/>

    <!-- Mandatory field, no operator -->
    <uInt64 id="10" name="Timestamp"/>

    <!-- Optional field, no operator -->
    <int64 id="11" name="SeqNo" presence="optional"/>

    <!-- Default operator -->
    <int32 id="12" name="MarketSegment" presence="optional">
      <default value="0"/>
    </int32>

    <!-- Delta operator on decimal -->
    <decimal id="13" name="BidPrice">
      <delta value="100"/>
    </decimal>

    <!-- Optional delta on decimal -->
    <decimal id="14" name="AskPrice" presence="optional">
      <delta/>
    </decimal>

    <!-- Decimal with individual exponent/mantissa operators -->
    <decimal id="15" name="LastPrice">
      <exponent><copy dictionary="template"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>

    <!-- Copy with custom dictionary, explicit key, initial value -->
    <string id="16" name="Symbol">
      <copy dictionary="symDict" key="sym" value=""/>
    </string>

    <!-- Tail operator -->
    <string id="17" name="SecurityDesc">
      <tail/>
    </string>

    <!-- Unicode string with length handle and copy -->
    <string id="18" name="Note" charset="unicode">
      <length name="NoteLength"/>
      <copy/>
    </string>

    <!-- Byte vector with length handle and delta -->
    <byteVector id="19" name="RawData">
      <length name="RawDataLength"/>
      <delta/>
    </byteVector>

    <!-- Optional group -->
    <group id="20" name="ExtendedAttributes" presence="optional"
           dictionary="type">
      <string id="21" name="AttrKey">
        <copy/>
      </string>
      <string id="22" name="AttrValue">
        <copy/>
      </string>
    </group>

    <!-- Sequence with explicit-length name and operator -->
    <sequence id="23" name="MDEntries" presence="optional">
      <length id="24" name="NoMDEntries">
        <delta/>
      </length>
      <uInt32 id="25" name="MDUpdateAction">
        <copy/>
      </uInt32>
      <decimal id="26" name="MDEntryPx">
        <delta/>
      </decimal>
      <decimal id="27" name="MDEntrySize">
        <delta/>
      </decimal>
    </sequence>
  </template>
</templates>"#
        .to_string()
}

// ============================================================
// 1. Basic round-trip — first message establishes all context
// ============================================================

#[test]
fn spec_minimal_first_message() {
    let xml = spec_xml();
    let seq = ValueData::Sequence(vec![ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MDUpdateAction".to_string(), make_val(Value::UInt32(0))); // Add
        m.insert(
            "MDEntryPx".to_string(),
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 12345,
            })),
        );
        m.insert(
            "MDEntrySize".to_string(),
            make_val(Value::Decimal(Decimal {
                exponent: 0,
                mantissa: 100,
            })),
        );
        m
    })]);

    let header = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(1))); // increment from 1 → 1
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCHANGE".to_string())),
        );
        m
    });
    let td = make_td(&[
        ("Header", header),
        ("Timestamp", make_val(Value::UInt64(1700000000000))),
        ("SeqNo", make_none()), // optional, absent
        ("MarketSegment", make_val(Value::Int32(1))),
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        (
            "AskPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10050,
            })),
        ),
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10025,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("AAPL".to_string()))),
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Apple Inc".to_string())),
        ),
        (
            "Note",
            make_val(Value::UnicodeString("Test note".to_string())),
        ),
        ("RawData", make_val(Value::Bytes(vec![0x01, 0x02, 0x03]))),
        ("ExtendedAttributes", ValueData::None), // optional group, absent
        ("MDEntries", seq),
    ]);

    let tpl = roundtrip(&xml, td);

    // Verify decoded values
    if let ValueData::Group(g) = &tpl.value {
        // Header: constants are implicit, check increment and copy
        if let Some(ValueData::Value(Some(Value::UInt32(seq)))) = g.get("MsgSeqNum") {
            assert_eq!(*seq, 1, "MsgSeqNum should be 1 (increment from initial 1)");
        } else {
            panic!("MsgSeqNum missing");
        }
        if let Some(ValueData::Value(Some(Value::AsciiString(sender)))) = g.get("SenderCompID") {
            assert_eq!(sender.as_str(), "EXCHANGE");
        } else {
            panic!("SenderCompID missing");
        }
        // MarketData
        if let Some(ValueData::Value(Some(Value::UInt64(ts)))) = g.get("Timestamp") {
            assert_eq!(*ts, 1700000000000);
        } else {
            panic!("Timestamp missing");
        }
        if let Some(ValueData::Value(Some(Value::AsciiString(sym)))) = g.get("Symbol") {
            assert_eq!(sym.as_str(), "AAPL");
        } else {
            panic!("Symbol missing");
        }
        if let Some(ValueData::Value(Some(Value::AsciiString(desc)))) = g.get("SecurityDesc") {
            assert_eq!(desc.as_str(), "Apple Inc");
        } else {
            panic!("SecurityDesc missing");
        }
    } else {
        panic!("expected group");
    }
}

// ============================================================
// 2. Second message — tests all compression operators
//    Copy omits unchanged, increment auto-increments, delta sends diff,
//    tail sends suffix only, default omits when equal to initial.
// ============================================================

#[test]
fn spec_minimal_second_message_compression() {
    let xml = spec_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // --- Message 1: establish context ---
    let seq1 = ValueData::Sequence(vec![ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MDUpdateAction".to_string(), make_val(Value::UInt32(0)));
        m.insert(
            "MDEntryPx".to_string(),
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        );
        m.insert(
            "MDEntrySize".to_string(),
            make_val(Value::Decimal(Decimal {
                exponent: 0,
                mantissa: 100,
            })),
        );
        m
    })]);

    let header1 = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(1)));
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCHANGE".to_string())),
        );
        m
    });
    let td1 = make_td(&[
        ("Header", header1),
        ("Timestamp", make_val(Value::UInt64(1700000000000))),
        ("SeqNo", make_none()),
        ("MarketSegment", make_val(Value::Int32(1))),
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        (
            "AskPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10050,
            })),
        ),
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10025,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("AAPL".to_string()))),
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Apple Inc".to_string())),
        ),
        (
            "Note",
            make_val(Value::UnicodeString("Test note".to_string())),
        ),
        ("RawData", make_val(Value::Bytes(vec![0x01, 0x02, 0x03]))),
        ("ExtendedAttributes", ValueData::None),
        ("MDEntries", seq1),
    ]);
    let bytes1 = enc.encode_template_data(td1).unwrap();
    let first_len = bytes1.len();

    // --- Message 2: test compression ---
    // MsgSeqNum: increment auto-increments 1 → 2 (omit, let increment handle it)
    // SenderCompID: copy, unchanged "EXCHANGE" (omit)
    // Timestamp: mandatory, new value (send)
    // MarketSegment: default, value = 0 = initial (omit)
    // BidPrice: delta, 10000 → 10010 (send delta)
    // Symbol: copy, unchanged "AAPL" (omit)
    // SecurityDesc: tail, "Apple Inc" → "Apple Inc." (suffix change, send tail)
    // Note: copy, unchanged (omit)
    // RawData: delta, unchanged (omit)
    // ExtendedAttributes: optional group, present with data (send)
    // MDEntries: sequence with length delta 1 → 2
    let ext_attrs = ValueData::Group({
        let mut m = HashMap::new();
        m.insert(
            "AttrKey".to_string(),
            make_val(Value::AsciiString("currency".to_string())),
        );
        m.insert(
            "AttrValue".to_string(),
            make_val(Value::AsciiString("USD".to_string())),
        );
        m
    });

    let seq2 = ValueData::Sequence(vec![
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("MDUpdateAction".to_string(), make_val(Value::UInt32(0)));
            m.insert(
                "MDEntryPx".to_string(),
                make_val(Value::Decimal(Decimal {
                    exponent: -2,
                    mantissa: 10010,
                })),
            );
            m.insert(
                "MDEntrySize".to_string(),
                make_val(Value::Decimal(Decimal {
                    exponent: 0,
                    mantissa: 200,
                })),
            );
            m
        }),
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("MDUpdateAction".to_string(), make_val(Value::UInt32(1))); // Delete
            m.insert(
                "MDEntryPx".to_string(),
                make_val(Value::Decimal(Decimal {
                    exponent: -2,
                    mantissa: 9999,
                })),
            );
            m.insert(
                "MDEntrySize".to_string(),
                make_val(Value::Decimal(Decimal {
                    exponent: 0,
                    mantissa: 50,
                })),
            );
            m
        }),
    ]);

    let header2 = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(2)));
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCHANGE".to_string())),
        );
        m
    });
    let td2 = make_td(&[
        ("Header", header2),
        ("Timestamp", make_val(Value::UInt64(1700000000001))),
        ("SeqNo", make_none()),
        ("MarketSegment", make_val(Value::Int32(0))), // = initial, omit
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10010,
            })),
        ),
        ("AskPrice", make_none()), // optional, absent
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10030,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("AAPL".to_string()))), // unchanged
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Apple Inc.".to_string())),
        ), // tail: "Apple Inc" → "Apple Inc."
        (
            "Note",
            make_val(Value::UnicodeString("Test note".to_string())),
        ), // unchanged
        ("RawData", make_val(Value::Bytes(vec![0x01, 0x02, 0x03]))),  // unchanged
        ("ExtendedAttributes", ext_attrs),
        ("MDEntries", seq2),
    ]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    // Message 2 should be smaller than message 1 (compression)
    eprintln!(
        "Spec minimal: msg1={} bytes, msg2={} bytes (compression {}%)",
        first_len,
        bytes2.len(),
        (100 - (bytes2.len() * 100 / first_len.max(1))) as usize
    );
    assert!(
        bytes2.len() <= first_len,
        "Second message should be <= first due to compression operators"
    );

    // Decode and verify
    let (_data1, _) = dec.parse(&bytes1).unwrap();
    let (data2, _) = dec.parse(&bytes2).unwrap();

    if let ValueData::Group(g) = &data2.value {
        // MsgSeqNum incremented
        if let Some(ValueData::Value(Some(Value::UInt32(seq)))) = g.get("MsgSeqNum") {
            assert_eq!(*seq, 2);
        } else {
            panic!("MsgSeqNum missing");
        }
        // SenderCompID copied from previous
        if let Some(ValueData::Value(Some(Value::AsciiString(s)))) = g.get("SenderCompID") {
            assert_eq!(s.as_str(), "EXCHANGE");
        } else {
            panic!("SenderCompID missing");
        }
        // MarketSegment defaulted to 0
        if let Some(ValueData::Value(Some(Value::Int32(seg)))) = g.get("MarketSegment") {
            assert_eq!(*seg, 0);
        } else {
            panic!("MarketSegment missing");
        }
        // SecurityDesc: tail updated "Apple Inc" → "Apple Inc."
        if let Some(ValueData::Value(Some(Value::AsciiString(d)))) = g.get("SecurityDesc") {
            assert_eq!(d.as_str(), "Apple Inc.");
        } else {
            panic!("SecurityDesc missing");
        }
        // ExtendedAttributes: optional group present
        if let Some(ValueData::Group(agg)) = g.get("ExtendedAttributes") {
            if let Some(ValueData::Value(Some(Value::AsciiString(v)))) = agg.get("AttrKey") {
                assert_eq!(v.as_str(), "currency");
            } else {
                panic!("AttrKey missing");
            }
        } else {
            panic!("ExtendedAttributes group missing");
        }
    } else {
        panic!("expected group");
    }
}

// ============================================================
// 3. Optional group absent + sequence absent
// ============================================================

#[test]
fn spec_minimal_optional_fields_absent() {
    let xml = spec_xml();
    let header = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(1)));
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCHANGE".to_string())),
        );
        m
    });
    let td = make_td(&[
        ("Header", header),
        ("Timestamp", make_val(Value::UInt64(1700000000000))),
        ("SeqNo", make_none()),                       // optional absent
        ("MarketSegment", make_val(Value::Int32(0))), // default = initial, omit
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("AskPrice", make_none()), // optional absent
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("GOOG".to_string()))),
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Google".to_string())),
        ),
        ("Note", make_val(Value::UnicodeString("".to_string()))), // empty unicode
        ("RawData", make_val(Value::Bytes(vec![]))),              // empty byte vector
        ("ExtendedAttributes", ValueData::None),                  // optional group absent
        ("MDEntries", ValueData::Sequence(vec![])),               // empty sequence
    ]);

    let tpl = roundtrip(&xml, td);
    if let ValueData::Group(g) = &tpl.value {
        if let Some(ValueData::Value(Some(Value::AsciiString(sym)))) = g.get("Symbol") {
            assert_eq!(sym.as_str(), "GOOG");
        } else {
            panic!("Symbol missing");
        }
    } else {
        panic!("expected group");
    }
}

// ============================================================
// 4. Copy with custom dictionary isolation
//    Two different templates with the same field name but different
//    dictionary scopes should not share copy state.
// ============================================================

#[test]
fn spec_minimal_copy_dictionary_isolation() {
    // The spec uses dictionary="symDict" key="sym" for Symbol.
    // Verify that Symbol copy state is isolated from SenderCompID copy state
    // (different dictionaries).
    let xml = spec_xml();
    let mut enc = FastEncoder::new(&xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(&xml, Dictionary::Global).unwrap();

    // Message 1
    let header1 = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(1)));
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCH1".to_string())),
        );
        m
    });
    let td1 = make_td(&[
        ("Header", header1),
        ("Timestamp", make_val(Value::UInt64(100))),
        ("SeqNo", make_none()),
        ("MarketSegment", make_val(Value::Int32(0))),
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("AskPrice", make_none()),
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("AAPL".to_string()))),
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Apple".to_string())),
        ),
        ("Note", make_val(Value::UnicodeString("".to_string()))),
        ("RawData", make_val(Value::Bytes(vec![]))),
        ("ExtendedAttributes", ValueData::None),
        ("MDEntries", ValueData::Sequence(vec![])),
    ]);
    let bytes1 = enc.encode_template_data(td1).unwrap();
    dec.parse(&bytes1).unwrap();

    // Message 2: change SenderCompID but keep Symbol
    let header2 = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("MsgSeqNum".to_string(), make_val(Value::UInt32(2)));
        m.insert(
            "SenderCompID".to_string(),
            make_val(Value::AsciiString("EXCH2".to_string())),
        );
        m
    });
    let td2 = make_td(&[
        ("Header", header2),
        ("Timestamp", make_val(Value::UInt64(101))),
        ("SeqNo", make_none()),
        ("MarketSegment", make_val(Value::Int32(0))),
        (
            "BidPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("AskPrice", make_none()),
        (
            "LastPrice",
            make_val(Value::Decimal(Decimal {
                exponent: -2,
                mantissa: 10000,
            })),
        ),
        ("Symbol", make_val(Value::AsciiString("AAPL".to_string()))), // unchanged
        (
            "SecurityDesc",
            make_val(Value::AsciiString("Apple".to_string())),
        ),
        ("Note", make_val(Value::UnicodeString("".to_string()))),
        ("RawData", make_val(Value::Bytes(vec![]))),
        ("ExtendedAttributes", ValueData::None),
        ("MDEntries", ValueData::Sequence(vec![])),
    ]);
    let bytes2 = enc.encode_template_data(td2).unwrap();
    let (data2, _) = dec.parse(&bytes2).unwrap();

    if let ValueData::Group(g) = &data2.value {
        // SenderCompID changed to EXCH2
        if let Some(ValueData::Value(Some(Value::AsciiString(s)))) = g.get("SenderCompID") {
            assert_eq!(s.as_str(), "EXCH2");
        } else {
            panic!("SenderCompID missing");
        }
        // Symbol still AAPL (copy from previous, different dictionary)
        if let Some(ValueData::Value(Some(Value::AsciiString(s)))) = g.get("Symbol") {
            assert_eq!(s.as_str(), "AAPL");
        } else {
            panic!("Symbol missing");
        }
    } else {
        panic!("expected group");
    }
}
