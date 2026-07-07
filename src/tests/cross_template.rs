//! Regression tests: copy-operator cross-template dictionary pollution.

use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::value::Value;
use crate::{FastDecoder, FastEncoder};
use std::collections::HashMap;

fn v(s: &str) -> ValueData {
    ValueData::Value(Some(Value::AsciiString(s.to_string())))
}

fn td(name: &str, field: &str, val: ValueData) -> TemplateData {
    let mut map = HashMap::new();
    map.insert(field.to_string(), val);
    TemplateData {
        name: name.to_string(),
        value: ValueData::Group(map),
        pmap_bytes: None,
    }
}

fn two_xml() -> &'static str {
    r#"<templates>
  <template id="1" name="Tick">
    <string id="1" name="Sym" presence="optional"><copy value=""/></string>
  </template>
  <template id="2" name="Txn">
    <string id="1" name="Sym" presence="optional"><copy value=""/></string>
  </template>
</templates>"#
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
enum M {
    #[serde(rename = "Tick")]
    Tick(T),
    #[serde(rename = "Txn")]
    Txn(X),
}
#[derive(Debug, serde::Deserialize)]
struct T {
    #[serde(rename = "Sym", default)]
    sym: Option<String>,
}
#[derive(Debug, serde::Deserialize)]
struct X {
    #[serde(rename = "Sym", default)]
    sym: Option<String>,
}

/// Global dict: Txn overwrites Tick's copy context in the encoder.
#[test]
fn global_dict_pollutes_encoder() {
    let mut enc = FastEncoder::new(two_xml()).unwrap();
    let t1 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    enc.encode_template_data(td("Txn", "Sym", v("XXXX")))
        .unwrap();
    let t2 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    // t2 is NOT compressed — context was overwritten by Txn
    assert!(
        t2.len() >= t1.len(),
        "tick2={} tick1={}",
        t2.len(),
        t1.len()
    );
}

/// Template dict: Txn cannot pollute Tick's copy context in the encoder.
#[test]
fn template_dict_isolates_encoder() {
    let mut enc = FastEncoder::new_with_template_dict(two_xml()).unwrap();
    let t1 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    enc.encode_template_data(td("Txn", "Sym", v("XXXX")))
        .unwrap();
    let t2 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    assert!(t2.len() < t1.len(), "tick2={} tick1={}", t2.len(), t1.len());
}

/// Decoder: with template dict, Txn doesn't corrupt Tick copy state.
#[test]
fn template_dict_isolates_decoder() {
    let mut enc = FastEncoder::new_with_template_dict(two_xml()).unwrap();
    let mut dec = FastDecoder::new_with_template_dict(two_xml()).unwrap();

    let t1 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    let t2 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    let xn = enc
        .encode_template_data(td("Txn", "Sym", v("XXXX")))
        .unwrap();

    assert!(t2.len() < t1.len()); // t2 is compressed (pmap=0)

    let (m1, _): (M, u64) = dec.decode(&t1).unwrap();
    let (mx, _): (M, u64) = dec.decode(&xn).unwrap();
    let (m2, _): (M, u64) = dec.decode(&t2).unwrap();

    if let M::Tick(t) = m2 {
        assert_eq!(
            t.sym.as_deref(),
            Some("AAPL"),
            "tick2 should get AAPL from Tick context, not XXXX from Txn"
        );
    } else {
        panic!("expected Tick")
    }
    if let M::Tick(t1) = m1 {
        assert_eq!(t1.sym.as_deref(), Some("AAPL"));
    } else {
        panic!("expected Tick")
    }
    if let M::Txn(x) = mx {
        assert_eq!(x.sym.as_deref(), Some("XXXX"));
    } else {
        panic!("expected Txn")
    }
}

/// Decoder: with global dict, Txn DOES corrupt Tick copy state.
#[test]
fn global_dict_pollutes_decoder() {
    let mut enc = FastEncoder::new_with_template_dict(two_xml()).unwrap();
    let mut dec = FastDecoder::new(two_xml()).unwrap(); // global dict

    let t1 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    let t2 = enc
        .encode_template_data(td("Tick", "Sym", v("AAPL")))
        .unwrap();
    let xn = enc
        .encode_template_data(td("Txn", "Sym", v("XXXX")))
        .unwrap();

    let (m1, _): (M, u64) = dec.decode(&t1).unwrap();
    let (mx, _): (M, u64) = dec.decode(&xn).unwrap();
    let (m2, _): (M, u64) = dec.decode(&t2).unwrap();

    if let M::Tick(t) = m2 {
        assert_eq!(
            t.sym.as_deref(),
            Some("XXXX"),
            "global dict: tick2 should be polluted by Txn"
        );
    } else {
        panic!("expected Tick")
    }
    if let M::Tick(t1) = m1 {
        assert_eq!(t1.sym.as_deref(), Some("AAPL"));
    } else {
        panic!("expected Tick")
    }
    if let M::Txn(x) = mx {
        assert_eq!(x.sym.as_deref(), Some("XXXX"));
    } else {
        panic!("expected Txn")
    }
}
