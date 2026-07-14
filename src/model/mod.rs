//! Model — bridges FAST decode output with serde deserialization.

use std::rc::Rc;

use crate::stacked::Stacked;
use crate::value::Value;

use self::template::TemplateData;
use self::value::{FieldVec, ValueData};

mod decimal;
pub(crate) mod serialize;
pub mod template;
pub mod value;

/// # Model Factory
/// Creates a template model that later can be deserialized using Serde.
#[derive(Debug, PartialEq)]
pub struct ModelFactory {
    pub data: Option<TemplateData>,

    /// Context stack — only Group entries (templates and sequence items).
    /// Uses Vec so sequences can save parent indices.
    context: Vec<(String, ValueData)>,
    ref_num: Stacked<u32>,
    /// Stack of active sequences: (name, items, original_length, parent_context_index)
    /// Sequences are NOT on the context stack — they are containers, not contexts.
    /// `parent_context_index` is the index into `context` where the sequence
    /// should be inserted when it completes.
    seq_stack: Vec<(String, Vec<ValueData>, u32, usize)>,
    /// Bytes consumed from the reader during decoding (set by DecoderContext).
    consumed: usize,
    /// Raw pmap bytes from the original message (for round-trip fidelity)
    pmap_bytes: Option<Vec<u8>>,
    /// Raw bytes of truncated sequence items (for round-trip fidelity)
    truncated_bytes: Vec<(String, Vec<u8>)>,
}

impl ModelFactory {
    pub fn new() -> Self {
        Self {
            data: None,
            context: Vec::new(),
            ref_num: Stacked::new(0),
            seq_stack: Vec::new(),
            consumed: 0,
            pmap_bytes: None,
            truncated_bytes: Vec::new(),
        }
    }

    pub(crate) fn set_consumed(&mut self, n: usize) {
        self.consumed = n;
    }

    pub(crate) fn consumed(&self) -> usize {
        self.consumed
    }

    #[allow(dead_code)]
    pub(crate) fn set_pmap_bytes(&mut self, bytes: Vec<u8>) {
        self.pmap_bytes = Some(bytes);
    }

    /// Discard an incomplete sequence item (e.g., when stream is truncated).
    pub(crate) fn discard_sequence_item(&mut self) {
        self.context.pop(); // Pop the item's Group
        let _ = self.ref_num.pop();
    }
}

impl Default for ModelFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelFactory {
    pub(crate) fn start_template(&mut self, _id: u32, name: &str, field_count: usize) {
        self.context.push((
            name.to_string(),
            ValueData::Group(Vec::with_capacity(field_count)),
        ));
    }

    pub(crate) fn stop_template(&mut self) {
        let (name, mut value) = self.context.pop().unwrap();
        // Inject any truncated sequence bytes into the root group
        if !self.truncated_bytes.is_empty() {
            if let ValueData::Group(group) = &mut value {
                for (seq_name, bytes) in self.truncated_bytes.drain(..) {
                    group.push((
                        Rc::from(format!("__{}_trunc__", seq_name)),
                        ValueData::Value(Some(Value::Bytes(bytes))),
                    ));
                }
            }
        }
        self.data = Some(TemplateData {
            name,
            value,
            pmap_bytes: self.pmap_bytes.take(),
        });
    }

    pub(crate) fn set_value(&mut self, _id: u32, name: Rc<str>, value: Option<Value>) {
        let last = self.context.last_mut().unwrap();
        if let ValueData::Group(group) = &mut last.1 {
            group.push((name, ValueData::Value(value)));
        }
    }

    pub(crate) fn start_sequence(&mut self, _id: u32, name: &str, length: u32) {
        // Push onto seq_stack, NOT context stack.
        // Sequences are containers, not contexts — their items use the
        // parent's context (the current Group).
        // Save the current context index as the parent target.
        let parent_idx = self.context.len() - 1;
        self.seq_stack.push((
            name.to_string(),
            Vec::with_capacity(length as usize),
            length,
            parent_idx,
        ));
    }

    pub(crate) fn start_sequence_item(&mut self, _index: u32, pmap_bytes: Option<Vec<u8>>) {
        let mut group = FieldVec::new();
        // Store original segment pmap for round-trip fidelity
        if let Some(bytes) = pmap_bytes {
            group.push((Rc::from("__pmap__"), ValueData::Value(Some(Value::Bytes(bytes)))));
        }
        self.context.push((String::new(), ValueData::Group(group)));
        self.ref_num.push(0);
    }

    pub(crate) fn stop_sequence_item(&mut self) {
        _ = self.ref_num.pop();
        let (_, item) = self.context.pop().unwrap();
        // The item goes into the current active sequence (from seq_stack)
        if let Some(seq_entry) = self.seq_stack.last_mut() {
            seq_entry.1.push(item);
        }
    }

    pub(crate) fn stop_sequence(&mut self) {
        let (n, items, seq_len, parent_idx) = self
            .seq_stack
            .pop()
            .unwrap_or_else(|| (String::new(), Vec::new(), 0, 0));
        let actual_len = items.len() as u32;

        // Insert sequence into parent context at saved index
        if parent_idx < self.context.len() {
            let (_, context) = &mut self.context[parent_idx];
            if let ValueData::Group(group) = context {
                // Store original length for round-trip fidelity (truncated sequences)
                if seq_len != actual_len {
                    group.push((
                        Rc::from(format!("__{}_len__", n)),
                        ValueData::Value(Some(Value::UInt32(seq_len))),
                    ));
                }
                group.push((Rc::from(n), ValueData::Sequence(items)));
            }
        }
    }

    /// Store raw bytes of truncated sequence items for round-trip fidelity.
    /// Cached until stop_template() applies them to the root group.
    pub(crate) fn set_truncated_bytes(&mut self, seq_name: String, bytes: Vec<u8>) {
        self.truncated_bytes.push((seq_name, bytes));
    }

    pub(crate) fn start_group(&mut self, name: &str) {
        self.context
            .push((name.to_string(), ValueData::Group(Vec::new())));
        self.ref_num.push(0);
    }

    pub(crate) fn stop_group(&mut self) {
        _ = self.ref_num.pop();
        let (n, g) = self.context.pop().unwrap();
        let last = self.context.last_mut().unwrap();
        if let ValueData::Group(group) = &mut last.1 {
            group.push((Rc::from(n), g));
        }
    }

    pub(crate) fn start_template_ref(&mut self, name: &str, dynamic: bool) {
        if dynamic {
            let tpl_ref = ValueData::DynamicTemplateRef(Box::new(TemplateData {
                name: name.to_string(),
                value: ValueData::None,
                pmap_bytes: None,
            }));
            let rc = *self.ref_num.must_peek();
            self.context.push((format!("templateRef:{rc}"), tpl_ref));
            *self.ref_num.must_peek_mut() += 1;
        } else {
            let tpl_ref = ValueData::StaticTemplateRef(name.to_string(), Box::new(ValueData::None));
            self.context.push((name.to_string(), tpl_ref));
        }
        self.context
            .push((String::new(), ValueData::Group(Vec::new())));
        self.ref_num.push(0);
    }

    pub(crate) fn stop_template_ref(&mut self) {
        _ = self.ref_num.pop();
        let (_, vg) = self.context.pop().unwrap();
        let (n, vr) = self.context.pop().unwrap();
        let last = self.context.last_mut().unwrap();
        if let ValueData::Group(group) = &mut last.1 {
            match vr {
                ValueData::StaticTemplateRef(_m, _) => {
                    if let ValueData::Group(g) = vg {
                        group.extend(g);
                    }
                }
                ValueData::DynamicTemplateRef(mut t) => {
                    t.value = vg;
                    group.push((Rc::from(n), ValueData::DynamicTemplateRef(t)));
                }
                _ => {}
            }
        }
    }
}
