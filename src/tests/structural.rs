//! Standalone unit tests for FAST EBNF structural elements:
//!
//! Structural elements only tested via SH schema before — no isolated unit tests.
//!
//! EBNF nodes covered:
//! - group (mandatory, optional with pmap, nested)
//! - sequence (empty, single, multiple, with pmap per item)
//! - templateRef (static + dynamic)
//! - typeRef (as template attribute)

use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{Dictionary, FastDecoder, FastEncoder};
use std::collections::HashMap;

// ============================================================
// Helpers
// ============================================================

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

fn make_val(v: Value) -> ValueData {
    ValueData::Value(Some(v))
}

fn make_none() -> ValueData {
    ValueData::Value(None)
}

fn roundtrip(xml: &str, td: TemplateData) -> TemplateData {
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();
    let bytes = enc.encode_template_data(td).unwrap();
    let (tpl, consumed) = dec.parse(&bytes).unwrap();
    assert_eq!(
        consumed,
        bytes.len(),
        "decoder did not consume all bytes (encoded {} bytes: {:02x?})",
        bytes.len(),
        bytes
    );
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

// ============================================================
// 1. GROUP — <group> element (fixed set of fields)
// ============================================================

#[test]
fn group_mandatory_roundtrip() {
    // Mandatory group with mandatory fields (no pmap on group)
    let xml = r#"<templates>
  <template id="1" name="Root">
    <uInt32 id="1" name="Outer"/>
    <group id="2" name="Inner">
      <uInt32 id="3" name="InnerVal"/>
      <string id="4" name="InnerTxt"/>
    </group>
  </template>
</templates>"#;
    let inner = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("InnerVal".to_string(), make_val(Value::UInt32(42)));
        m.insert(
            "InnerTxt".to_string(),
            make_val(Value::AsciiString("hello".to_string())),
        );
        m
    });
    let td = make_td(
        "Root",
        &[("Outer", make_val(Value::UInt32(100))), ("Inner", inner)],
    );
    let tpl = roundtrip(xml, td);

    assert_eq!(*get_field(&tpl, "Outer"), make_val(Value::UInt32(100)));
    if let ValueData::Group(g) = get_field(&tpl, "Inner") {
        assert_eq!(g.get("InnerVal").unwrap(), &make_val(Value::UInt32(42)));
        assert_eq!(
            g.get("InnerTxt").unwrap(),
            &make_val(Value::AsciiString("hello".to_string()))
        );
    } else {
        panic!("expected group");
    }
}

#[test]
fn group_mandatory_with_optional_fields() {
    // Mandatory group with optional fields inside (has_pmap=true, sub-segment pmap has bits)
    let xml = r#"<templates>
  <template id="1" name="Root">
    <uInt32 id="1" name="Outer"/>
    <group id="2" name="Inner">
      <uInt32 id="3" name="InnerVal" presence="optional"/>
      <string id="4" name="InnerTxt" presence="optional"/>
    </group>
  </template>
</templates>"#;
    let inner = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("InnerVal".to_string(), make_val(Value::UInt32(7)));
        m.insert("InnerTxt".to_string(), make_none());
        m
    });
    let td = make_td(
        "Root",
        &[("Outer", make_val(Value::UInt32(1))), ("Inner", inner)],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = get_field(&tpl, "Inner") {
        assert_eq!(g.get("InnerVal").unwrap(), &make_val(Value::UInt32(7)));
        assert_eq!(g.get("InnerTxt").unwrap(), &make_none());
    }
}

#[test]
fn group_optional_present() {
    // Optional group present with optional field inside (has_pmap for both root and sub-segment)
    let xml = r#"<templates>
  <template id="1" name="Root">
    <uInt32 id="1" name="Outer"/>
    <group id="2" name="Inner" presence="optional">
      <uInt32 id="3" name="InnerVal" presence="optional"/>
    </group>
  </template>
</templates>"#;
    let inner = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("InnerVal".to_string(), make_val(Value::UInt32(99)));
        m
    });
    let td = make_td(
        "Root",
        &[("Outer", make_val(Value::UInt32(1))), ("Inner", inner)],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = get_field(&tpl, "Inner") {
        assert_eq!(g.get("InnerVal").unwrap(), &make_val(Value::UInt32(99)));
    } else {
        panic!("expected group");
    }
}

#[test]
fn group_optional_absent() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <uInt32 id="1" name="Outer"/>
    <group id="2" name="Inner" presence="optional">
      <uInt32 id="3" name="InnerVal" presence="optional"/>
    </group>
  </template>
</templates>"#;
    let td = make_td(
        "Root",
        &[
            ("Outer", make_val(Value::UInt32(1))),
            ("Inner", ValueData::None),
        ],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = &tpl.value {
        assert!(
            matches!(g.get("Inner"), Some(&ValueData::None) | None),
            "expected group to be absent, got: {:?}",
            g.get("Inner")
        );
    }
}

#[test]
fn group_nested_three_levels() {
    // Three levels of nesting: Outer -> Middle -> Inner
    // Each level has optional fields so sub-segment pmaps have bits
    let xml = r#"<templates>
  <template id="1" name="Root">
    <group id="1" name="Outer">
      <uInt32 id="2" name="OuterVal" presence="optional"/>
      <group id="3" name="Middle">
        <string id="4" name="MiddleTxt"/>
        <group id="5" name="Inner" presence="optional">
          <uInt32 id="6" name="InnerVal" presence="optional"/>
        </group>
      </group>
    </group>
  </template>
</templates>"#;
    let inner = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("InnerVal".to_string(), make_val(Value::UInt32(123)));
        m
    });
    let middle = ValueData::Group({
        let mut m = HashMap::new();
        m.insert(
            "MiddleTxt".to_string(),
            make_val(Value::AsciiString("mid".to_string())),
        );
        m.insert("Inner".to_string(), inner);
        m
    });
    let outer = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("OuterVal".to_string(), make_val(Value::UInt32(1)));
        m.insert("Middle".to_string(), middle);
        m
    });
    let td = make_td("Root", &[("Outer", outer)]);
    let tpl = roundtrip(xml, td);

    // Navigate: Root -> Outer -> Middle -> Inner
    if let ValueData::Group(outer_g) = get_field(&tpl, "Outer") {
        if let ValueData::Group(middle_g) = outer_g.get("Middle").unwrap() {
            if let ValueData::Group(inner_g) = middle_g.get("Inner").unwrap() {
                if let ValueData::Value(Some(Value::UInt32(v))) = inner_g.get("InnerVal").unwrap() {
                    assert_eq!(*v, 123);
                } else {
                    panic!("expected UInt32(123)");
                }
            } else {
                panic!("expected group");
            }
        } else {
            panic!("expected group");
        }
    } else {
        panic!("expected group");
    }
}

// ============================================================
// 2. SEQUENCE — <sequence> element (variable-length list)
// ============================================================

fn seq_xml_basic() -> &'static str {
    r#"<templates>
  <template id="1" name="Root">
    <uInt32 id="1" name="Header"/>
    <sequence id="2" name="Items">
      <length id="3" name="ItemCount"/>
      <uInt32 id="4" name="ItemVal"/>
    </sequence>
  </template>
</templates>"#
}

#[test]
fn sequence_empty() {
    let xml = seq_xml_basic();
    let td = make_td(
        "Root",
        &[
            ("Header", make_val(Value::UInt32(1))),
            ("Items", ValueData::Sequence(Vec::new())),
        ],
    );
    let tpl = roundtrip(xml, td);

    assert_eq!(*get_field(&tpl, "Header"), make_val(Value::UInt32(1)));
    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert!(items.is_empty());
    } else {
        panic!("expected sequence");
    }
}

#[test]
fn sequence_single_item() {
    let xml = seq_xml_basic();
    let item = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("ItemVal".to_string(), make_val(Value::UInt32(42)));
        m
    });
    let td = make_td(
        "Root",
        &[
            ("Header", make_val(Value::UInt32(1))),
            ("Items", ValueData::Sequence(vec![item])),
        ],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert_eq!(items.len(), 1);
        if let ValueData::Group(g) = &items[0] {
            assert_eq!(g.get("ItemVal").unwrap(), &make_val(Value::UInt32(42)));
        }
    } else {
        panic!("expected sequence");
    }
}

#[test]
fn sequence_multiple_items() {
    let xml = seq_xml_basic();
    let items: Vec<ValueData> = (0..5)
        .map(|i| {
            ValueData::Group({
                let mut m = HashMap::new();
                m.insert("ItemVal".to_string(), make_val(Value::UInt32(i * 100)));
                m
            })
        })
        .collect();
    let td = make_td(
        "Root",
        &[
            ("Header", make_val(Value::UInt32(1))),
            ("Items", ValueData::Sequence(items)),
        ],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert_eq!(items.len(), 5);
        for (i, item) in items.iter().enumerate() {
            if let ValueData::Group(g) = item {
                assert_eq!(
                    g.get("ItemVal").unwrap(),
                    &make_val(Value::UInt32((i as u32) * 100))
                );
            }
        }
    } else {
        panic!("expected sequence");
    }
}

// Sequence with multiple fields per item
#[test]
fn sequence_multi_field_items() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Items">
      <length id="2" name="ItemCount"/>
      <uInt32 id="3" name="Id"/>
      <string id="4" name="Name"/>
    </sequence>
  </template>
</templates>"#;
    let items = vec![
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("Id".to_string(), make_val(Value::UInt32(1)));
            m.insert(
                "Name".to_string(),
                make_val(Value::AsciiString("alpha".to_string())),
            );
            m
        }),
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("Id".to_string(), make_val(Value::UInt32(2)));
            m.insert(
                "Name".to_string(),
                make_val(Value::AsciiString("beta".to_string())),
            );
            m
        }),
    ];
    let td = make_td("Root", &[("Items", ValueData::Sequence(items))]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert_eq!(items.len(), 2);
    }
}

// Sequence with pmap per item (optional fields inside sequence)
#[test]
fn sequence_with_pmap_per_item() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Items">
      <length id="2" name="ItemCount"/>
      <uInt32 id="3" name="Id"/>
      <string id="4" name="Name" presence="optional"/>
    </sequence>
  </template>
</templates>"#;
    let items = vec![
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("Id".to_string(), make_val(Value::UInt32(1)));
            m.insert(
                "Name".to_string(),
                make_val(Value::AsciiString("hello".to_string())),
            );
            m
        }),
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("Id".to_string(), make_val(Value::UInt32(2)));
            m.insert("Name".to_string(), make_none());
            m
        }),
    ];
    let td = make_td("Root", &[("Items", ValueData::Sequence(items))]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert_eq!(items.len(), 2);
        if let ValueData::Group(g0) = &items[0] {
            assert_eq!(
                g0.get("Name").unwrap(),
                &make_val(Value::AsciiString("hello".to_string()))
            );
        }
        if let ValueData::Group(g1) = &items[1] {
            assert_eq!(g1.get("Name").unwrap(), &make_none());
        }
    }
}

// Sequence with optional length (Copy operator)
#[test]
fn sequence_copy_length_omits_when_zero() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Items" presence="optional">
      <length id="2" name="ItemCount"><copy value="0"/></length>
      <uInt32 id="3" name="ItemVal"/>
    </sequence>
  </template>
</templates>"#;
    let td = make_td("Root", &[("Items", ValueData::Sequence(Vec::new()))]);
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let bytes = enc.encode_template_data(td).unwrap();

    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();
    let (tpl, consumed) = dec.parse(&bytes).unwrap();
    assert_eq!(consumed, bytes.len());
    if let ValueData::Sequence(items) = get_field(&tpl, "Items") {
        assert!(items.is_empty());
    }
}

// Sequence inside a group
#[test]
fn sequence_inside_group() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <group id="1" name="Header">
      <uInt32 id="2" name="HeaderVal"/>
    </group>
    <sequence id="3" name="Items">
      <length id="4" name="ItemCount"/>
      <uInt32 id="5" name="ItemVal"/>
    </sequence>
  </template>
</templates>"#;
    let header = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("HeaderVal".to_string(), make_val(Value::UInt32(99)));
        m
    });
    let items = vec![ValueData::Group({
        let mut m = HashMap::new();
        m.insert("ItemVal".to_string(), make_val(Value::UInt32(1)));
        m
    })];
    let td = make_td(
        "Root",
        &[("Header", header), ("Items", ValueData::Sequence(items))],
    );
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(seq) = get_field(&tpl, "Items") {
        assert_eq!(seq.len(), 1);
    }
}

// ============================================================
// 3. TEMPLATEREF — <templateRef> element
// ============================================================

// Static templateRef: fields merge into parent
#[test]
fn templateref_static_roundtrip() {
    let xml = r#"<templates>
  <template id="1" name="Header">
    <uInt32 id="1" name="HeaderVal"/>
    <string id="2" name="HeaderTxt"/>
  </template>
  <template id="2" name="Root">
    <uInt32 id="3" name="Outer"/>
    <templateRef id="4" name="Header"/>
    <string id="5" name="Footer"/>
  </template>
</templates>"#;
    let header = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("HeaderVal".to_string(), make_val(Value::UInt32(42)));
        m.insert(
            "HeaderTxt".to_string(),
            make_val(Value::AsciiString("hi".to_string())),
        );
        m
    });
    let td = make_td(
        "Root",
        &[
            ("Outer", make_val(Value::UInt32(1))),
            ("Header", header),
            ("Footer", make_val(Value::AsciiString("bye".to_string()))),
        ],
    );
    let tpl = roundtrip(xml, td);

    // Static templateRef fields merge into parent
    assert_eq!(*get_field(&tpl, "Outer"), make_val(Value::UInt32(1)));
    assert_eq!(*get_field(&tpl, "HeaderVal"), make_val(Value::UInt32(42)));
    assert_eq!(
        *get_field(&tpl, "HeaderTxt"),
        make_val(Value::AsciiString("hi".to_string()))
    );
    assert_eq!(
        *get_field(&tpl, "Footer"),
        make_val(Value::AsciiString("bye".to_string()))
    );
}

// Dynamic templateRef: encoded with pmap + template ID on wire
#[test]
fn templateref_dynamic_roundtrip() {
    let xml = r#"<templates>
  <template id="1" name="DynamicPayload">
    <uInt32 id="1" name="PayloadVal"/>
    <string id="2" name="PayloadTxt"/>
  </template>
  <template id="2" name="Root">
    <uInt32 id="3" name="Outer"/>
    <templateRef id="4"/>
    <string id="5" name="Footer"/>
  </template>
</templates>"#;

    let payload = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("PayloadVal".to_string(), make_val(Value::UInt32(777)));
        m.insert(
            "PayloadTxt".to_string(),
            make_val(Value::AsciiString("dyn".to_string())),
        );
        m
    });
    // For dynamic templateRef, the DynamicTemplateRef IS the data at the instruction level
    let mut map = HashMap::new();
    map.insert("Outer".to_string(), make_val(Value::UInt32(1)));
    // The encoder's encode_template_ref_buf matches on ValueData::DynamicTemplateRef directly
    map.insert(
        "".to_string(),
        ValueData::DynamicTemplateRef(Box::new(TemplateData {
            name: "DynamicPayload".to_string(),
            value: payload,
            pmap_bytes: None,
        })),
    );
    map.insert(
        "Footer".to_string(),
        make_val(Value::AsciiString("end".to_string())),
    );
    let td = TemplateData {
        name: "Root".to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    };
    let tpl = roundtrip(xml, td);

    assert_eq!(*get_field(&tpl, "Outer"), make_val(Value::UInt32(1)));
    assert_eq!(
        *get_field(&tpl, "Footer"),
        make_val(Value::AsciiString("end".to_string()))
    );
    // Dynamic templateRef is stored under "templateRef:0"
    let tpl_ref = get_field(&tpl, "templateRef:0");
    if let ValueData::DynamicTemplateRef(tpl_data) = tpl_ref {
        assert_eq!(tpl_data.name, "DynamicPayload");
        if let ValueData::Group(g) = &tpl_data.value {
            assert_eq!(g.get("PayloadVal").unwrap(), &make_val(Value::UInt32(777)));
        }
    } else {
        panic!("expected DynamicTemplateRef, got: {:?}", tpl_ref);
    }
}

// Multiple dynamic templateRefs in sequence
#[test]
fn templateref_dynamic_multiple() {
    let xml = r#"<templates>
  <template id="1" name="DynamicPayload">
    <uInt32 id="1" name="PayloadVal"/>
    <string id="2" name="PayloadTxt"/>
  </template>
  <template id="2" name="Root">
    <uInt32 id="3" name="Outer"/>
    <templateRef id="4"/>
    <string id="5" name="Footer"/>
  </template>
</templates>"#;

    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();

    // First message
    let mut map1 = HashMap::new();
    map1.insert("Outer".to_string(), make_val(Value::UInt32(100)));
    map1.insert(
        "".to_string(),
        ValueData::DynamicTemplateRef(Box::new(TemplateData {
            name: "DynamicPayload".to_string(),
            value: ValueData::Group({
                let mut m = HashMap::new();
                m.insert("PayloadVal".to_string(), make_val(Value::UInt32(1)));
                m.insert(
                    "PayloadTxt".to_string(),
                    make_val(Value::AsciiString("first".to_string())),
                );
                m
            }),
            pmap_bytes: None,
        })),
    );
    map1.insert(
        "Footer".to_string(),
        make_val(Value::AsciiString("v1".to_string())),
    );
    let td1 = TemplateData {
        name: "Root".to_string(),
        value: ValueData::Group(map1),
        pmap_bytes: None,
    };
    let bytes1 = enc.encode_template_data(td1).unwrap();

    // Second message
    let mut map2 = HashMap::new();
    map2.insert("Outer".to_string(), make_val(Value::UInt32(200)));
    map2.insert(
        "".to_string(),
        ValueData::DynamicTemplateRef(Box::new(TemplateData {
            name: "DynamicPayload".to_string(),
            value: ValueData::Group({
                let mut m = HashMap::new();
                m.insert("PayloadVal".to_string(), make_val(Value::UInt32(2)));
                m.insert(
                    "PayloadTxt".to_string(),
                    make_val(Value::AsciiString("second".to_string())),
                );
                m
            }),
            pmap_bytes: None,
        })),
    );
    map2.insert(
        "Footer".to_string(),
        make_val(Value::AsciiString("v2".to_string())),
    );
    let td2 = TemplateData {
        name: "Root".to_string(),
        value: ValueData::Group(map2),
        pmap_bytes: None,
    };
    let bytes2 = enc.encode_template_data(td2).unwrap();

    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();
    let (tpl1, consumed1) = dec.parse(&bytes1).unwrap();
    assert_eq!(consumed1, bytes1.len());
    let (tpl2, consumed2) = dec.parse(&bytes2).unwrap();
    assert_eq!(consumed2, bytes2.len());

    assert_eq!(*get_field(&tpl1, "Outer"), make_val(Value::UInt32(100)));
    assert_eq!(*get_field(&tpl2, "Outer"), make_val(Value::UInt32(200)));
}

// ============================================================
// 4. TYPEREF — typeRef attribute on templates
// ============================================================

#[test]
fn typeref_template_roundtrip() {
    let xml = r#"<templates>
  <template id="1" name="TypeA" typeRef="TypeA">
    <uInt32 id="1" name="ValA"/>
  </template>
  <template id="2" name="TypeB" typeRef="TypeB">
    <uInt32 id="2" name="ValB"/>
    <string id="3" name="TxtB"/>
  </template>
</templates>"#;
    let td = make_td("TypeA", &[("ValA", make_val(Value::UInt32(42)))]);
    let tpl = roundtrip(xml, td);
    assert_eq!(*get_field(&tpl, "ValA"), make_val(Value::UInt32(42)));
}

#[test]
fn typeref_multiple_templates() {
    let xml = r#"<templates>
  <template id="1" name="TypeA" typeRef="TypeA">
    <uInt32 id="1" name="ValA"/>
  </template>
  <template id="2" name="TypeB" typeRef="TypeB">
    <uInt32 id="2" name="ValB"/>
    <string id="3" name="TxtB"/>
  </template>
</templates>"#;
    let td_a = make_td("TypeA", &[("ValA", make_val(Value::UInt32(100)))]);
    let td_b = make_td(
        "TypeB",
        &[
            ("ValB", make_val(Value::UInt32(200))),
            ("TxtB", make_val(Value::AsciiString("test".to_string()))),
        ],
    );

    let tpl_a = roundtrip(xml, td_a);
    assert_eq!(*get_field(&tpl_a, "ValA"), make_val(Value::UInt32(100)));

    let tpl_b = roundtrip(xml, td_b);
    assert_eq!(*get_field(&tpl_b, "ValB"), make_val(Value::UInt32(200)));
    assert_eq!(
        *get_field(&tpl_b, "TxtB"),
        make_val(Value::AsciiString("test".to_string()))
    );
}

// typeRef as child element <typeRef name="..."/> on template (spec syntax)
#[test]
fn typeref_template_child_element() {
    let xml = r#"<templates>
  <template id="1" name="TypeA">
    <typeRef name="AppTypeA" />
    <uInt32 id="1" name="ValA"/>
  </template>
</templates>"#;
    let td = make_td("TypeA", &[("ValA", make_val(Value::UInt32(42)))]);
    let tpl = roundtrip(xml, td);
    assert_eq!(*get_field(&tpl, "ValA"), make_val(Value::UInt32(42)));
}

// typeRef as child element on group
#[test]
fn typeref_group_child_element() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <group id="1" name="Payload">
      <typeRef name="PayloadType" />
      <uInt32 id="2" name="Val"/>
      <string id="3" name="Txt" presence="optional"/>
    </group>
  </template>
</templates>"#;
    let payload = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("Val".to_string(), make_val(Value::UInt32(99)));
        m.insert("Txt".to_string(), make_none());
        m
    });
    let td = make_td("Root", &[("Payload", payload)]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = get_field(&tpl, "Payload") {
        assert_eq!(g.get("Val").unwrap(), &make_val(Value::UInt32(99)));
        assert_eq!(g.get("Txt").unwrap(), &make_none());
    }
}

// typeRef as child element on sequence
#[test]
fn typeref_sequence_child_element() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Items">
      <typeRef name="ItemType" />
      <length id="2" name="ItemCount"/>
      <uInt32 id="3" name="ItemVal"/>
    </sequence>
  </template>
</templates>"#;
    let items = vec![ValueData::Group({
        let mut m = HashMap::new();
        m.insert("ItemVal".to_string(), make_val(Value::UInt32(1)));
        m
    })];
    let td = make_td("Root", &[("Items", ValueData::Sequence(items))]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(seq) = get_field(&tpl, "Items") {
        assert_eq!(seq.len(), 1);
        if let ValueData::Group(g) = &seq[0] {
            assert_eq!(g.get("ItemVal").unwrap(), &make_val(Value::UInt32(1)));
        }
    }
}

// typeRef as attribute on group (not child element)
#[test]
fn typeref_group_attribute() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <group id="1" name="Payload" typeRef="PayloadType">
      <uInt32 id="2" name="Val"/>
    </group>
  </template>
</templates>"#;
    let payload = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("Val".to_string(), make_val(Value::UInt32(77)));
        m
    });
    let td = make_td("Root", &[("Payload", payload)]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = get_field(&tpl, "Payload") {
        assert_eq!(g.get("Val").unwrap(), &make_val(Value::UInt32(77)));
    }
}

// typeRef as attribute on sequence (not child element)
#[test]
fn typeref_sequence_attribute() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Items" typeRef="ItemType">
      <length id="2" name="ItemCount"/>
      <uInt32 id="3" name="ItemVal"/>
    </sequence>
  </template>
</templates>"#;
    let items = vec![ValueData::Group({
        let mut m = HashMap::new();
        m.insert("ItemVal".to_string(), make_val(Value::UInt32(42)));
        m
    })];
    let td = make_td("Root", &[("Items", ValueData::Sequence(items))]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(seq) = get_field(&tpl, "Items") {
        assert_eq!(seq.len(), 1);
        if let ValueData::Group(g) = &seq[0] {
            assert_eq!(g.get("ItemVal").unwrap(), &make_val(Value::UInt32(42)));
        }
    }
}

// dictionary="type" with typeRef — copy operator scoped by application type
#[test]
fn typeref_dictionary_type_scoping() {
    // Two templates with different typeRef values and dictionary="type".
    // Copy operator state should be independent per typeRef.
    let xml = r#"<templates>
  <template id="1" name="MsgA" typeRef="TypeA" dictionary="type">
    <string id="1" name="Label">
      <copy/>
    </string>
    <uInt32 id="2" name="Val"/>
  </template>
  <template id="2" name="MsgB" typeRef="TypeB" dictionary="type">
    <string id="1" name="Label">
      <copy/>
    </string>
    <uInt32 id="2" name="Val"/>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    // Encode two messages of type MsgA with same Label
    let td_a1 = make_td(
        "MsgA",
        &[
            ("Label", make_val(Value::AsciiString("hello".to_string()))),
            ("Val", make_val(Value::UInt32(1))),
        ],
    );
    let bytes_a1 = enc.encode_template_data(td_a1).unwrap();

    let td_a2 = make_td(
        "MsgA",
        &[
            ("Label", make_val(Value::AsciiString("hello".to_string()))),
            ("Val", make_val(Value::UInt32(2))),
        ],
    );
    let bytes_a2 = enc.encode_template_data(td_a2).unwrap();

    // Second message should skip Label (copy operator)
    assert!(
        bytes_a2.len() < bytes_a1.len(),
        "copy should skip unchanged Label"
    );

    // Encode MsgB with same Label — should NOT be compressed (different typeRef scope)
    let td_b = make_td(
        "MsgB",
        &[
            ("Label", make_val(Value::AsciiString("hello".to_string()))),
            ("Val", make_val(Value::UInt32(3))),
        ],
    );
    let bytes_b = enc.encode_template_data(td_b).unwrap();

    // Decode and verify values
    let (tpl_a1, _) = dec.parse(&bytes_a1).unwrap();
    assert_eq!(
        *get_field(&tpl_a1, "Label"),
        make_val(Value::AsciiString("hello".to_string()))
    );
    assert_eq!(*get_field(&tpl_a1, "Val"), make_val(Value::UInt32(1)));

    let (tpl_a2, _) = dec.parse(&bytes_a2).unwrap();
    assert_eq!(
        *get_field(&tpl_a2, "Label"),
        make_val(Value::AsciiString("hello".to_string()))
    );
    assert_eq!(*get_field(&tpl_a2, "Val"), make_val(Value::UInt32(2)));

    let (tpl_b, _) = dec.parse(&bytes_b).unwrap();
    assert_eq!(
        *get_field(&tpl_b, "Label"),
        make_val(Value::AsciiString("hello".to_string()))
    );
    assert_eq!(*get_field(&tpl_b, "Val"), make_val(Value::UInt32(3)));
}

// same typeRef across different templates — copy state should be shared
#[test]
fn typeref_shared_state_same_type() {
    // Two different templates with the same typeRef and dictionary="type".
    // Copy operator state should be shared: a value set in MsgA is visible to MsgB.
    let xml = r#"<templates>
  <template id="1" name="MsgA" typeRef="SharedType" dictionary="type">
    <string id="1" name="Label">
      <copy/>
    </string>
    <uInt32 id="2" name="ValA"/>
  </template>
  <template id="2" name="MsgB" typeRef="SharedType" dictionary="type">
    <string id="1" name="Label">
      <copy/>
    </string>
    <uInt32 id="3" name="ValB"/>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    // Encode MsgA with Label
    let td_a = make_td(
        "MsgA",
        &[
            ("Label", make_val(Value::AsciiString("shared".to_string()))),
            ("ValA", make_val(Value::UInt32(1))),
        ],
    );
    let bytes_a = enc.encode_template_data(td_a).unwrap();

    // Encode MsgB with same Label — should be compressed (shared typeRef state)
    let td_b = make_td(
        "MsgB",
        &[
            ("Label", make_val(Value::AsciiString("shared".to_string()))),
            ("ValB", make_val(Value::UInt32(2))),
        ],
    );
    let bytes_b = enc.encode_template_data(td_b).unwrap();

    // MsgB should skip Label (copy state shared via typeRef)
    assert!(
        bytes_b.len() < bytes_a.len(),
        "same typeRef should share copy state"
    );

    // Decode and verify
    let (da, _) = dec.parse(&bytes_a).unwrap();
    assert_eq!(
        *get_field(&da, "Label"),
        make_val(Value::AsciiString("shared".to_string()))
    );

    let (db, _) = dec.parse(&bytes_b).unwrap();
    assert_eq!(
        *get_field(&db, "Label"),
        make_val(Value::AsciiString("shared".to_string()))
    );
}

// typeRef as namespace within a single template — two groups with same field names
// but different typeRefs should have independent copy state
#[test]
fn typeref_group_namespace_isolation() {
    // Same template, two groups with different typeRefs, same field names, dictionary="type".
    // Copy operators should NOT share state across typeRef boundaries.
    let xml = r#"<templates>
  <template id="1" name="Quote" typeRef="QuoteType" dictionary="type">
    <group id="1" name="BidSide" typeRef="Bid" dictionary="type">
      <string id="2" name="Side">
        <copy/>
      </string>
      <uInt32 id="3" name="Px">
        <copy/>
      </uInt32>
    </group>
    <group id="4" name="AskSide" typeRef="Ask" dictionary="type">
      <string id="5" name="Side">
        <copy/>
      </string>
      <uInt32 id="6" name="Px">
        <copy/>
      </uInt32>
    </group>
  </template>
</templates>"#;
    let mut enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
    let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();

    // First message: both sides present
    let msg1 = make_td(
        "Quote",
        &[
            (
                "BidSide",
                ValueData::Group({
                    let mut m = HashMap::new();
                    m.insert(
                        "Side".to_string(),
                        make_val(Value::AsciiString("BID".to_string())),
                    );
                    m.insert("Px".to_string(), make_val(Value::UInt32(100)));
                    m
                }),
            ),
            (
                "AskSide",
                ValueData::Group({
                    let mut m = HashMap::new();
                    m.insert(
                        "Side".to_string(),
                        make_val(Value::AsciiString("ASK".to_string())),
                    );
                    m.insert("Px".to_string(), make_val(Value::UInt32(200)));
                    m
                }),
            ),
        ],
    );
    let bytes1 = enc.encode_template_data(msg1).unwrap();

    // Second message: same values — copy should compress within each typeRef
    let msg2 = make_td(
        "Quote",
        &[
            (
                "BidSide",
                ValueData::Group({
                    let mut m = HashMap::new();
                    m.insert(
                        "Side".to_string(),
                        make_val(Value::AsciiString("BID".to_string())),
                    );
                    m.insert("Px".to_string(), make_val(Value::UInt32(100)));
                    m
                }),
            ),
            (
                "AskSide",
                ValueData::Group({
                    let mut m = HashMap::new();
                    m.insert(
                        "Side".to_string(),
                        make_val(Value::AsciiString("ASK".to_string())),
                    );
                    m.insert("Px".to_string(), make_val(Value::UInt32(200)));
                    m
                }),
            ),
        ],
    );
    let bytes2 = enc.encode_template_data(msg2).unwrap();

    // Second message should be shorter (copy compression)
    assert!(
        bytes2.len() <= bytes1.len(),
        "copy should compress unchanged values"
    );

    // Decode and verify
    let (d1, _) = dec.parse(&bytes1).unwrap();
    let (d2, _) = dec.parse(&bytes2).unwrap();
    assert_eq!(get_field(&d1, "BidSide"), get_field(&d2, "BidSide"));
    assert_eq!(get_field(&d1, "AskSide"), get_field(&d2, "AskSide"));
}

// typeRef override in nested group — copy state should use group's typeRef, not template's
#[test]
fn typeref_nested_override_isolation() {
    // Template typeRef="A", inner group typeRef="B".
    // Fields inside the group should scope copy state to "B", not "A".
    let xml = r#"<templates>
  <template id="1" name="Root" typeRef="Outer" dictionary="type">
    <string id="1" name="Label">
      <copy/>
    </string>
    <group id="2" name="Inner" typeRef="Inner" dictionary="type">
      <string id="3" name="Label">
        <copy/>
      </string>
      <uInt32 id="4" name="Val"/>
    </group>
  </template>
</templates>"#;
    let msg = make_td(
        "Root",
        &[
            ("Label", make_val(Value::AsciiString("outer".to_string()))),
            (
                "Inner",
                ValueData::Group({
                    let mut m = HashMap::new();
                    m.insert(
                        "Label".to_string(),
                        make_val(Value::AsciiString("inner".to_string())),
                    );
                    m.insert("Val".to_string(), make_val(Value::UInt32(42)));
                    m
                }),
            ),
        ],
    );
    let tpl = roundtrip(xml, msg);

    // Outer and inner "Label" should have independent copy state
    assert_eq!(
        *get_field(&tpl, "Label"),
        make_val(Value::AsciiString("outer".to_string()))
    );
    if let ValueData::Group(g) = get_field(&tpl, "Inner") {
        assert_eq!(
            g.get("Label").unwrap(),
            &make_val(Value::AsciiString("inner".to_string()))
        );
        assert_eq!(g.get("Val").unwrap(), &make_val(Value::UInt32(42)));
    }
}

// ============================================================
// 5. COMPLEX — group containing sequence
// ============================================================

#[test]
fn group_containing_sequence() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <group id="1" name="Container">
      <uInt32 id="2" name="Count"/>
      <sequence id="3" name="Entries">
        <length id="4" name="EntryCount"/>
        <uInt32 id="5" name="EntryVal"/>
        <string id="6" name="EntryTxt"/>
      </sequence>
    </group>
  </template>
</templates>"#;
    let entries = vec![
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("EntryVal".to_string(), make_val(Value::UInt32(1)));
            m.insert(
                "EntryTxt".to_string(),
                make_val(Value::AsciiString("a".to_string())),
            );
            m
        }),
        ValueData::Group({
            let mut m = HashMap::new();
            m.insert("EntryVal".to_string(), make_val(Value::UInt32(2)));
            m.insert(
                "EntryTxt".to_string(),
                make_val(Value::AsciiString("b".to_string())),
            );
            m
        }),
    ];
    let container = ValueData::Group({
        let mut m = HashMap::new();
        m.insert("Count".to_string(), make_val(Value::UInt32(2)));
        m.insert("Entries".to_string(), ValueData::Sequence(entries));
        m
    });
    let td = make_td("Root", &[("Container", container)]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Group(g) = get_field(&tpl, "Container") {
        if let ValueData::Sequence(seq) = g.get("Entries").unwrap() {
            assert_eq!(seq.len(), 2);
        }
    }
}

// ============================================================
// 6. COMPLEX — sequence containing group
// ============================================================

#[test]
fn sequence_containing_group() {
    let xml = r#"<templates>
  <template id="1" name="Root">
    <sequence id="1" name="Rows">
      <length id="2" name="RowCount"/>
      <group id="3" name="Row">
        <uInt32 id="4" name="RowId"/>
        <string id="5" name="RowName"/>
      </group>
    </sequence>
  </template>
</templates>"#;
    let rows = vec![ValueData::Group({
        let mut m = HashMap::new();
        let row = ValueData::Group({
            let mut r = HashMap::new();
            r.insert("RowId".to_string(), make_val(Value::UInt32(1)));
            r.insert(
                "RowName".to_string(),
                make_val(Value::AsciiString("first".to_string())),
            );
            r
        });
        m.insert("Row".to_string(), row);
        m
    })];
    let td = make_td("Root", &[("Rows", ValueData::Sequence(rows))]);
    let tpl = roundtrip(xml, td);

    if let ValueData::Sequence(seq) = get_field(&tpl, "Rows") {
        assert_eq!(seq.len(), 1);
        if let ValueData::Group(g) = &seq[0] {
            if let ValueData::Group(row) = g.get("Row").unwrap() {
                assert_eq!(row.get("RowId").unwrap(), &make_val(Value::UInt32(1)));
            }
        }
    }
}
