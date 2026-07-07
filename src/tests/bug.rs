/// Our decoder strips `<typeRef>` elements from the XML template,
/// allowing templates that extend base templates to be parsed.
use crate::{Dictionary, FastDecoder};

#[test]
fn typeref_stripped() {
    // Minimal FAST template with `<typeRef>` (extends a base template)
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<templates version="2.26" xmlns="http://www.fixprotocol.org/ns/template-definition">
    <template name="Child" id="1">
        <typeRef name="Base" />
        <string name="ExtraField" id="100"><copy/></string>
    </template>
</templates>"#;

    // After stripping typeRef, the remaining template parses successfully.
    let stripped = xml.replace("<typeRef name=\"Base\" />", "");
    FastDecoder::new(&stripped, Dictionary::Global).expect("typeRef stripped, remaining template parses");
}
