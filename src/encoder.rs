//! FAST Encoder — encodes values back into binary using XML template definitions.
//!
//! `inject()` pattern: each field sets its pmap bit AND
//! writes its value atomically, eliminating stale-state issues.

use serde::Serialize;
use std::rc::Rc;

use crate::context::{Context, DictionaryType};
use crate::definitions::Definitions;
use crate::errors::{Error, Result};
use crate::instruction::Instruction;
use crate::model::template::TemplateData;
use crate::model::value::ValueData;
use crate::pmap::PresenceMap;
use crate::template::Template;
use crate::types::{Dictionary, Operator, TypeRef};
use crate::value::{Value, ValueType};
use crate::writer::{FastWriter, FastWriterOwned};

/// Encoder for FAST protocol messages.
pub struct FastEncoder {
    pub(crate) definitions: Definitions,
    pub(crate) context: Context,
}

impl FastEncoder {
    pub fn new(text: &str) -> Result<Self> {
        Ok(Self {
            definitions: Definitions::new(text)?,
            context: Context::new(),
        })
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reset(&mut self) {
        self.context.reset();
    }

    /// Like [`Self::new`] but, when `template_dict` is `true`, sets all
    /// templates to `Dictionary::Template` instead of `Dictionary::Global`,
    /// isolating copy-operator state per template.
    pub fn new_with_template_dict(text: &str) -> Result<Self> {
        Self::new_from_xml(text, true)
    }

    pub(crate) fn new_from_xml(text: &str, template_dict: bool) -> Result<Self> {
        Ok(Self {
            definitions: Definitions::new_from_xml(text, template_dict)?,
            context: Context::new(),
        })
    }

    /// Encode a `serde::Serialize` value into FAST binary bytes.
    ///
    /// The template name is extracted from `#[serde(rename = "...")]` annotations
    /// on structs, or from variant names for enums.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Struct with #[serde(rename = "NGTSMarketData")]
    /// let bytes = enc.encode(&msg)?;
    ///
    /// // Enum: variant name from #[serde(rename = "...")]
    /// let bytes = enc.encode(&msg)?;
    /// ```
    pub fn encode<T: Serialize>(&mut self, msg: &T) -> Result<Vec<u8>> {
        let data = crate::model::serialize::to_template_data(msg)?;
        self.encode_template_data(data)
    }

    /// Encode a `TemplateData` back into bytes.
    pub(crate) fn encode_template_data(&mut self, data: TemplateData) -> Result<Vec<u8>> {
        let template = self
            .definitions
            .templates_by_name
            .get(&data.name)
            .ok_or_else(|| Error::Runtime(format!("template '{}' not found", data.name)))?
            .clone();

        self.context.set(
            DictionaryType::Global,
            Rc::from("__template_id__"),
            Some(Value::UInt32(template.id)),
        );

        let mut wr = FastWriter::new();
        let mut ctx = EncoderContext::new(
            &mut self.definitions,
            &mut self.context,
            &mut wr,
            template.id,
        );
        ctx.encode_template(&template, &data.value, data.pmap_bytes)?;
        Ok(wr.into_inner())
    }
}

/// Encoding state held while processing a single segment (template, group, or sequence item).
pub(crate) struct SegmentState<'a> {
    /// Accumulated pmap bits
    pmap: PresenceMap,
    /// Body bytes written during inject()
    body: Vec<u8>,
    /// Reference to the outer writer
    _outer_wr: std::marker::PhantomData<&'a ()>,
}

impl<'a> SegmentState<'a> {
    fn new() -> Self {
        Self {
            pmap: PresenceMap::empty(),
            body: Vec::new(),
            _outer_wr: std::marker::PhantomData,
        }
    }

    fn write_presence_map_buf(buf: &mut Vec<u8>, bitmap: u64, size: u8) {
        if size == 0 {
            return;
        }
        let mut remaining = size as u32;
        while remaining > 0 {
            let take = remaining.min(7);
            let shift = remaining - take;
            let byte = ((bitmap >> shift) & 0x7F) as u8;
            if remaining <= 7 {
                buf.push(byte | 0x80);
            } else {
                buf.push(byte);
            }
            remaining = remaining.saturating_sub(take);
        }
    }
}

/// Per-message encoding context. Mirrors `DecoderContext`.
pub(crate) struct EncoderContext<'a> {
    definitions: &'a mut Definitions,
    context: &'a mut Context,
    wr: &'a mut FastWriter,
    template_id: u32,
    dictionary: Dictionary,
    type_ref: TypeRef,
    dictionary_depth: usize,
    type_ref_depth: usize,
    /// Saved dictionary/type_ref for nesting
    saved_dictionary: Vec<Dictionary>,
    saved_type_ref: Vec<TypeRef>,
}

impl<'a> EncoderContext<'a> {
    fn new(
        definitions: &'a mut Definitions,
        context: &'a mut Context,
        wr: &'a mut FastWriter,
        template_id: u32,
    ) -> Self {
        Self {
            definitions,
            context,
            wr,
            template_id,
            dictionary: Dictionary::Global,
            type_ref: TypeRef::Any,
            dictionary_depth: 0,
            type_ref_depth: 0,
            saved_dictionary: Vec::new(),
            saved_type_ref: Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // Template encoding — buffered: collect body + pmap, write pmap then body
    // ------------------------------------------------------------------

    pub(crate) fn encode_template(
        &mut self,
        template: &Template,
        data: &ValueData,
        original_pmap: Option<Vec<u8>>,
    ) -> Result<()> {
        self.push_context(template.dictionary.clone(), template.type_ref.clone());

        // Start buffered segment
        let mut seg = SegmentState::new();

        // Template ID is present for message-level segments (per FAST spec: segment ::= PresenceMap TemplateIdentifier? ...)
        seg.pmap.set_next_bit(true);

        // Write template ID varint to body
        let tid_instr = self.definitions.template_id_instruction.clone();
        tid_instr.write_value_buf(&mut seg.body, &Some(Value::UInt32(template.id)))?;

        // Encode fields — inject_buf sets pmap bit + appends to body
        self.encode_instructions_buf(template.instructions.as_slice(), data, &mut seg)?;

        // Flush: write pmap then body
        if let Some(ref bytes) = original_pmap {
            self.wr.write_raw_bytes(bytes);
        } else {
            self.wr
                .write_presence_map(seg.pmap.bitmap(), seg.pmap.size());
        }
        self.wr.write_raw_bytes(&seg.body);

        self.pop_context();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Instruction dispatch (buffered version)
    // ------------------------------------------------------------------

    fn encode_instructions_buf(
        &mut self,
        instructions: &[Instruction],
        data: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        for instr in instructions {
            match instr.value_type {
                ValueType::Sequence => self.encode_sequence_buf(instr, data, seg)?,
                ValueType::Group => self.encode_group_buf(instr, data, seg)?,
                ValueType::TemplateReference => self.encode_template_ref_buf(instr, data, seg)?,
                _ => self.inject_buf(instr, data, seg)?,
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // inject_buf — extract value from data, delegate to Instruction::inject_buf()
    // ------------------------------------------------------------------

    fn inject_buf(
        &mut self,
        instruction: &Instruction,
        data: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        let has_dict = self.push_dict(instruction.dictionary.clone());

        let value = self.resolve_value(instruction, data)?;

        // For Decimal fields with operators on sub-instructions, decompose
        // and encode exponent + mantissa separately so their operators apply.
        if self.try_encode_decimal(instruction, &value, seg)? {
            if has_dict {
                self.pop_dict();
            }
            return Ok(());
        }

        instruction.inject_buf(self, &value, seg)?;
        if has_dict {
            self.pop_dict();
        }
        Ok(())
    }

    /// Resolve the effective value for an instruction, applying operator-specific fallbacks.
    fn resolve_value(
        &mut self,
        instruction: &Instruction,
        data: &ValueData,
    ) -> Result<Option<Value>> {
        let mut value: Option<Value> = match (data, instruction.name.as_str()) {
            (ValueData::Group(group), name) => match group.get(name) {
                Some(ValueData::Value(v)) => v.clone(),
                Some(ValueData::None) => instruction.initial_value.clone(),
                _ => None,
            },
            (ValueData::Value(v), _) => v.clone(),
            _ => None,
        };

        // Operator-specific fallbacks for missing values.
        // Only apply when the field is truly absent, not when it is explicitly
        // NULL on a nullable field (ValueData::Value(None)).
        if value.is_none() {
            // Check if the field was explicitly set to NULL
            let is_explicit_null = match data {
                ValueData::Group(group) => {
                    matches!(
                        group.get(instruction.name.as_str()),
                        Some(ValueData::Value(None))
                    )
                }
                ValueData::Value(None) => true,
                _ => false,
            };
            if !is_explicit_null {
                value = self.operator_fallback(instruction);
            }
        }

        // Validate mandatory fields have values
        if value.is_none() && !instruction.is_optional() && !instruction.is_nullable() {
            if !Self::operator_handles_absent(&instruction.operator) {
                return Err(Error::Runtime(format!(
                    "mandatory field {} has no value",
                    instruction.name
                )));
            }
        }
        Ok(value)
    }

    /// Return the fallback value for an operator when the field is absent from data.
    fn operator_fallback(&mut self, instruction: &Instruction) -> Option<Value> {
        match instruction.operator {
            Operator::Constant | Operator::Default => instruction.initial_value.clone(),
            Operator::Copy | Operator::Increment | Operator::Tail => self
                .context
                .get(self.make_dict_type(), &instruction.key)
                .flatten()
                .or_else(|| instruction.initial_value.clone()),
            Operator::Delta | Operator::None => None,
        }
    }

    /// Check if an operator can handle a missing (None) value without error.
    fn operator_handles_absent(op: &Operator) -> bool {
        matches!(
            op,
            Operator::Copy | Operator::Increment | Operator::Tail | Operator::Delta
        )
    }

    /// Try to encode a Decimal instruction by decomposing into exponent + mantissa.
    /// Returns true if the decimal was handled (encoded or skipped), false to fall through.
    fn try_encode_decimal(
        &mut self,
        instruction: &Instruction,
        value: &Option<Value>,
        seg: &mut SegmentState<'_>,
    ) -> Result<bool> {
        if instruction.value_type != ValueType::Decimal || instruction.instructions.is_empty() {
            return Ok(false);
        }
        let has_operator = instruction
            .instructions
            .iter()
            .any(|si| si.operator != Operator::None);
        if !has_operator {
            return Ok(false);
        }
        let Some(Value::Decimal(d)) = value else {
            // Absent decimal with operators:
            // - Optional: skip encoding (pmap bit = 0)
            // - Mandatory: use previous context value (pmap bit = 0)
            let skip_sub_instructions = if instruction.is_optional() {
                seg.pmap.set_next_bit(false);
                true
            } else {
                // Mandatory absent — don't write sub-instructions,
                // but don't set pmap bit (decoder falls through to context)
                false
            };
            if skip_sub_instructions {
                return Ok(true);
            }
            // For mandatory decimals with operators, use previous context value
            let dict_type = self.make_dict_type();
            let prev = match self.context.get(dict_type, &instruction.key) {
                Some(Some(prev)) => prev,
                _ => match &instruction.initial_value {
                    Some(iv) => iv.clone(),
                    None => return Ok(true), // nothing to encode
                },
            };
            let Value::Decimal(prev_d) = prev else {
                return Ok(true);
            };
            self.push_context(instruction.dictionary.clone(), instruction.type_ref.clone());
            let exp_data = ValueData::Value(Some(Value::Int32(prev_d.exponent)));
            let mant_data = ValueData::Value(Some(Value::Int64(prev_d.mantissa)));
            self.inject_buf(&instruction.instructions[0], &exp_data, seg)?;
            self.inject_buf(&instruction.instructions[1], &mant_data, seg)?;
            self.pop_context();
            return Ok(true);
        };
        // For decimals with sub-instruction operators, always set pmap bit
        // to signal whether sub-instructions are present in the stream.
        if has_operator {
            seg.pmap.set_next_bit(true);
        }
        self.push_context(instruction.dictionary.clone(), instruction.type_ref.clone());
        let exp_data = ValueData::Value(Some(Value::Int32(d.exponent)));
        let mant_data = ValueData::Value(Some(Value::Int64(d.mantissa)));
        self.inject_buf(&instruction.instructions[0], &exp_data, seg)?;
        self.inject_buf(&instruction.instructions[1], &mant_data, seg)?;
        self.pop_context();
        Ok(true)
    }

    // ------------------------------------------------------------------
    // Sequence (buffered)
    // ------------------------------------------------------------------

    fn encode_sequence_buf(
        &mut self,
        instruction: &Instruction,
        data: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        self.push_context(instruction.dictionary.clone(), instruction.type_ref.clone());

        let field_data = match (data, instruction.name.as_str()) {
            (ValueData::Group(group), name) => group.get(name).unwrap_or(&ValueData::None),
            _ => data,
        };

        let seq = match field_data {
            ValueData::Sequence(items) => items,
            _ => &Vec::new(),
        };

        // Write length field — use stored original length for truncated sequences
        let seq_len = match data {
            ValueData::Group(group) => {
                if let Some(ValueData::Value(Some(Value::UInt32(orig_len)))) =
                    group.get(&format!("__{}_len__", instruction.name))
                {
                    *orig_len
                } else {
                    seq.len() as u32
                }
            }
            _ => seq.len() as u32,
        };

        let length_instr = instruction.instructions.first().unwrap();

        // For sequences where field_data is None (absent from data), pass ValueData::None
        // to the length encoder so the operator (Default/Copy/Increment) can
        // compare against the initial/previous value and potentially skip writing.
        // This distinguishes between "no sequence" (None) and "empty sequence" (Some([])).
        let length_data: ValueData = if *field_data == ValueData::None {
            ValueData::None
        } else {
            ValueData::Value(Some(Value::UInt32(seq_len)))
        };
        self.inject_buf(length_instr, &length_data, seg)?;

        for item in seq {
            if instruction.has_pmap.get() {
                self.encode_sequence_item_buf(&instruction.instructions[1..], item, seg)?;
            } else {
                self.encode_instructions_buf(&instruction.instructions[1..], item, seg)?;
            }
        }

        // Replay truncated bytes for round-trip fidelity
        if let ValueData::Group(group) = data {
            if let Some(ValueData::Value(Some(Value::Bytes(truncated)))) =
                group.get(&format!("__{}_trunc__", instruction.name))
            {
                seg.body.extend_from_slice(truncated);
            }
        }

        self.pop_context();
        Ok(())
    }

    /// Encode a sequence item segment with its own pmap.
    fn encode_sequence_item_buf(
        &mut self,
        instructions: &[Instruction],
        item: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        // Check for stored pmap bytes from decoder
        let stored_pmap = if let ValueData::Group(group) = item {
            group.get("__pmap__").and_then(|v| match v {
                ValueData::Value(Some(Value::Bytes(b))) => Some(b.clone()),
                _ => None,
            })
        } else {
            None
        };

        // Create nested segment for this item
        let mut item_seg = SegmentState::new();
        self.encode_instructions_buf(instructions, item, &mut item_seg)?;

        // Write item pmap + body to parent segment's body
        if let Some(ref bytes) = stored_pmap {
            seg.body.extend_from_slice(bytes);
        } else {
            SegmentState::write_presence_map_buf(
                &mut seg.body,
                item_seg.pmap.bitmap(),
                item_seg.pmap.size(),
            );
        }
        seg.body.extend_from_slice(&item_seg.body);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Group (buffered)
    // ------------------------------------------------------------------

    fn encode_group_buf(
        &mut self,
        instruction: &Instruction,
        data: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        self.push_context(instruction.dictionary.clone(), instruction.type_ref.clone());

        let field_data = match (data, &instruction.name) {
            (ValueData::Group(group), name) => group.get(name).unwrap_or(&ValueData::None),
            _ => data,
        };

        if instruction.is_optional() && *field_data == ValueData::None {
            // Optional group absent — set pmap bit to false if group contributes one
            seg.pmap.set_next_bit(false);
            self.pop_context();
            return Ok(());
        }

        // Optional groups always contribute a pmap bit to the parent
        if instruction.is_optional() {
            seg.pmap.set_next_bit(true);
        }

        if instruction.has_pmap.get() {
            // Encode group as a sub-segment with its own pmap
            let mut sub_seg = SegmentState::new();
            self.encode_instructions_buf(&instruction.instructions, field_data, &mut sub_seg)?;

            // Write sub-segment pmap + body to parent
            SegmentState::write_presence_map_buf(
                &mut seg.body,
                sub_seg.pmap.bitmap(),
                sub_seg.pmap.size(),
            );
            seg.body.extend_from_slice(&sub_seg.body);
        } else {
            self.encode_instructions_buf(&instruction.instructions, field_data, seg)?;
        }

        self.pop_context();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Template reference (buffered)
    // ------------------------------------------------------------------

    fn encode_template_ref_buf(
        &mut self,
        instruction: &Instruction,
        data: &ValueData,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        let is_dynamic = instruction.name.is_empty();

        let inner_data: &ValueData = if is_dynamic {
            // For dynamic templateRef, extract DynamicTemplateRef from data.
            // When data is a parent Group, find the first DynamicTemplateRef value.
            match data {
                ValueData::DynamicTemplateRef(t) => &t.value,
                ValueData::Group(g) => {
                    // Find first DynamicTemplateRef in group
                    let found = g
                        .values()
                        .find(|v| matches!(v, ValueData::DynamicTemplateRef(_)));
                    match found {
                        Some(ValueData::DynamicTemplateRef(t)) => &t.value,
                        _ => &ValueData::None,
                    }
                }
                _ => &ValueData::None,
            }
        } else {
            // Static templateRef: fields merge into parent under the ref name
            match data {
                ValueData::Group(g) => g.get(&instruction.name).unwrap_or(&ValueData::None),
                _ => &ValueData::None,
            }
        };

        let template: Rc<Template> = if is_dynamic {
            // Determine template name from data
            let tpl_name = match data {
                ValueData::DynamicTemplateRef(t) => t.name.clone(),
                ValueData::Group(g) => {
                    if let Some(ValueData::DynamicTemplateRef(t)) = g
                        .values()
                        .find(|v| matches!(v, ValueData::DynamicTemplateRef(_)))
                    {
                        t.name.clone()
                    } else {
                        return Err(Error::Runtime(
                            "no DynamicTemplateRef found in data".to_string(),
                        ));
                    }
                }
                _ => return Err(Error::Runtime("expected DynamicTemplateRef".to_string())),
            };
            self.definitions
                .templates_by_name
                .get(&tpl_name)
                .ok_or_else(|| Error::Runtime(format!("template '{}' not found", tpl_name)))?
                .clone()
        } else {
            self.definitions
                .templates_by_name
                .get(&instruction.name)
                .ok_or_else(|| Error::Dynamic(format!("Unknown template: {}", instruction.name)))?
                .clone()
        };

        self.push_context(template.dictionary.clone(), template.type_ref.clone());

        if is_dynamic {
            // Dynamic templateRef: write sub-segment with pmap + template ID + fields
            let mut sub_seg = SegmentState::new();
            // Template ID always present
            sub_seg.pmap.set_next_bit(true);
            let tid_instr = self.definitions.template_id_instruction.clone();
            tid_instr.write_value_buf(&mut sub_seg.body, &Some(Value::UInt32(template.id)))?;
            self.encode_instructions_buf(&template.instructions, inner_data, &mut sub_seg)?;
            SegmentState::write_presence_map_buf(
                &mut seg.body,
                sub_seg.pmap.bitmap(),
                sub_seg.pmap.size(),
            );
            seg.body.extend_from_slice(&sub_seg.body);
        } else {
            self.encode_instructions_buf(&template.instructions, inner_data, seg)?;
        }

        self.pop_context();
        Ok(())
    }

    // ------------------------------------------------------------------
    // Dictionary / typeRef context management
    // ------------------------------------------------------------------

    #[inline]
    fn push_context(&mut self, dictionary: Dictionary, type_ref: TypeRef) {
        if dictionary != Dictionary::Inherit {
            self.saved_dictionary
                .push(std::mem::replace(&mut self.dictionary, dictionary));
            self.dictionary_depth += 1;
        }
        if type_ref != TypeRef::Any {
            self.saved_type_ref
                .push(std::mem::replace(&mut self.type_ref, type_ref));
            self.type_ref_depth += 1;
        }
    }

    #[inline]
    fn pop_context(&mut self) {
        if self.dictionary_depth > 0 {
            self.dictionary_depth -= 1;
            self.dictionary = self.saved_dictionary.pop().unwrap();
        }
        if self.type_ref_depth > 0 {
            self.type_ref_depth -= 1;
            self.type_ref = self.saved_type_ref.pop().unwrap();
        }
    }

    #[inline]
    fn push_dict(&mut self, dictionary: Dictionary) -> bool {
        if dictionary == Dictionary::Inherit {
            false
        } else {
            self.saved_dictionary
                .push(std::mem::replace(&mut self.dictionary, dictionary));
            self.dictionary_depth += 1;
            true
        }
    }

    #[inline]
    fn pop_dict(&mut self) {
        self.dictionary_depth -= 1;
        self.dictionary = self.saved_dictionary.pop().unwrap();
    }

    fn make_dict_type(&self) -> DictionaryType {
        match self.dictionary {
            Dictionary::Inherit => unreachable!(),
            Dictionary::Global => DictionaryType::Global,
            Dictionary::Template => DictionaryType::Template(self.template_id),
            Dictionary::Type => {
                let name = match self.type_ref {
                    TypeRef::Any => Rc::from("__any__"),
                    TypeRef::ApplicationType(ref name) => name.clone(),
                };
                DictionaryType::Type(name)
            }
            Dictionary::UserDefined(ref name) => DictionaryType::UserDefined(name.clone()),
        }
    }

    #[inline]
    pub(crate) fn ctx_set(&mut self, i: &Instruction, v: Option<Value>) {
        self.context.set(self.make_dict_type(), i.key.clone(), v);
    }

    #[inline]
    pub(crate) fn ctx_get(&mut self, i: &Instruction) -> Result<Option<Option<Value>>> {
        let v = self.context.get(self.make_dict_type(), &i.key);
        if let Some(Some(ref v)) = v
            && !i.value_type.matches(v)
        {
            return Err(Error::Runtime(format!(
                "field {} has wrong value type in context",
                i.name
            )));
        }
        Ok(v)
    }
}

// ---------------------------------------------------------------------
// Instruction::inject_buf()
// Sets pmap bit (on segment) AND writes value (to segment body) atomically
// ---------------------------------------------------------------------

impl Instruction {
    pub(crate) fn inject_buf(
        &self,
        s: &mut EncoderContext<'_>,
        value: &Option<Value>,
        seg: &mut SegmentState<'_>,
    ) -> Result<()> {
        if value.is_none() && !self.is_optional() && !self.is_nullable() {
            // For operators that provide implicit values from context, None is valid
            match self.operator {
                Operator::Copy
                | Operator::Default
                | Operator::Increment
                | Operator::Delta
                | Operator::Tail => {
                    // OK — value comes from context or initial_value
                }
                Operator::None => {
                    return Err(Error::Runtime(format!(
                        "mandatory field {} has no value",
                        self.name
                    )));
                }
                Operator::Constant => {
                    // Constant doesn't need a value on wire
                }
            }
        }

        match self.operator {
            Operator::None => {
                self.write_value_buf(&mut seg.body, value)?;
            }
            Operator::Constant => {
                if value.is_some() && self.initial_value.as_ref() != value.as_ref() {
                    return Err(Error::Runtime(format!(
                        "constant field {} has wrong value",
                        self.name
                    )));
                }
                if self.is_optional() {
                    seg.pmap.set_next_bit(value.is_some());
                }
            }
            Operator::Default => {
                if self.initial_value.as_ref() == value.as_ref() {
                    seg.pmap.set_next_bit(false);
                } else {
                    seg.pmap.set_next_bit(true);

                    self.write_value_buf(&mut seg.body, value)?;
                }
            }
            Operator::Copy => {
                let prev_value = match s.ctx_get(self)? {
                    Some(v) => v,
                    None => {
                        s.ctx_set(self, self.initial_value.clone());
                        self.initial_value.clone()
                    }
                };
                if prev_value == *value {
                    seg.pmap.set_next_bit(false);
                } else {
                    seg.pmap.set_next_bit(true);

                    s.ctx_set(self, value.clone());
                    self.write_value_buf(&mut seg.body, value)?;
                }
            }
            Operator::Increment => {
                let prev_value = s
                    .ctx_get(self)?
                    .flatten()
                    .or_else(|| self.initial_value.clone());
                let next_value = match prev_value {
                    Some(v) => Some(v.increment()?),
                    None => None,
                };
                s.ctx_set(self, value.clone());
                if next_value == *value {
                    seg.pmap.set_next_bit(false);
                } else {
                    seg.pmap.set_next_bit(true);

                    self.write_value_buf(&mut seg.body, value)?;
                }
            }
            Operator::Delta => match value {
                Some(v) => {
                    let base = match s.ctx_get(self)? {
                        Some(Some(ref prev)) => prev.clone(),
                        Some(None) => {
                            return Err(Error::Runtime(
                                "delta operator has empty previous value".to_string(),
                            ));
                        }
                        None => match &self.initial_value {
                            Some(iv) => iv.clone(),
                            None => self.value_type.default_value()?,
                        },
                    };
                    s.ctx_set(self, Some(v.clone()));
                    match self.value_type {
                        ValueType::AsciiString => {
                            self.write_string_delta_buf(&mut seg.body, v, &base, true)?;
                        }
                        ValueType::UnicodeString | ValueType::Bytes => {
                            self.write_bytes_delta_buf(&mut seg.body, v, &base)?;
                        }
                        _ => {
                            let delta = v.compute_delta(&base)?;
                            self.write_delta_buf(&mut seg.body, &Some(delta))?;
                        }
                    }
                }
                None => {
                    self.write_delta_buf(&mut seg.body, &None)?;
                }
            },
            Operator::Tail => {
                let prev_value = s
                    .ctx_get(self)?
                    .flatten()
                    .or_else(|| self.initial_value.clone());
                if prev_value == *value {
                    seg.pmap.set_next_bit(false);

                    s.ctx_set(self, value.clone());
                } else {
                    let tail = match value {
                        None => {
                            // NULL for nullable field — sets previous state to "empty"
                            s.ctx_set(self, None);
                            None
                        }
                        Some(v) => {
                            s.ctx_set(self, value.clone());
                            let prev = match prev_value {
                                Some(p) => p,
                                None => self.value_type.default_value()?,
                            };
                            Some(v.compute_tail(&prev)?)
                        }
                    };
                    seg.pmap.set_next_bit(true);

                    self.write_tail_buf(&mut seg.body, &tail)?;
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Buffer-aware value writing
    // ------------------------------------------------------------------

    fn write_value_buf(&self, buf: &mut Vec<u8>, value: &Option<Value>) -> Result<()> {
        // Use a temporary FastWriter that writes to buf
        let mut w = FastWriter::from_buf(buf);
        self.write_value(&mut w, value)
    }

    fn write_delta_buf(&self, buf: &mut Vec<u8>, value: &Option<Value>) -> Result<()> {
        let mut w = FastWriter::from_buf(buf);
        match self.value_type {
            ValueType::UInt32
            | ValueType::Int32
            | ValueType::UInt64
            | ValueType::Int64
            | ValueType::Length
            | ValueType::Exponent
            | ValueType::Mantissa => match value {
                None => self.write_int_wr(&mut w, None::<i64>),
                Some(Value::Int64(v)) => self.write_int_wr(&mut w, Some(*v)),
                Some(v) => Err(Error::Runtime(format!(
                    "{} field's delta must be Int64, got: {:?}",
                    self.name, v
                ))),
            },
            _ => unreachable!(),
        }
    }

    /// Write string delta: subtraction length (unsigned int, value+1 per FAST §4.7) + new data.
    fn write_string_delta_buf(
        &self,
        buf: &mut Vec<u8>,
        value: &Value,
        base: &Value,
        _is_ascii: bool,
    ) -> Result<()> {
        let Value::AsciiString(new) = value else {
            return Err(Error::Runtime(format!(
                "{} delta expected AsciiString, got: {:?}",
                self.name, value
            )));
        };
        // Compute common prefix length, then subtraction = chars after prefix.
        let (common, sub_len) = match base {
            Value::AsciiString(b) => {
                let common = b
                    .chars()
                    .zip(new.chars())
                    .take_while(|(o, n)| o == n)
                    .count();
                (common, b.chars().count() - common)
            }
            _ => (0, 0),
        };
        let mut w = FastWriter::from_buf(buf);
        // FAST §4.7: unsigned integer encoding with value incremented by one
        w.write_uint(sub_len as u64 + 1);
        // Write new data (suffix after common prefix)
        let suffix = new.chars().skip(common).collect::<String>();
        w.write_ascii_string(&suffix);
        Ok(())
    }

    /// Write byte vector delta: subtraction length (unsigned int, value+1 per FAST §4.7) + new data.
    fn write_bytes_delta_buf(&self, buf: &mut Vec<u8>, value: &Value, base: &Value) -> Result<()> {
        let new_bytes = match value {
            Value::UnicodeString(s) => s.as_bytes(),
            Value::Bytes(b) => b.as_slice(),
            _ => {
                return Err(Error::Runtime(format!(
                    "{} delta expected Bytes/UnicodeString, got: {:?}",
                    self.name, value
                )));
            }
        };
        // Compute common prefix, then subtraction = bytes after prefix.
        let (common, sub_len) = match base {
            Value::UnicodeString(b) => {
                let b_slice = b.as_bytes();
                let common = b_slice
                    .iter()
                    .zip(new_bytes.iter())
                    .take_while(|(o, n)| o == n)
                    .count();
                (common, b_slice.len() - common)
            }
            Value::Bytes(b) => {
                let common = b
                    .iter()
                    .zip(new_bytes.iter())
                    .take_while(|(o, n)| o == n)
                    .count();
                (common, b.len() - common)
            }
            _ => (0, 0),
        };
        let mut w = FastWriter::from_buf(buf);
        // FAST §4.7: unsigned integer encoding with value incremented by one
        w.write_uint(sub_len as u64 + 1);
        // Write new data (suffix after common prefix)
        let suffix = &new_bytes[common..];
        w.write_uint(suffix.len() as u64);
        w.write_raw_bytes(suffix);
        Ok(())
    }

    fn write_tail_buf(&self, buf: &mut Vec<u8>, tail: &Option<Value>) -> Result<()> {
        let mut w = FastWriter::from_buf(buf);
        match self.value_type {
            ValueType::AsciiString => match tail {
                None => self.write_ascii_string_wr(&mut w, None),
                Some(Value::AsciiString(s)) => self.write_ascii_string_wr(&mut w, Some(s)),
                Some(v) => Err(Error::Runtime(format!(
                    "{} field's tail must be AsciiString, got: {:?}",
                    self.name, v
                ))),
            },
            ValueType::UnicodeString | ValueType::Bytes => match tail {
                None => self.write_bytes_wr(&mut w, None),
                Some(Value::Bytes(b)) => self.write_bytes_wr(&mut w, Some(b)),
                Some(v) => Err(Error::Runtime(format!(
                    "{} field's tail must be Bytes, got: {:?}",
                    self.name, v
                ))),
            },
            _ => unreachable!(),
        }
    }
}

// ---------------------------------------------------------------------
// FastWriter extension for buffer-aware operations
// ---------------------------------------------------------------------
// Instruction value writing — dispatches to FastWriterOwned
// ---------------------------------------------------------------------

impl Instruction {
    fn write_value(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match self.value_type {
            ValueType::UInt32 | ValueType::Length => self.write_uint32_val(w, value),
            ValueType::Int32 => self.write_int32_val(w, value),
            ValueType::UInt64 => self.write_uint64_val(w, value),
            ValueType::Int64 | ValueType::Mantissa => self.write_int64_val(w, value),
            ValueType::Exponent => self.write_exponent_val(w, value),
            ValueType::Decimal => self.write_decimal(w, value),
            ValueType::AsciiString => self.write_ascii_val(w, value),
            ValueType::UnicodeString => self.write_unicode_val(w, value),
            ValueType::Bytes => self.write_bytes_val(w, value, "Bytes"),
            _ => unreachable!(),
        }
    }

    fn write_uint32_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_uint_wr(w, None::<u32>),
            Some(Value::UInt32(v)) => self.write_uint_wr(w, Some(*v)),
            _ => Err(self.type_mismatch_err("UInt32", value)),
        }
    }

    fn write_int32_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_int_wr(w, None::<i32>),
            Some(Value::Int32(v)) => self.write_int_wr(w, Some(*v)),
            _ => Err(self.type_mismatch_err("Int32", value)),
        }
    }

    fn write_uint64_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_uint_wr(w, None::<u64>),
            Some(Value::UInt64(v)) => self.write_uint_wr(w, Some(*v)),
            _ => Err(self.type_mismatch_err("UInt64", value)),
        }
    }

    fn write_int64_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_int_wr(w, None::<i64>),
            Some(Value::Int64(v)) => self.write_int_wr(w, Some(*v)),
            _ => Err(self.type_mismatch_err("Int64", value)),
        }
    }

    fn write_exponent_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_int_wr(w, None::<i32>),
            Some(Value::Int32(v)) => self.write_int_wr(w, Some(*v)),
            _ => Err(Error::Runtime(format!(
                "Field {}:exponent must have Int32 value, got: {:?}",
                self.name, value
            ))),
        }
    }

    fn write_ascii_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_ascii_string_wr(w, None),
            Some(Value::AsciiString(v)) => self.write_ascii_string_wr(w, Some(v)),
            Some(Value::UnicodeString(v)) => {
                if v.is_ascii() {
                    self.write_ascii_string_wr(w, Some(v))
                } else {
                    Err(Error::Runtime(format!(
                        "Field {} must be valid ASCII string",
                        self.name
                    )))
                }
            }
            _ => Err(self.type_mismatch_err("ASCIIString", value)),
        }
    }

    fn write_unicode_val(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => self.write_unicode_string_wr(w, None),
            Some(Value::UnicodeString(v) | Value::AsciiString(v)) => {
                self.write_unicode_string_wr(w, Some(v))
            }
            _ => Err(self.type_mismatch_err("UnicodeString", value)),
        }
    }

    fn write_bytes_val(
        &self,
        w: &mut FastWriterOwned<'_>,
        value: &Option<Value>,
        expected: &str,
    ) -> Result<()> {
        match value {
            None => self.write_bytes_wr(w, None),
            Some(Value::Bytes(v)) => self.write_bytes_wr(w, Some(v)),
            _ => Err(self.type_mismatch_err(expected, value)),
        }
    }

    fn type_mismatch_err(&self, expected: &str, value: &Option<Value>) -> Error {
        Error::Runtime(format!(
            "Field {} must have {} value, got: {:?}",
            self.name, expected, value
        ))
    }

    fn write_uint_wr<T>(&self, w: &mut FastWriterOwned<'_>, value: Option<T>) -> Result<()>
    where
        T: Into<u64>,
    {
        let v = value.map(Into::into);
        if self.is_nullable() {
            w.write_uint_nullable(v);
            Ok(())
        } else {
            w.write_uint(v.ok_or_else(|| {
                Error::Runtime(format!("mandatory field {} has no value", self.name))
            })?);
            Ok(())
        }
    }

    fn write_int_wr<T>(&self, w: &mut FastWriterOwned<'_>, value: Option<T>) -> Result<()>
    where
        T: Into<i64>,
    {
        let v = value.map(Into::into);
        if self.is_nullable() {
            w.write_int_nullable(v);
            Ok(())
        } else {
            w.write_int(v.ok_or_else(|| {
                Error::Runtime(format!("mandatory field {} has no value", self.name))
            })?);
            Ok(())
        }
    }

    fn write_ascii_string_wr(
        &self,
        w: &mut FastWriterOwned<'_>,
        value: Option<&String>,
    ) -> Result<()> {
        if self.is_nullable() {
            w.write_ascii_string_nullable(value.cloned());
        } else {
            w.write_ascii_string(value.ok_or_else(|| {
                Error::Runtime(format!("mandatory field {} has no value", self.name))
            })?);
        }
        Ok(())
    }

    fn write_unicode_string_wr(
        &self,
        w: &mut FastWriterOwned<'_>,
        value: Option<&String>,
    ) -> Result<()> {
        if self.is_nullable() {
            w.write_unicode_string_nullable(value.cloned());
        } else {
            w.write_unicode_string(value.ok_or_else(|| {
                Error::Runtime(format!("mandatory field {} has no value", self.name))
            })?);
        }
        Ok(())
    }

    fn write_bytes_wr(&self, w: &mut FastWriterOwned<'_>, value: Option<&Vec<u8>>) -> Result<()> {
        if self.is_nullable() {
            w.write_bytes_nullable(value.map(|v| &**v));
        } else {
            w.write_bytes(value.ok_or_else(|| {
                Error::Runtime(format!("mandatory field {} has no value", self.name))
            })?);
        }
        Ok(())
    }

    fn write_decimal(&self, w: &mut FastWriterOwned<'_>, value: &Option<Value>) -> Result<()> {
        match value {
            None => {
                let exponent_instr = self
                    .instructions
                    .first()
                    .ok_or_else(|| Error::Runtime("exponent field not found".to_string()))?;
                exponent_instr.write_value(w, &None)?;
                Ok(())
            }
            Some(Value::Decimal(d)) => {
                let exponent_instr = self
                    .instructions
                    .first()
                    .ok_or_else(|| Error::Runtime("exponent field not found".to_string()))?;
                exponent_instr.write_value(w, &Some(Value::Int32(d.exponent)))?;

                let mantissa_instr = self
                    .instructions
                    .get(1)
                    .ok_or_else(|| Error::Runtime("mantissa field not found".to_string()))?;
                mantissa_instr.write_value(w, &Some(Value::Int64(d.mantissa)))?;
                Ok(())
            }
            _ => Err(Error::Runtime(format!(
                "Field {} must have Decimal value, got: {:?}",
                self.name, value
            ))),
        }
    }
}

// ---------------------------------------------------------------------
// Value helpers
// ---------------------------------------------------------------------

impl Value {
    /// Compute the delta from base to self.
    pub(crate) fn compute_delta(&self, base: &Value) -> Result<Value> {
        match (base, self) {
            (Value::UInt32(b), Value::UInt32(n)) => Ok(Value::Int64(*n as i64 - *b as i64)),
            (Value::Int32(b), Value::Int32(n)) => Ok(Value::Int64(*n as i64 - *b as i64)),
            (Value::UInt64(b), Value::UInt64(n)) => Ok(Value::Int64(*n as i64 - *b as i64)),
            (Value::Int64(b), Value::Int64(n)) => Ok(Value::Int64(*n - *b)),
            (Value::Decimal(b), Value::Decimal(n)) => Ok(Value::Int64(n.mantissa - b.mantissa)),
            _ => Err(Error::Runtime(
                "cannot compute delta for these value types".to_string(),
            )),
        }
    }

    /// Compute the tail of self relative to base.
    ///
    /// Per FAST §4.8: the decoder reconstructs the new value as
    /// `base[0 : base_len - tail_len] + tail`. We find the longest
    /// common prefix, then write only the replacement suffix.
    pub(crate) fn compute_tail(&self, base: &Value) -> Result<Value> {
        match (base, self) {
            (Value::AsciiString(b), Value::AsciiString(n)) => {
                let prefix_len = b.chars().zip(n.chars()).take_while(|(a, c)| a == c).count();
                let b_len = b.chars().count();
                let n_len = n.chars().count();
                let tail_str: String = n.chars().skip(prefix_len).collect();
                // Decoder removes tail_len chars from base end, appends tail.
                // Result length = base_len - tail_len + tail_len = base_len.
                // So tail can only represent strings <= base_len.
                // If new is longer (append), send full string.
                let result = if n_len > b_len || tail_str.chars().count() >= b_len {
                    Value::AsciiString(n.clone())
                } else {
                    Value::AsciiString(tail_str)
                };
                Ok(result)
            }
            (Value::UnicodeString(b), Value::UnicodeString(n)) => {
                let prefix_len = b.chars().zip(n.chars()).take_while(|(a, c)| a == c).count();
                let b_len = b.as_bytes().len();
                let n_len = n.as_bytes().len();
                let tail_bytes: Vec<u8> =
                    n.chars().skip(prefix_len).collect::<String>().into_bytes();
                let result = if n_len > b_len || tail_bytes.len() >= b_len {
                    Value::Bytes(n.as_bytes().to_vec())
                } else {
                    Value::Bytes(tail_bytes)
                };
                Ok(result)
            }
            (Value::Bytes(b), Value::Bytes(n)) => {
                let prefix_len = b.iter().zip(n.iter()).take_while(|(a, c)| a == c).count();
                let b_len = b.len();
                let n_len = n.len();
                let tail_bytes = n[prefix_len..].to_vec();
                let result = if n_len > b_len || tail_bytes.len() >= b_len {
                    Value::Bytes(n.clone())
                } else {
                    Value::Bytes(tail_bytes)
                };
                Ok(result)
            }
            _ => Err(Error::Runtime(
                "cannot compute tail for these value types".to_string(),
            )),
        }
    }
}

// ---------------------------------------------------------------------
// Make encode_signed public within the crate
// ---------------------------------------------------------------------

#[allow(dead_code)]
mod writer_reexport {
    /// Encode a signed i64 into a byte array (MSB-first, 7-bit chunks, stop bit on last).
    pub(crate) fn encode_signed(number: i64, len: usize) -> [u8; 10] {
        let mut buf = [0; 10];
        for i in 0..len {
            let offset_bits_index = len - i - 1;
            let shifted = number >> (offset_bits_index * 7);
            buf[i] = (shifted & 0x7F) as u8;
        }
        buf[len - 1] |= 0x80;
        buf
    }
}
