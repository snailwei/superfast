//! FAST Decoder — decodes binary messages using XML template definitions.

use std::rc::Rc;

use crate::context::{Context, DictionaryType};
use crate::definitions::Definitions;
use crate::errors::{Error, Result};
use crate::instruction::Instruction;
use crate::pmap::PresenceMap;
use crate::reader::FastReader;
use crate::stacked::Stacked;
use crate::template::Template;
use crate::types::{Dictionary, TypeRef};
use crate::value::{Value, ValueType};

/// Decoder for FAST protocol messages.
pub struct FastDecoder {
    pub(crate) definitions: Definitions,
    pub(crate) context: Context,
    pub(crate) strict: bool,
}

impl FastDecoder {
    /// Parse XML template definitions and create a decoder.
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
    /// # Examples
    ///
    /// ```ignore
    /// use superfast::{Dictionary, FastDecoder};
    ///
    /// let mut dec = FastDecoder::new(xml, Dictionary::Global).unwrap();
    /// let (msg, consumed): (MyMessage, u64) = dec.decode(buffer)?;
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Error::Static`] if the XML is malformed or semantically invalid.
    pub fn new(text: &str, default_dict: Dictionary) -> Result<Self> {
        Ok(Self {
            definitions: Definitions::new(text, default_dict)?,
            context: Context::new(),
            strict: true,
        })
    }

    /// Set whether strict mode is enforced for stateful operators.
    ///
    /// `true` (default): spec-compliant ERR D5/D6 on missing copy values.
    /// `false`: use type defaults for mid-stream captured data testing.
    #[allow(dead_code)]
    pub fn set_strict(&mut self, strict: bool) {
        self.strict = strict;
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn reset(&mut self) {
        self.context.reset();
    }

    /// Decode a single FAST message from the start of `buffer`.
    ///
    /// The type parameter `T` is any `serde::Deserialize` type whose field
    /// names match the template's field names — typically a struct defined
    /// for one template, or an enum with `#[serde(rename = "...")]` variants
    /// covering multiple templates.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Decode into a typed struct — one type per template
    /// let (msg, consumed): (NgtsMarketData, u64) = decoder.decode(buf)?;
    ///
    /// // Or decode into an enum covering multiple templates
    /// let (msg, consumed): (Message, u64) = decoder.decode(buf)?;
    /// ```
    ///
    /// Returns `(message, bytes_consumed)`. The `bytes_consumed` is always
    /// `<= buffer.len()`; remaining bytes (`buffer[consumed..]`) can be
    /// passed to a subsequent call to decode more messages.
    ///
    /// # Truncation
    ///
    /// When a sequence length field declares N items but fewer than N
    /// complete items fit in the buffer, the decoder returns the items
    /// it could parse and excludes the partial item from `consumed`
    /// **if the item's pmap read hit EOF immediately**. If the pmap was
    /// read successfully but field decoding failed, the bytes consumed
    /// during the failed decode **are included** in `consumed`.
    ///
    /// # Copy state
    ///
    /// The decoder keeps copy-operator state between calls. Reuse the
    /// same `FastDecoder` instance across calls to maintain state;
    /// create a new instance to reset it.
    pub fn decode<T>(&mut self, buffer: &[u8]) -> Result<(T, u64)>
    where
        T: serde::de::Deserialize<'static>,
    {
        let (data, consumed) = self.parse(buffer)?;
        let msg = T::deserialize(data).map_err(|e| Error::Runtime(e.to_string()))?;
        Ok((msg, consumed as u64))
    }

    /// Parse one FAST message into an intermediate `TemplateData` without
    /// deserializing it into a typed struct.
    ///
    /// Use this when the message type is unknown at compile time or when you
    /// need to inspect the parsed data (field names, values) before deciding
    /// how to deserialize it. The returned [`TemplateData`](crate::model::template::TemplateData)
    /// carries the template name and a map of field names to decoded values,
    /// and can be deserialized later into any `serde::Deserialize` type.
    ///
    /// For known message types, prefer [`decode`](Self::decode) which returns
    /// the typed struct directly.
    ///
    /// Returns `(TemplateData, bytes_consumed)`.
    pub fn parse(&mut self, bytes: &[u8]) -> Result<(crate::model::template::TemplateData, usize)> {
        let mut reader = FastReader::new(bytes);
        let mut ctx = DecoderContext {
            definitions: &mut self.definitions,
            context: &mut self.context,
            rdr: &mut reader,
            template_id: Stacked::new_empty(),
            dictionary: Stacked::new(Dictionary::Global),
            type_ref: Stacked::new(TypeRef::Any),
            presence_map: Stacked::new_empty(),
            model: crate::model::ModelFactory::new(),
            strict: self.strict,
        };
        ctx.decode_template()?;

        let model = std::mem::take(&mut ctx.model);
        let consumed = model.consumed();
        let data = model
            .data
            .ok_or_else(|| Error::Runtime("No data in message".to_string()))?;
        Ok((data, consumed))
    }
}

/// Processing context for a single message decode.
pub(crate) struct DecoderContext<'a> {
    pub(crate) definitions: &'a mut Definitions,
    pub(crate) context: &'a mut Context,
    pub(crate) rdr: &'a mut FastReader<'a>,
    pub(crate) template_id: Stacked<u32>,
    pub(crate) dictionary: Stacked<Dictionary>,
    pub(crate) type_ref: Stacked<TypeRef>,
    pub(crate) presence_map: Stacked<PresenceMap>,
    pub(crate) model: crate::model::ModelFactory,
    pub(crate) strict: bool,
}

impl<'a> DecoderContext<'a> {
    fn read_template_id(&mut self) -> Result<u32> {
        let instruction = self.definitions.template_id_instruction.clone();
        match instruction.extract(self)? {
            Some(Value::UInt32(id)) => Ok(id),
            Some(_) => Err(Error::Runtime(
                "Wrong template id type in context storage".to_string(),
            )),
            None => Err(Error::Runtime(
                "No template id in context storage".to_string(),
            )),
        }
    }

    fn decode_template_id(&mut self) -> Result<()> {
        let template_id = self.read_template_id()?;
        self.template_id.push(template_id);
        Ok(())
    }

    fn drop_template_id(&mut self) {
        self.template_id.pop();
    }

    fn decode_presence_map(&mut self) -> Result<()> {
        match self.rdr.read_presence_map() {
            Ok(pmap) => {
                self.presence_map.push(pmap);
                Ok(())
            }
            Err("eof") => Err(Error::UnexpectedEof),
            Err(_e) => Err(Error::UnexpectedEof),
        }
    }

    fn drop_presence_map(&mut self) {
        _ = self.presence_map.pop();
    }

    pub(crate) fn decode_template(&mut self) -> Result<()> {
        // Capture pmap bytes for round-trip fidelity
        let pmap_start = self.rdr.pos();
        match self.rdr.read_presence_map() {
            Ok(pmap) => {
                let pmap_bytes = self.rdr.buf()[pmap_start..self.rdr.pos()].to_vec();
                self.presence_map.push(pmap);
                self.model.set_pmap_bytes(pmap_bytes);
            }
            Err("eof") => return Err(Error::UnexpectedEof),
            Err(_e) => return Err(Error::UnexpectedEof),
        }
        self.decode_template_id()?;
        let tid = *self.template_id.must_peek();
        let template = self
            .definitions
            .templates_by_id
            .get(&tid)
            .ok_or_else(|| Error::Dynamic(format!("Unknown template id: {}", tid)))?
            .clone();
        self.model.start_template(template.id, &template.name);

        let has_dictionary = self.switch_dictionary(&template.dictionary);
        let has_type_ref = self.switch_type_ref(&template.type_ref);

        self.decode_instructions(&template.instructions)?;

        if has_dictionary {
            self.restore_dictionary();
        }
        if has_type_ref {
            self.restore_type_ref();
        }

        self.model.stop_template();
        self.model.set_consumed(self.rdr.pos());
        self.drop_template_id();
        self.drop_presence_map();
        Ok(())
    }

    fn decode_instructions(&mut self, instructions: &[Instruction]) -> Result<()> {
        for instruction in instructions {
            match instruction.value_type {
                ValueType::Sequence => {
                    self.decode_sequence(instruction)?;
                }
                ValueType::Group => {
                    self.decode_group(instruction)?;
                }
                ValueType::TemplateReference => {
                    self.decode_template_ref(instruction)?;
                }
                _ => {
                    self.decode_field(instruction)?;
                }
            }
        }
        Ok(())
    }

    fn decode_segment(&mut self, instructions: &[Instruction]) -> Result<()> {
        self.decode_presence_map()?;
        self.decode_instructions(instructions)?;
        self.drop_presence_map();
        Ok(())
    }

    fn decode_field(&mut self, instruction: &Instruction) -> Result<()> {
        let value = self.extract_field(instruction)?;
        // extract() already reconstructed the correct value from operators
        // (copy/default/increment/tail) — always set it in the model
        self.model
            .set_value(instruction.id, &instruction.name, value);
        Ok(())
    }

    fn decode_sequence(&mut self, instruction: &Instruction) -> Result<()> {
        let has_dictionary = self.switch_dictionary(&instruction.dictionary);
        let has_type_ref = self.switch_type_ref(&instruction.type_ref);

        let length_instruction = instruction.instructions.first().unwrap();
        match self.extract_field(length_instruction)? {
            None => {}
            Some(Value::UInt32(length)) => {
                self.model
                    .start_sequence(instruction.id, &instruction.name, length);
                let mut truncated_bytes = Vec::new();
                let has_pmap = instruction.has_pmap.get();
                for idx in 0..length {
                    let item_start = self.rdr.pos();

                    // Step 1: Read element pmap (explicitly, without ? so stop_sequence always runs)
                    let mut pmap_bytes: Option<Vec<u8>> = None;
                    if has_pmap {
                        let start = self.rdr.pos();
                        match self
                            .rdr
                            .read_presence_map()
                            .map_err(|_| Error::UnexpectedEof)
                        {
                            Ok(pmap) => {
                                pmap_bytes = Some(self.rdr.buf()[start..self.rdr.pos()].to_vec());
                                self.presence_map.push(pmap);
                            }
                            Err(_) => {
                                // Truncated stream — stop_sequence will still be called
                                break;
                            }
                        }
                    }

                    // Step 2: Push item context
                    self.model.start_sequence_item(idx, pmap_bytes);
                    // Step 3: Decode instructions
                    match self.decode_instructions(&instruction.instructions[1..]) {
                        Ok(()) => {
                            self.model.stop_sequence_item();
                            if has_pmap {
                                self.drop_presence_map();
                            }
                        }
                        Err(Error::UnexpectedEof) => {
                            let consumed = self.rdr.pos() - item_start;
                            if consumed > 0 {
                                truncated_bytes.extend_from_slice(
                                    &self.rdr.buf()[item_start..item_start + consumed],
                                );
                            }
                            self.model.discard_sequence_item();
                            break;
                        }
                        Err(e) => {
                            let consumed = self.rdr.pos() - item_start;
                            if consumed > 0 {
                                truncated_bytes.extend_from_slice(
                                    &self.rdr.buf()[item_start..item_start + consumed],
                                );
                            }
                            self.model.discard_sequence_item();
                            return Err(e);
                        }
                    }
                }
                self.model.stop_sequence();
                if !truncated_bytes.is_empty() {
                    self.model
                        .set_truncated_bytes(instruction.name.clone(), truncated_bytes);
                }
            }
            _ => return Err(Error::Dynamic("Length field must be UInt32".to_string())),
        }

        if has_dictionary {
            self.restore_dictionary();
        }
        if has_type_ref {
            self.restore_type_ref();
        }
        Ok(())
    }

    fn decode_group(&mut self, instruction: &Instruction) -> Result<()> {
        if instruction.is_optional() && !self.pmap_next_bit_set() {
            return Ok(());
        }

        let has_dictionary = self.switch_dictionary(&instruction.dictionary);
        let has_type_ref = self.switch_type_ref(&instruction.type_ref);

        self.model.start_group(&instruction.name);
        if instruction.has_pmap.get() {
            self.decode_segment(&instruction.instructions)?;
        } else {
            self.decode_instructions(&instruction.instructions)?;
        }
        self.model.stop_group();

        if has_dictionary {
            self.restore_dictionary();
        }
        if has_type_ref {
            self.restore_type_ref();
        }
        Ok(())
    }

    fn decode_template_ref(&mut self, instruction: &Instruction) -> Result<()> {
        let is_dynamic = instruction.name.is_empty();

        let template: Rc<Template> = if is_dynamic {
            self.decode_presence_map()?;
            self.decode_template_id()?;
            self.definitions
                .templates_by_id
                .get(self.template_id.peek().unwrap())
                .ok_or_else(|| {
                    Error::Dynamic(format!(
                        "Unknown template id: {}",
                        self.template_id.peek().unwrap()
                    ))
                })?
                .clone()
        } else {
            self.definitions
                .templates_by_name
                .get(&instruction.name)
                .ok_or_else(|| Error::Dynamic(format!("Unknown template: {}", instruction.name)))?
                .clone()
        };
        self.model.start_template_ref(&template.name, is_dynamic);

        let has_dictionary = self.switch_dictionary(&template.dictionary);
        let has_type_ref = self.switch_type_ref(&template.type_ref);

        self.decode_instructions(&template.instructions)?;

        if has_dictionary {
            self.restore_dictionary();
        }
        if has_type_ref {
            self.restore_type_ref();
        }

        self.model.stop_template_ref();
        if is_dynamic {
            self.drop_template_id();
            self.drop_presence_map();
        }
        Ok(())
    }

    fn extract_field(&mut self, instruction: &Instruction) -> Result<Option<Value>> {
        let has_dict = self.switch_dictionary(&instruction.dictionary);
        let value = instruction.extract(self)?;
        if has_dict {
            self.restore_dictionary();
        }
        Ok(value)
    }

    #[inline]
    fn switch_dictionary(&mut self, dictionary: &Dictionary) -> bool {
        self.dictionary.push(dictionary.clone());
        true
    }

    #[inline]
    fn restore_dictionary(&mut self) {
        _ = self.dictionary.pop();
    }

    #[inline]
    fn switch_type_ref(&mut self, type_ref: &TypeRef) -> bool {
        if *type_ref == TypeRef::Any {
            false
        } else {
            self.type_ref.push(type_ref.clone());
            true
        }
    }

    #[inline]
    fn restore_type_ref(&mut self) {
        _ = self.type_ref.pop();
    }

    #[inline]
    pub(crate) fn pmap_next_bit_set(&mut self) -> bool {
        self.presence_map.must_peek_mut().next_bit_set()
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

    fn make_dict_type(&self) -> DictionaryType {
        let dictionary = self.dictionary.must_peek();
        match dictionary {
            Dictionary::Global => DictionaryType::Global,
            Dictionary::Template => DictionaryType::Template(*self.template_id.must_peek()),
            Dictionary::Type => {
                let name = match self.type_ref.must_peek() {
                    TypeRef::Any => Rc::from("__any__"),
                    TypeRef::ApplicationType(name) => name.clone(),
                };
                DictionaryType::Type(name)
            }
            Dictionary::UserDefined(name) => DictionaryType::UserDefined(name.clone()),
        }
    }
}
