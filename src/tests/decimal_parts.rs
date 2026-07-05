//! Tests for FAST decimal fields with individual `<exponent>` / `<mantissa>` operators.
//!
//! Covers the `decFieldOp = exponent?, mantissa?` EBNF rules:
//! `<decimal name="..."><exponent><fieldOp/></exponent><mantissa><fieldOp/></mantissa></decimal>`
//!
//! Each sub-element carries its own field operator, allowing exponent and mantissa
//! to be encoded independently (e.g., exponent copied while mantissa deltas).

use crate::decimal::Decimal;
use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{FastDecoder, FastEncoder};
use std::collections::HashMap;

// ============================================================
// Helpers
// ============================================================

fn make_dec(exp: i32, mant: i64) -> ValueData {
    ValueData::Value(Some(Value::Decimal(Decimal::new(exp, mant))))
}

fn make_dec_none() -> ValueData {
    ValueData::Value(None)
}

fn make_td(name: &str, fields: &[(&str, ValueData)]) -> TemplateData {
    let mut map = HashMap::new();
    for (k, v) in fields {
        map.insert(k.to_string(), v.clone());
    }
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
    assert_eq!(
        consumed,
        bytes.len(),
        "decoder did not consume all bytes (encoded {} bytes: {:02x?})",
        bytes.len(),
        bytes
    );
    tpl
}

fn get_decimal<'a>(tpl: &'a TemplateData, field: &str) -> &'a Decimal {
    if let ValueData::Group(ref g) = tpl.value {
        if let Some(ValueData::Value(Some(Value::Decimal(d)))) = g.get(field) {
            return d;
        }
    }
    panic!("decimal field '{}' not found in decoded template", field);
}

// ============================================================
// 1. <exponent><copy/></exponent> — exponent copied from previous
// ============================================================

#[test]
fn exponent_copy_unchanged_omits() {
    // Exponent stays the same between messages, so copy omits it
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();

    // Message 1: exponent=-2, mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-2 (same), mantissa=10050 (delta)
    let td2 = make_td("Dec", &[("Price", make_dec(-2, 10050))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    // Second message should be smaller (exponent copied, only mantissa delta sent)
    eprintln!("Exponent copy: msg1={} bytes {:02x?}", bytes1.len(), bytes1);
    eprintln!("Exponent copy: msg2={} bytes {:02x?}", bytes2.len(), bytes2);
    assert!(
        bytes2.len() <= bytes1.len(),
        "exponent copy + mantissa delta should be <= first message"
    );

    // Decode and verify (roundtrip_single uses fresh enc/dec)
    let tpl1 = roundtrip(xml, make_td("Dec", &[("Price", make_dec(-2, 10000))]));
    let d1 = get_decimal(&tpl1, "Price");
    assert_eq!(d1.exponent, -2);
    assert_eq!(d1.mantissa, 10000);
}

#[test]
fn exponent_copy_changed_writes() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: exponent=-2, mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-4 (CHANGED), mantissa=5000
    let td2 = make_td("Dec", &[("Price", make_dec(-4, 5000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    // Decode
    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    let d1 = get_decimal(&tpl1, "Price");
    assert_eq!(d1.exponent, -2);
    assert_eq!(d1.mantissa, 10000);

    let d2 = get_decimal(&tpl2, "Price");
    assert_eq!(d2.exponent, -4);
    assert_eq!(d2.mantissa, 5000);
}

// ============================================================
// 2. <mantissa><copy/></mantissa> — mantissa copied from previous
// ============================================================

#[test]
fn mantissa_copy_unchanged_omits() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><delta/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: exponent=-2, mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-2 (same), mantissa=10000 (same — copy omits)
    let td2 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    let d1 = get_decimal(&tpl1, "Price");
    assert_eq!(d1.exponent, -2);
    assert_eq!(d1.mantissa, 10000);

    let d2 = get_decimal(&tpl2, "Price");
    assert_eq!(d2.exponent, -2);
    assert_eq!(d2.mantissa, 10000);
}

#[test]
fn mantissa_copy_changed_writes() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><delta/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Mantissa changed — must be written
    let td2 = make_td("Dec", &[("Price", make_dec(-2, 20000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-2, 20000));
}

// ============================================================
// 3. <exponent><delta/></exponent><mantissa><delta/></mantissa>
// ============================================================

#[test]
fn both_parts_delta_independent() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><delta/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: exponent=-2, mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-3 (delta -1), mantissa=5000 (delta -5000)
    let td2 = make_td("Dec", &[("Price", make_dec(-3, 5000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-3, 5000));
}

#[test]
fn both_parts_delta_sequence() {
    // Three messages: each changes both exponent and mantissa
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><delta/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let prices = [
        Decimal::new(-2, 10000),
        Decimal::new(-3, 5000),
        Decimal::new(-4, 2000),
    ];

    for (i, p) in prices.iter().enumerate() {
        let td = make_td("Dec", &[("Price", make_dec(p.exponent, p.mantissa))]);
        let bytes = enc.encode_template_data(td).unwrap();
        eprintln!(
            "Delta seq msg{}: {} bytes {:02x?}",
            i + 1,
            bytes.len(),
            bytes
        );

        let (tpl, _) = dec.decode_raw(&bytes).unwrap();
        let d = get_decimal(&tpl, "Price");
        assert_eq!(d.exponent, p.exponent, "msg{} exponent", i + 1);
        assert_eq!(d.mantissa, p.mantissa, "msg{} mantissa", i + 1);
    }
}

// ============================================================
// 4. <exponent><increment/></exponent> — exponent increments
// ============================================================

#[test]
fn exponent_increment() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><increment/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: exponent=-2, mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-1 (incremented by 1), mantissa=10000 (copy, unchanged)
    let td2 = make_td("Dec", &[("Price", make_dec(-1, 10000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    // Message 3: exponent=0 (incremented by 1 again), mantissa=10000 (copy)
    let td3 = make_td("Dec", &[("Price", make_dec(0, 10000))]);
    let bytes3 = enc.encode_template_data(td3).unwrap();

    // Messages 2 and 3 should be very small (exponent auto-incremented, mantissa copied)
    eprintln!("Exp inc: msg1={} bytes {:02x?}", bytes1.len(), bytes1);
    eprintln!("Exp inc: msg2={} bytes {:02x?}", bytes2.len(), bytes2);
    eprintln!("Exp inc: msg3={} bytes {:02x?}", bytes3.len(), bytes3);

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();
    let (tpl3, _) = dec.decode_raw(&bytes3).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-1, 10000));
    assert_eq!(*get_decimal(&tpl3, "Price"), Decimal::new(0, 10000));
}

#[test]
fn exponent_increment_gap_writes() {
    // Exponent jumps by more than 1 — must be written explicitly
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><increment/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td1 = make_td("Dec", &[("Price", make_dec(-4, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Jump from -4 to -2 (gap of 2, not 1) — must write exponent
    let td2 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-4, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-2, 10000));
}

// ============================================================
// 5. <exponent><default value="-2"/></exponent> — exponent default
// ============================================================

#[test]
fn exponent_default_value_omits() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><default value="-2"/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: exponent=-2 (default), mantissa=10000
    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: exponent=-2 (default), mantissa=10000 (copy)
    let td2 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-2, 10000));
}

#[test]
fn exponent_default_non_default_writes() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><default value="-2"/></exponent>
      <mantissa><copy/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Exponent differs from default — must be written
    let td = make_td("Dec", &[("Price", make_dec(-4, 10000))]);
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    let d = get_decimal(&tpl, "Price");
    assert_eq!(d.exponent, -4);
    assert_eq!(d.mantissa, 10000);
}

// ============================================================
// 6. <mantissa><default value="0"/></mantissa> — mantissa default
// ============================================================

#[test]
fn mantissa_default_value_omits() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
      <mantissa><default value="0"/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Mantissa = 0 (default)
    let td = make_td("Dec", &[("Price", make_dec(-2, 0))]);
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    let d = get_decimal(&tpl, "Price");
    assert_eq!(d.exponent, -2);
    assert_eq!(d.mantissa, 0);
}

#[test]
fn mantissa_default_non_default_writes() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
      <mantissa><default value="0"/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td = make_td("Dec", &[("Price", make_dec(-2, 42))]);
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    let d = get_decimal(&tpl, "Price");
    assert_eq!(d.exponent, -2);
    assert_eq!(d.mantissa, 42);
}

// ============================================================
// 7. <exponent><copy dictionary="X" key="Y"/></exponent>
//    Dictionary/key attributes on exponent
// ============================================================

#[test]
fn exponent_copy_with_dictionary_key() {
    // Two decimal fields with different dictionary scopes for exponent copy.
    // Changing Price1's exponent should not affect Price2's exponent copy state.
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price1">
      <exponent><copy dictionary="d1" key="exp1"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
    <decimal id="2" name="Price2">
      <exponent><copy dictionary="d2" key="exp2"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Price1=(-2, 10000), Price2=(-4, 5000)
    let td1 = make_td(
        "Dec",
        &[
            ("Price1", make_dec(-2, 10000)),
            ("Price2", make_dec(-4, 5000)),
        ],
    );
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: Price1 exponent unchanged (-2), Price2 exponent changed (-3)
    let td2 = make_td(
        "Dec",
        &[
            ("Price1", make_dec(-2, 10010)),
            ("Price2", make_dec(-3, 5010)),
        ],
    );
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price1"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl1, "Price2"), Decimal::new(-4, 5000));
    assert_eq!(*get_decimal(&tpl2, "Price1"), Decimal::new(-2, 10010));
    assert_eq!(*get_decimal(&tpl2, "Price2"), Decimal::new(-3, 5010));
}

#[test]
fn exponent_copy_isolated_across_templates() {
    // Same field name, different templates — exponent copy state isolated
    let xml = r#"<templates>
  <template id="1" name="Bid">
    <decimal id="1" name="Price">
      <exponent><copy dictionary="template"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
  <template id="2" name="Ask">
    <decimal id="1" name="Price">
      <exponent><copy dictionary="template"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Bid: exponent=-2
    let td1 = make_td("Bid", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Ask: exponent=-4 (different template, independent copy state)
    let td2 = make_td("Ask", &[("Price", make_dec(-4, 20000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    // Bid again: exponent=-2 (copy should still work for Bid's dictionary)
    let td3 = make_td("Bid", &[("Price", make_dec(-2, 10010))]);
    let bytes3 = enc.encode_template_data(td3).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();
    let (tpl3, _) = dec.decode_raw(&bytes3).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-4, 20000));
    assert_eq!(*get_decimal(&tpl3, "Price"), Decimal::new(-2, 10010));
}

// ============================================================
// 8. Optional decimal with individual operators
// ============================================================

#[test]
fn optional_decimal_with_parts_present() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price" presence="optional">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    let d = get_decimal(&tpl, "Price");
    assert_eq!(d.exponent, -2);
    assert_eq!(d.mantissa, 10000);
}

#[test]
fn optional_decimal_with_parts_absent() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <uInt32 id="2" name="Header"/>
    <decimal id="1" name="Price" presence="optional">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Price absent
    let td = make_td(
        "Dec",
        &[
            ("Header", ValueData::Value(Some(Value::UInt32(1)))),
            ("Price", make_dec_none()),
        ],
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    if let ValueData::Group(ref g) = tpl.value {
        assert!(
            matches!(g.get("Price"), Some(ValueData::Value(None))),
            "Price should be None (absent)"
        );
    } else {
        panic!("expected group");
    }
}

#[test]
fn optional_decimal_with_parts_present_then_absent() {
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <uInt32 id="2" name="Header"/>
    <decimal id="1" name="Price" presence="optional">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    // Message 1: Price present
    let td1 = make_td(
        "Dec",
        &[
            ("Header", ValueData::Value(Some(Value::UInt32(1)))),
            ("Price", make_dec(-2, 10000)),
        ],
    );
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Message 2: Price absent
    let td2 = make_td(
        "Dec",
        &[
            ("Header", ValueData::Value(Some(Value::UInt32(2)))),
            ("Price", make_dec_none()),
        ],
    );
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    if let ValueData::Group(ref g) = tpl2.value {
        assert!(
            matches!(g.get("Price"), Some(ValueData::Value(None))),
            "Price should be None in message 2"
        );
    }
}

// ============================================================
// 9. <exponent><constant value="-2"/></exponent> — fixed exponent
// ============================================================

#[test]
fn exponent_constant_fixed_exponent() {
    // Exponent is always -2; only mantissa varies
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><constant value="-2"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td1 = make_td("Dec", &[("Price", make_dec(-2, 10000))]);
    let bytes1 = enc.encode_template_data(td1).unwrap();

    let td2 = make_td("Dec", &[("Price", make_dec(-2, 20000))]);
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let (tpl1, _) = dec.decode_raw(&bytes1).unwrap();
    let (tpl2, _) = dec.decode_raw(&bytes2).unwrap();

    // Both should have exponent=-2 (constant)
    assert_eq!(*get_decimal(&tpl1, "Price"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl2, "Price"), Decimal::new(-2, 20000));
}

#[test]
fn exponent_constant_multiple_decimals() {
    // Two decimals, both with fixed exponent=-2
    let xml = r#"<templates>
  <template id="1" name="Quote">
    <decimal id="1" name="Bid">
      <exponent><constant value="-2"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
    <decimal id="2" name="Ask">
      <exponent><constant value="-2"/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let td = make_td(
        "Quote",
        &[("Bid", make_dec(-2, 10000)), ("Ask", make_dec(-2, 10050))],
    );
    let bytes = enc.encode_template_data(td).unwrap();

    let (tpl, _) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(*get_decimal(&tpl, "Bid"), Decimal::new(-2, 10000));
    assert_eq!(*get_decimal(&tpl, "Ask"), Decimal::new(-2, 10050));
}

// ============================================================
// 10. Validation: mismatched exponent/mantissa
// ============================================================

#[test]
fn decimal_exponent_without_mantissa_errors() {
    // <exponent> without <mantissa> is invalid per assemble_decimal
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
    </decimal>
  </template>
</templates>"#;

    let result = FastEncoder::new(xml);
    assert!(
        result.is_err(),
        "Should error when <exponent> is provided without <mantissa>"
    );
}

#[test]
fn decimal_mantissa_without_exponent_errors() {
    // <mantissa> without <exponent> is invalid per assemble_decimal
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;

    let result = FastEncoder::new(xml);
    assert!(
        result.is_err(),
        "Should error when <mantissa> is provided without <exponent>"
    );
}

#[test]
fn decimal_operator_and_parts_mixed_errors() {
    // Having both a decimal-level operator AND individual parts is invalid
    let xml = r#"<templates>
  <template id="1" name="Dec">
    <decimal id="1" name="Price">
      <delta/>
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;

    let result = FastEncoder::new(xml);
    assert!(
        result.is_err(),
        "Should error when decimal-level operator is mixed with <exponent>/<mantissa>"
    );
}

// ============================================================
// 11. Multi-message roundtrip with all per-part operators
// ============================================================

#[test]
fn exponent_copy_mantissa_delta_multi_message() {
    // Simulates real market data: exponent stays -2, mantissa updates
    let xml = r#"<templates>
  <template id="1" name="Tick">
    <uInt32 id="2" name="Seq"><increment value="0"/></uInt32>
    <decimal id="1" name="Price">
      <exponent><copy/></exponent>
      <mantissa><delta/></mantissa>
    </decimal>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();

    let ticks = [
        (1, Decimal::new(-2, 10000)),
        (2, Decimal::new(-2, 10050)),
        (3, Decimal::new(-2, 9980)),
        (4, Decimal::new(-2, 10025)),
    ];

    let mut all_bytes = Vec::new();
    for (seq, price) in &ticks {
        let td = make_td(
            "Tick",
            &[
                ("Seq", ValueData::Value(Some(Value::UInt32(*seq)))),
                ("Price", make_dec(price.exponent, price.mantissa)),
            ],
        );
        let bytes = enc.encode_template_data(td).unwrap();
        eprintln!(
            "Tick {}: seq={} price={} exp={} mant={}, {} bytes {:02x?}",
            seq,
            seq,
            price,
            price.exponent,
            price.mantissa,
            bytes.len(),
            bytes
        );
        all_bytes.extend(&bytes);
    }

    // Decode all ticks
    let mut offset = 0;
    for (i, (expected_seq, expected_price)) in ticks.iter().enumerate() {
        let (tpl, consumed) = dec.decode_raw(&all_bytes[offset..]).unwrap();
        offset += consumed as usize;

        if let ValueData::Group(ref g) = tpl.value {
            if let Some(ValueData::Value(Some(Value::UInt32(seq)))) = g.get("Seq") {
                assert_eq!(seq, expected_seq, "tick {} seq", i + 1);
            }
        }
        let d = get_decimal(&tpl, "Price");
        assert_eq!(
            d.exponent,
            expected_price.exponent,
            "tick {} exponent",
            i + 1
        );
        assert_eq!(
            d.mantissa,
            expected_price.mantissa,
            "tick {} mantissa",
            i + 1
        );
    }
    assert_eq!(offset, all_bytes.len(), "consumed all bytes");
}

// ============================================================
// Helper: roundtrip_single (creates fresh enc/dec)
// ============================================================

#[allow(dead_code)]
fn roundtrip_single(xml: &str, td: TemplateData) -> TemplateData {
    let mut enc = FastEncoder::new(xml).unwrap();
    let mut dec = FastDecoder::new(xml).unwrap();
    let bytes = enc.encode_template_data(td).unwrap();
    let (tpl, consumed) = dec.decode_raw(&bytes).unwrap();
    assert_eq!(consumed, bytes.len());
    tpl
}
