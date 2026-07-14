//! FAST Definitions — template registry.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};

use crate::errors::{Error, Result};
use crate::instruction::Instruction;
use crate::template::Template;
use crate::types::{Dictionary, Operator, Presence, TypeRef};
use crate::value::ValueType;

/// Stores template definitions and global processing context.
#[derive(Clone)]
pub struct Definitions {
    pub(crate) templates: Vec<Arc<Template>>,
    pub(crate) templates_by_id: HashMap<u32, Arc<Template>>,
    pub(crate) templates_by_name: HashMap<String, Arc<Template>>,
    pub(crate) template_id_instruction: Arc<Instruction>,
}

impl Definitions {
    pub(crate) fn new_from_templates(ts: Vec<Template>, default_dict: Dictionary) -> Result<Self> {
        let isolate = matches!(default_dict, Dictionary::Template);
        let mut templates = Vec::with_capacity(ts.len());
        let mut templates_by_id = HashMap::with_capacity(ts.len());
        let mut templates_by_name = HashMap::with_capacity(ts.len());
        for mut t in ts {
            if isolate && t.dictionary == Dictionary::Global {
                t.dictionary = Dictionary::Template;
            }
            // Propagate template-level dictionary to child instructions that
            // don't have an explicit dictionary attribute (spec inheritance).
            for instr in &mut t.instructions {
                instr.propagate_dictionary(&t.dictionary);
            }
            let t = Arc::new(t);
            if t.id != 0 {
                templates_by_id.insert(t.id, t.clone());
            }
            if !t.name.is_empty() {
                templates_by_name.insert(t.name.clone(), t.clone());
            }
            templates.push(t);
        }

        let template_id_instruction = Arc::new(Instruction {
            id: 0,
            name: "__template_id__".to_string(),
            value_type: ValueType::UInt32,
            presence: Presence::Mandatory,
            nullable: false,
            operator: Operator::Copy,
            initial_value: None,
            instructions: Vec::new(),
            dictionary: Dictionary::Global,
            key: Arc::from("__template_id__"),
            type_ref: TypeRef::Any,
            has_pmap: AtomicBool::new(false),
            was_present: AtomicI8::new(-1),
        });

        let definitions = Self {
            templates,
            templates_by_id,
            templates_by_name,
            template_id_instruction,
        };
        definitions.finalize()?;
        Ok(definitions)
    }

    /// Parse XML template definitions.
    ///
    /// The `default_dict` parameter sets the dictionary scope for templates
    /// whose XML does not specify a `dictionary` attribute. See [`Dictionary`]
    /// for the meaning of each scope.
    ///
    /// Use [`Dictionary::Global`] for single-template workloads (spec default).
    /// Use [`Dictionary::Template`] for multi-template workloads where different
    /// message types share field names and global state would cause cross-template
    /// pollution (e.g., market-data feeds).
    ///
    /// # Errors
    ///
    /// Returns [`Error::Static`] if the XML is malformed or semantically invalid.
    pub fn new(text: &str, default_dict: Dictionary) -> Result<Self> {
        let doc = roxmltree::Document::parse(text)?;
        let root = doc
            .root()
            .first_child()
            .ok_or_else(|| Error::Static("no root element found".to_string()))?;
        if root.tag_name().name() != "templates" {
            return Err(Error::Static("<templates/> node not found".to_string()));
        }
        let mut templates = Vec::new();
        for child in root.children() {
            if child.is_element() {
                templates.push(Template::from_node(child)?);
            }
        }
        Self::new_from_templates(templates, default_dict)
    }

    fn finalize(&self) -> Result<()> {
        for tpl in &self.templates {
            let need_pmap = self.require_presence_map_bit(&tpl.instructions)?;
            tpl.require_pmap.store(if need_pmap { 1 } else { -1 }, Ordering::Relaxed);
        }
        Ok(())
    }

    fn require_presence_map_bit(&self, instructions: &[Instruction]) -> Result<bool> {
        let mut has_pmap_bit = false;
        for i in instructions {
            if self.has_presence_map_bit(i)? {
                has_pmap_bit = true;
            }
        }
        Ok(has_pmap_bit)
    }

    fn set_has_pmap(&self, instr: &Instruction) -> Result<()> {
        let instructions: &[Instruction] = match instr.value_type {
            ValueType::Group | ValueType::TemplateReference | ValueType::Decimal => {
                &instr.instructions
            }
            ValueType::Sequence => &instr.instructions[1..],
            _ => {
                return Ok(());
            }
        };
        let need_pmap = self.require_presence_map_bit(instructions)?;
        instr.has_pmap.store(need_pmap, Ordering::Relaxed);
        Ok(())
    }

    fn has_presence_map_bit(&self, instr: &Instruction) -> Result<bool> {
        self.set_has_pmap(instr)?;

        match instr.value_type {
            ValueType::Group => {
                return Ok(instr.is_optional());
            }
            ValueType::Sequence => {
                return self.has_presence_map_bit(instr.instructions.first().ok_or_else(|| {
                    Error::Static(format!("sequence '{}' has no length field", instr.name))
                })?);
            }
            ValueType::TemplateReference => {
                if instr.name.is_empty() {
                    return Ok(false);
                }
                let template = self
                    .templates_by_name
                    .get(&instr.name)
                    .ok_or_else(|| Error::Static(format!("template '{}' not found", instr.name)))?;
                return match template.require_pmap.load(Ordering::Relaxed) {
                    -1 => Err(Error::Static(format!(
                        "template '{}' not initialized yet; consider reordering templates",
                        instr.name
                    ))),
                    0 => Ok(false),
                    1 => Ok(true),
                    _ => Err(Error::Static(format!(
                        "template '{}' has invalid require_pmap state",
                        instr.name
                    ))),
                };
            }
            ValueType::Decimal => {
                if instr.has_pmap.load(Ordering::Relaxed) {
                    return Ok(true);
                }
            }
            _ => {}
        }
        match instr.operator {
            Operator::None | Operator::Delta => Ok(false),
            Operator::Default | Operator::Copy | Operator::Increment | Operator::Tail => Ok(true),
            Operator::Constant => Ok(instr.is_optional()),
        }
    }
}
