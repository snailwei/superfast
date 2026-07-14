//! FAST Template — a sequence of instructions.

use std::sync::atomic::AtomicI8;

use roxmltree::Node;

use crate::errors::Result;
use crate::instruction::Instruction;
use crate::types::{Dictionary, TypeRef};

/// A template contains a sequence of instructions. The order of the instructions is significant and corresponds
/// to the order of the data in the stream.
pub(crate) struct Template {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) type_ref: TypeRef,
    pub(crate) dictionary: Dictionary,
    pub(crate) instructions: Vec<Instruction>,

    // This flag indicates if the template requires a presence map in case of statically referenced
    // from another template. If the flag is None, the presence map is not calculated yet.
    pub(crate) require_pmap: AtomicI8, // -1=None, 0=Some(false), 1=Some(true)
}

impl Template {
    pub(crate) fn from_node(node: Node) -> Result<Self> {
        if node.tag_name().name() != "template" {
            return Err(crate::errors::Error::Static(format!(
                "expected <template/> node, got <{}/>",
                node.tag_name().name()
            )));
        }
        let id = node.attribute("id").unwrap_or("0").parse::<u32>()?;
        let name = node
            .attribute("name")
            .ok_or_else(|| crate::errors::Error::Static("template name not found".to_string()))?
            .to_string();
        let mut type_ref = node
            .attribute("typeRef")
            .map_or(TypeRef::Any, TypeRef::from_str);
        let dictionary = node
            .attribute("dictionary")
            .map_or(Dictionary::Global, Dictionary::from_str);
        let mut instructions = Vec::new();
        for child in node.children() {
            if child.tag_name().name() == "typeRef" {
                // Read <typeRef name="..."/> child element (preferred spec syntax)
                // Child element takes precedence over attribute
                if let Some(name) = child.attribute("name") {
                    type_ref = TypeRef::from_str(name);
                }
            } else if child.is_element() {
                instructions.push(Instruction::from_node(child)?);
            }
        }
        Ok(Self {
            id,
            name,
            type_ref,
            dictionary,
            instructions,
            require_pmap: AtomicI8::new(-1),
        })
    }
}
