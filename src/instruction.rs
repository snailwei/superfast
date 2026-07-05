//! FAST Field Instruction — parses XML and extracts field values.

use std::cell::Cell;
use std::ops::RangeInclusive;
use std::rc::Rc;

use roxmltree::Node;

use crate::decimal::Decimal;
use crate::decoder::DecoderContext;
use crate::errors::{Error, Result};
use crate::types::{Dictionary, Operator, Presence, TypeRef};
use crate::value::{Value, ValueType};

const MAX_EXPONENT: i32 = 63;
const MIN_EXPONENT: i32 = -63;

/// # Field Instruction
#[derive(Debug)]
pub(crate) struct Instruction {
    pub(crate) id: u32,
    pub(crate) name: String,
    pub(crate) value_type: ValueType,
    pub(crate) presence: Presence,
    pub(crate) nullable: bool,
    pub(crate) operator: Operator,
    pub(crate) initial_value: Option<Value>,
    pub(crate) instructions: Vec<Instruction>,
    pub(crate) dictionary: Dictionary,
    pub(crate) type_ref: TypeRef,
    pub(crate) key: Rc<str>,
    pub(crate) has_pmap: Cell<bool>,
    /// Whether this field was present in the stream during the last decode.
    /// Only meaningful for fields that check the pmap (Default, Copy, Increment, Tail).
    pub(crate) was_present: Cell<Option<bool>>,
}

impl Instruction {
    fn new(id: u32, name: &str, type_: ValueType) -> Self {
        let (nm, ky) = match type_ {
            ValueType::Mantissa | ValueType::Exponent => (String::new(), String::new()),
            _ => (name.to_string(), name.to_string()),
        };
        Self {
            id,
            name: nm,
            value_type: type_,
            presence: Presence::Mandatory,
            nullable: false,
            operator: Operator::None,
            initial_value: None,
            instructions: Vec::new(),
            dictionary: Dictionary::Inherit,
            type_ref: TypeRef::Any,
            key: Rc::from(ky),
            has_pmap: Cell::new(false),
            was_present: Cell::new(None),
        }
    }

    pub fn from_node(node: Node) -> Result<Self> {
        let (id, name, type_) = Self::parse_node_header(node)?;
        Self::validate_header(id, &name, type_)?;

        let mut instruction = Self::new(id, &name, type_);
        Self::apply_common_attrs(&mut instruction, node);
        Self::parse_children(&mut instruction, node)?;
        instruction.check_is_valid()?;
        Ok(instruction)
    }

    /// Parse id, name, charset, and value type from the XML node.
    fn parse_node_header(node: Node) -> Result<(u32, String, ValueType)> {
        let id = node.attribute("id").unwrap_or("0").parse::<u32>()?;
        let name = node.attribute("name").unwrap_or("").to_string();
        let unicode = match node.attribute("charset") {
            Some("unicode") => true,
            Some(charset) => return Err(Error::Static(format!("unknown charset: {charset}"))),
            _ => false,
        };
        let type_ = ValueType::from_tag(node.tag_name().name(), unicode)?;
        Ok((id, name, type_))
    }

    /// Validate that id/name meet requirements for the value type.
    fn validate_header(id: u32, name: &str, type_: ValueType) -> Result<()> {
        match type_ {
            ValueType::Mantissa
            | ValueType::Exponent
            | ValueType::Length
            | ValueType::Sequence
            | ValueType::Group
            | ValueType::TemplateReference => {}
            _ if id == 0 => {
                return Err(Error::Runtime(
                    "instruction must have non-zero 'id' attribute".to_string(),
                ));
            }
            _ => {}
        }
        match type_ {
            ValueType::Mantissa
            | ValueType::Exponent
            | ValueType::Length
            | ValueType::TemplateReference => {}
            _ if name.is_empty() => {
                return Err(Error::Runtime(
                    "instruction must have 'name' attribute".to_string(),
                ));
            }
            _ => {}
        }
        Ok(())
    }

    /// Apply common XML attributes (presence, nullable, dictionary, key, typeRef).
    fn apply_common_attrs(instr: &mut Self, node: Node) {
        if let Some(p) = node.attribute("presence") {
            if let Ok(presence) = Presence::from_str(p) {
                instr.presence = presence;
            }
        }
        if node.attribute("nullable") == Some("true") {
            instr.nullable = true;
        }
        if let Some(d) = node.attribute("dictionary") {
            instr.dictionary = Dictionary::from_str(d);
        }
        if let Some(k) = node.attribute("key") {
            instr.key = Rc::from(k);
        }
        if let Some(k) = node.attribute("typeRef") {
            instr.type_ref = TypeRef::from_str(k);
        }
    }

    /// Dispatch to type-specific child parsing based on value_type.
    fn parse_children(instr: &mut Self, node: Node) -> Result<()> {
        match instr.value_type {
            ValueType::TemplateReference => Ok(()),
            ValueType::Group => Self::parse_group_children(instr, node),
            ValueType::Sequence => Self::parse_sequence_children(instr, node),
            ValueType::Decimal => Self::parse_decimal_children(instr, node),
            ValueType::UnicodeString | ValueType::Bytes => {
                Self::parse_string_with_length(instr, node)
            }
            _ => Self::parse_default_operator(instr, node),
        }
    }

    /// Parse children for Unicode strings and byte vectors.
    /// Handles explicit \<length\> children as length handles, then parses the operator.
    fn parse_string_with_length(instr: &mut Self, node: Node) -> Result<()> {
        let mut operator_found = false;
        for child in node.children().filter(Node::is_element) {
            if child.tag_name().name() == "length" {
                // Explicit length handle — parse as Length instruction
                let mut length_instr = Self::from_node(child)?;
                if length_instr.name.is_empty() {
                    length_instr.name = format!("{}:length", instr.name);
                }
                length_instr.presence = instr.presence;
                instr.instructions.push(length_instr);
            } else {
                // Operator element
                if !operator_found {
                    instr.operator = Operator::from_tag(child.tag_name().name())?;
                    if let Some(s) = child.attribute("value") {
                        instr.set_initial_value(s)?;
                    }
                    operator_found = true;
                }
            }
        }
        // No explicit length handle — create synthetic Length instruction
        if instr.instructions.is_empty() {
            let mut length = Self::new(0, &format!("{}:length", instr.name), ValueType::Length);
            length.presence = instr.presence;
            instr.instructions.push(length);
        }
        Ok(())
    }

    /// Parse Group children: typeRef + nested instructions.
    fn parse_group_children(instr: &mut Self, node: Node) -> Result<()> {
        for n in node.children().filter(Node::is_element) {
            if n.tag_name().name() == "typeRef" {
                if let Some(name) = n.attribute("name") {
                    instr.type_ref = TypeRef::from_str(name);
                }
                continue;
            }
            let child = Self::from_node(n)?;
            instr.instructions.push(child);
        }
        Ok(())
    }

    /// Parse Sequence children: typeRef, implicit Length, then item instructions.
    fn parse_sequence_children(instr: &mut Self, node: Node) -> Result<()> {
        let type_ref_name = node
            .children()
            .filter(Node::is_element)
            .find(|n| n.tag_name().name() == "typeRef")
            .and_then(|n| n.attribute("name").map(|s| s.to_string()));
        if let Some(name) = type_ref_name {
            instr.type_ref = TypeRef::from_str(&name);
        }

        for (i, c) in node
            .children()
            .filter(Node::is_element)
            .filter(|n| n.tag_name().name() != "typeRef")
            .enumerate()
        {
            let mut child = Self::from_node(c)?;
            if i == 0 {
                if let ValueType::Length = child.value_type {
                    if child.name.is_empty() {
                        child.name = format!("{}:length", instr.name);
                    }
                    child.presence = instr.presence;
                } else {
                    let mut length =
                        Self::new(0, &format!("{}:length", instr.name), ValueType::Length);
                    length.presence = instr.presence;
                    instr.instructions.push(length);
                }
            }
            instr.instructions.push(child);
        }
        Ok(())
    }

    /// Parse Decimal children: exponent, mantissa, operator, initial value.
    fn parse_decimal_children(instr: &mut Self, node: Node) -> Result<()> {
        let mut operator: Option<Operator> = None;
        let mut exponent: Option<Instruction> = None;
        let mut mantissa: Option<Instruction> = None;
        let mut initial_value: Option<String> = None;

        for child in node.children().filter(Node::is_element) {
            match child.tag_name().name() {
                "exponent" => exponent = Some(Self::from_node(child)?),
                "mantissa" => mantissa = Some(Self::from_node(child)?),
                _ => {
                    operator = Some(Operator::from_tag(child.tag_name().name())?);
                    if let Some(v) = child.attribute("value") {
                        initial_value = Some(v.to_string());
                    }
                }
            }
        }

        let (op, mut ex, mut mn) =
            Self::assemble_decimal(operator, exponent, mantissa, initial_value)?;
        ex.presence = instr.presence;
        mn.presence = Presence::Mandatory;
        if ex.key.is_empty() {
            ex.key = Rc::from(format!("{}:exponent", &instr.key));
        }
        if mn.key.is_empty() {
            mn.key = Rc::from(format!("{}:mantissa", &instr.key));
        }
        instr.operator = op;
        instr.instructions.push(ex);
        instr.instructions.push(mn);
        Ok(())
    }

    /// Assemble operator, exponent, mantissa from parsed decimal children.
    fn assemble_decimal(
        operator: Option<Operator>,
        exponent: Option<Instruction>,
        mantissa: Option<Instruction>,
        initial_value: Option<String>,
    ) -> Result<(Operator, Instruction, Instruction)> {
        match (operator, exponent, mantissa) {
            (None, None, None) => {
                let ex = Self::new(0, "exponent", ValueType::Exponent);
                let mn = Self::new(0, "mantissa", ValueType::Mantissa);
                Ok((Operator::None, ex, mn))
            }
            (Some(o), None, None) => {
                let mut ex = Self::new(0, "exponent", ValueType::Exponent);
                let mut mn = Self::new(0, "mantissa", ValueType::Mantissa);
                let op = match o {
                    Operator::Delta | Operator::Increment => {
                        ex.operator = o;
                        mn.operator = o;
                        Operator::None
                    }
                    other => other,
                };
                if let Some(v) = initial_value {
                    let d = Decimal::from_string(&v)?;
                    ex.initial_value = Some(Value::Int32(d.exponent));
                    mn.initial_value = Some(Value::Int64(d.mantissa));
                }
                Ok((op, ex, mn))
            }
            (None, Some(e), Some(m)) => Ok((Operator::None, e, m)),
            _ => Err(Error::Static("invalid decimal elements".to_string())),
        }
    }

    /// Parse default scalar operator + initial value from the first child element.
    fn parse_default_operator(instr: &mut Self, node: Node) -> Result<()> {
        if let Some(operator) = node.children().find(Node::is_element) {
            instr.operator = Operator::from_tag(operator.tag_name().name())?;
            if let Some(s) = operator.attribute("value") {
                instr.set_initial_value(s)?;
            }
        }
        Ok(())
    }

    pub(crate) fn check_is_valid(&self) -> Result<()> {
        match self.operator {
            Operator::None | Operator::Copy | Operator::Delta => {}
            Operator::Constant => {
                if self.initial_value.is_none() {
                    return Err(Error::Static(
                        "constant operator has no initial value".to_string(),
                    ));
                }
            }
            Operator::Default => {
                if !self.is_optional() && self.initial_value.is_none() {
                    return Err(Error::Static(
                        "default operator has no initial value".to_string(),
                    ));
                }
            }
            Operator::Increment => match self.value_type {
                ValueType::UInt32
                | ValueType::Int32
                | ValueType::UInt64
                | ValueType::Int64
                | ValueType::Length
                | ValueType::Exponent
                | ValueType::Mantissa => {}
                _ => {
                    return Err(Error::Static(format!(
                        "increment operator is not applicable to {:?} field type",
                        self.value_type
                    )));
                }
            },
            Operator::Tail => match self.value_type {
                ValueType::AsciiString | ValueType::UnicodeString | ValueType::Bytes => {}
                _ => {
                    return Err(Error::Static(format!(
                        "tail operator is not applicable to {:?} field type",
                        self.value_type
                    )));
                }
            },
        }
        Ok(())
    }

    pub fn is_optional(&self) -> bool {
        self.presence == Presence::Optional
    }

    pub(crate) fn is_nullable(&self) -> bool {
        match self.operator {
            Operator::Constant => false,
            _ => self.nullable || self.is_optional(),
        }
    }

    fn set_initial_value(&mut self, value: &str) -> Result<()> {
        match self.value_type {
            ValueType::UInt32
            | ValueType::Int32
            | ValueType::UInt64
            | ValueType::Int64
            | ValueType::Length
            | ValueType::Exponent
            | ValueType::Mantissa
            | ValueType::AsciiString
            | ValueType::UnicodeString
            | ValueType::Bytes => {
                self.initial_value = Some(self.value_type.parse_initial(value)?);
                Ok(())
            }
            ValueType::Decimal => unreachable!(),
            _ => Err(Error::Static(format!(
                "cannot set initial value to {:?}",
                self.value_type
            ))),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn extract(&self, s: &mut DecoderContext<'_>) -> Result<Option<Value>> {
        match self.operator {
            Operator::None => {
                self.was_present.set(Some(true));
                Ok(self.read(s)?)
            }

            Operator::Constant => {
                let present = !self.is_optional() || s.pmap_next_bit_set();
                self.was_present.set(Some(present));
                let v = if present {
                    match &self.initial_value {
                        Some(v) => Some(v.clone()),
                        None => unreachable!(),
                    }
                } else {
                    None
                };
                Ok(v)
            }

            Operator::Default => {
                let present = s.pmap_next_bit_set();
                self.was_present.set(Some(present));
                if present {
                    match self.read(s) {
                        Ok(v) => Ok(v),
                        Err(Error::UnexpectedEof) => {
                            // Truncated stream — fall back to default for optional fields
                            Ok(self.initial_value.clone())
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    if self.is_nullable() && !self.is_optional() {
                        return Err(Error::Runtime(
                            "default operator has no default value".to_string(),
                        ));
                    }
                    Ok(self.initial_value.clone())
                }
            }

            Operator::Copy => {
                let present = s.pmap_next_bit_set();
                self.was_present.set(Some(present));
                if present {
                    let v = self.read(s)?;
                    s.ctx_set(self, v.clone());
                    return Ok(v);
                }

                let Some(v) = s.ctx_get(self)? else {
                    // Undefined previous: FAST §4.5
                    if self.initial_value.is_some() {
                        let v = self.initial_value.clone().unwrap();
                        s.ctx_set(self, Some(v.clone()));
                        return Ok(Some(v));
                    }
                    if !self.is_optional() && s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D5: mandatory field '{}' absent, undefined previous, no initial value",
                            self.name
                        )));
                    }
                    if self.is_optional() {
                        s.ctx_set(self, None);
                        return Ok(None);
                    }
                    // Loose mode: use type default
                    let v = self.initial_value.clone().unwrap_or_else(|| {
                        self.value_type
                            .default_value()
                            .expect("copy operator: type has no default value")
                    });
                    s.ctx_set(self, Some(v.clone()));
                    return Ok(Some(v));
                };

                if v.is_none() && !self.is_optional() {
                    if s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D6: mandatory field '{}' absent with empty previous value",
                            self.name
                        )));
                    }
                    return Ok(None);
                }
                Ok(v)
            }

            Operator::Increment => {
                let present = s.pmap_next_bit_set();
                self.was_present.set(Some(present));
                if present {
                    let v = self.read(s)?;
                    s.ctx_set(self, v.clone());
                    return Ok(v);
                }
                let Some(v) = s.ctx_get(self)? else {
                    // Undefined previous: FAST §4.5
                    if self.initial_value.is_some() {
                        let iv = self.initial_value.as_ref().unwrap();
                        let v = Some(iv.clone().increment()?);
                        s.ctx_set(self, v.clone());
                        return Ok(v);
                    }
                    if !self.is_optional() && s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D5: mandatory field '{}' absent, undefined previous, no initial value",
                            self.name
                        )));
                    }
                    if self.is_optional() {
                        s.ctx_set(self, None);
                        return Ok(None);
                    }
                    // Loose mode: increment type default
                    let d = self
                        .value_type
                        .default_value()
                        .expect("increment: type has no default");
                    let v = Some(d.increment()?);
                    s.ctx_set(self, v.clone());
                    return Ok(v);
                };

                let Some(prev) = v else {
                    // Empty previous
                    if !self.is_optional() && s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D6: mandatory field '{}' absent with empty previous value",
                            self.name
                        )));
                    }
                    return Ok(None);
                };

                let v = Some(prev.increment()?);
                s.ctx_set(self, v.clone());
                Ok(v)
            }

            Operator::Delta => {
                self.was_present.set(Some(true));
                let Some(delta) = self.read_delta(s)? else {
                    return Ok(None);
                };
                let base = match s.ctx_get(self)? {
                    Some(v) => match v {
                        Some(prev) => prev.clone(),
                        None => {
                            if s.strict {
                                return Err(Error::Dynamic(format!(
                                    "ERR D6: mandatory field '{}' absent with empty previous value (delta)",
                                    self.name
                                )));
                            }
                            // Loose mode: use initial or type default as base
                            match &self.initial_value {
                                Some(v) => v.clone(),
                                None => self.value_type.default_value()?,
                            }
                        }
                    },
                    None => match &self.initial_value {
                        Some(v) => v.clone(),
                        None => self.value_type.default_value()?,
                    },
                };
                let value = Some(base.apply_delta(&delta)?);
                s.ctx_set(self, value.clone());
                Ok(value)
            }

            Operator::Tail => {
                let present = s.pmap_next_bit_set();
                self.was_present.set(Some(present));
                if present {
                    let Some(tail) = self.read_tail(s)? else {
                        // Null tail data: previous state becomes "empty"
                        if self.is_optional() {
                            return Ok(None);
                        }
                        if self.is_nullable() {
                            // Nullable field: NULL sets previous to empty
                            s.ctx_set(self, None);
                            return Ok(None);
                        }
                        if s.strict {
                            return Err(Error::Dynamic(format!(
                                "tail operator has null data for mandatory field '{}'",
                                self.name
                            )));
                        }
                        // Loose mode: use base as-is
                        let base = match s.ctx_get(self)? {
                            Some(v) => match v {
                                Some(prev) => prev.clone(),
                                None => match &self.initial_value {
                                    Some(v) => v.clone(),
                                    None => self.value_type.default_value()?,
                                },
                            },
                            None => match &self.initial_value {
                                Some(v) => v.clone(),
                                None => self.value_type.default_value()?,
                            },
                        };
                        let value = Some(base);
                        s.ctx_set(self, value.clone());
                        return Ok(value);
                    };
                    let base = match s.ctx_get(self)? {
                        Some(v) => match v {
                            Some(prev) => prev.clone(),
                            None => match &self.initial_value {
                                Some(v) => v.clone(),
                                None => self.value_type.default_value()?,
                            },
                        },
                        None => match &self.initial_value {
                            Some(v) => v.clone(),
                            None => self.value_type.default_value()?,
                        },
                    };
                    // For tail: append the tail to the base
                    let value = Some(self.apply_tail(&base, &tail)?);
                    s.ctx_set(self, value.clone());
                    return Ok(value);
                }

                let Some(v) = s.ctx_get(self)? else {
                    // Undefined previous: FAST §4.5
                    if self.initial_value.is_some() {
                        let v = self.initial_value.clone();
                        s.ctx_set(self, v.clone());
                        return Ok(v);
                    }
                    if !self.is_optional() && s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D5: mandatory field '{}' absent, undefined previous, no initial value",
                            self.name
                        )));
                    }
                    if self.is_optional() {
                        s.ctx_set(self, None);
                        return Ok(None);
                    }
                    // Loose mode: use type default
                    let v = Some(
                        self.value_type
                            .default_value()
                            .expect("tail: type has no default"),
                    );
                    s.ctx_set(self, v.clone());
                    return Ok(v);
                };

                if v.is_none() && !self.is_optional() {
                    if s.strict {
                        return Err(Error::Dynamic(format!(
                            "ERR D6: mandatory field '{}' absent with empty previous value",
                            self.name
                        )));
                    }
                    return Ok(None);
                }
                Ok(v)
            }
        }
    }

    fn apply_tail(&self, base: &Value, tail: &Value) -> Result<Value> {
        // FAST §4.8: length of tail = chars/bytes to remove from back of base, then append tail.
        // If tail length >= base length, result = tail value.
        match (self.value_type, base, tail) {
            (ValueType::AsciiString, Value::AsciiString(b), Value::AsciiString(t)) => {
                let tail_len = t.chars().count();
                let result = if tail_len >= b.chars().count() {
                    t.clone()
                } else {
                    let keep = b
                        .chars()
                        .take(b.chars().count() - tail_len)
                        .collect::<String>();
                    format!("{}{}", keep, t)
                };
                Ok(Value::AsciiString(result))
            }
            (ValueType::UnicodeString, Value::UnicodeString(b), Value::Bytes(t)) => {
                let tail_len = t.len();
                let result = if tail_len >= b.as_bytes().len() {
                    String::from_utf8(t.clone())
                        .map_err(|_| Error::Runtime("invalid UTF-8 in tail result".to_string()))?
                } else {
                    let keep = &b.as_bytes()[..b.as_bytes().len() - tail_len];
                    let mut buf = Vec::from(keep);
                    buf.extend_from_slice(t);
                    String::from_utf8(buf)
                        .map_err(|_| Error::Runtime("invalid UTF-8 in tail result".to_string()))?
                };
                Ok(Value::UnicodeString(result))
            }
            (ValueType::Bytes, Value::Bytes(b), Value::Bytes(t)) => {
                let tail_len = t.len();
                let result = if tail_len >= b.len() {
                    t.clone()
                } else {
                    let mut buf = Vec::from(&b[..b.len() - tail_len]);
                    buf.extend_from_slice(t);
                    buf
                };
                Ok(Value::Bytes(result))
            }
            _ => Err(Error::Runtime(format!(
                "cannot apply tail {:?} to {:?}",
                tail, base
            ))),
        }
    }

    fn read(&self, s: &mut DecoderContext<'_>) -> Result<Option<Value>> {
        match self.value_type {
            ValueType::UInt32 | ValueType::Length => match self.read_uint32(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::UInt32(v))),
            },
            ValueType::UInt64 => match self.read_uint64(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::UInt64(v))),
            },
            ValueType::Int32 => match self.read_int32(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::Int32(v))),
            },
            ValueType::Int64 | ValueType::Mantissa => match self.read_int64(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::Int64(v))),
            },
            ValueType::AsciiString => match self.read_ascii_string(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::AsciiString(v))),
            },
            ValueType::UnicodeString => match self.read_unicode_string(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::UnicodeString(v))),
            },
            ValueType::Bytes => match self.read_bytes(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::Bytes(v))),
            },
            ValueType::Decimal => {
                let Some((exponent, mantissa)) = self.read_decimal_components(s)? else {
                    return Ok(None);
                };
                Ok(Some(Value::Decimal(Decimal::new(exponent, mantissa))))
            }
            ValueType::Exponent => match self.read_exponent(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::Int32(v))),
            },
            _ => unreachable!(),
        }
    }

    fn read_uint32(&self, s: &mut DecoderContext<'_>) -> Result<Option<u32>> {
        if self.is_nullable() {
            match s.rdr.read_uint_nullable() {
                Ok(None) => Ok(None),
                Ok(Some(v)) => {
                    if v > u64::from(u32::MAX) {
                        return Err(Error::Runtime(format!("uInt32 value is out of range: {v}")));
                    }
                    Ok(Some(v as u32))
                }
                Err(_e) => Err(Error::UnexpectedEof), // map &'static str to Error
            }
        } else {
            let v = s.rdr.read_uint().map_err(|_| Error::UnexpectedEof)?;
            if v > u64::from(u32::MAX) {
                return Err(Error::Runtime(format!("uInt32 value is out of range: {v}")));
            }
            Ok(Some(v as u32))
        }
    }

    fn read_uint64(&self, s: &mut DecoderContext<'_>) -> Result<Option<u64>> {
        if self.is_nullable() {
            Ok(s.rdr
                .read_uint_nullable()
                .map_err(|_| Error::UnexpectedEof)?)
        } else {
            Ok(Some(s.rdr.read_uint().map_err(|_| Error::UnexpectedEof)?))
        }
    }

    fn read_int32(&self, s: &mut DecoderContext<'_>) -> Result<Option<i32>> {
        const INT32_RANGE: RangeInclusive<i64> = (i32::MIN as i64)..=(i32::MAX as i64);
        if self.is_nullable() {
            match s
                .rdr
                .read_int_nullable()
                .map_err(|_| Error::UnexpectedEof)?
            {
                None => Ok(None),
                Some(v) => {
                    if !INT32_RANGE.contains(&v) {
                        return Err(Error::Runtime(format!("Int32 value is out of range: {v}")));
                    }
                    Ok(Some(v as i32))
                }
            }
        } else {
            let v = s.rdr.read_int().map_err(|_| Error::UnexpectedEof)?;
            if !INT32_RANGE.contains(&v) {
                return Err(Error::Runtime(format!("Int32 value is out of range: {v}")));
            }
            Ok(Some(v as i32))
        }
    }

    fn read_int64(&self, s: &mut DecoderContext<'_>) -> Result<Option<i64>> {
        if self.is_nullable() {
            Ok(s.rdr
                .read_int_nullable()
                .map_err(|_| Error::UnexpectedEof)?)
        } else {
            Ok(Some(s.rdr.read_int().map_err(|_| Error::UnexpectedEof)?))
        }
    }

    fn read_ascii_string(&self, s: &mut DecoderContext<'_>) -> Result<Option<String>> {
        if self.is_nullable() {
            Ok(s.rdr
                .read_ascii_string_nullable()
                .map_err(|_| Error::UnexpectedEof)?)
        } else {
            Ok(Some(
                s.rdr
                    .read_ascii_string()
                    .map_err(|_| Error::UnexpectedEof)?,
            ))
        }
    }

    fn read_unicode_string(&self, s: &mut DecoderContext<'_>) -> Result<Option<String>> {
        if self.is_nullable() {
            Ok(s.rdr
                .read_unicode_string_nullable()
                .map_err(|_| Error::UnexpectedEof)?)
        } else {
            Ok(Some(
                s.rdr
                    .read_unicode_string()
                    .map_err(|_| Error::UnexpectedEof)?,
            ))
        }
    }

    fn read_bytes(&self, s: &mut DecoderContext<'_>) -> Result<Option<Vec<u8>>> {
        if self.is_nullable() {
            Ok(s.rdr
                .read_bytes_nullable()
                .map_err(|_| Error::UnexpectedEof)?)
        } else {
            Ok(Some(s.rdr.read_bytes().map_err(|_| Error::UnexpectedEof)?))
        }
    }

    fn read_delta(&self, s: &mut DecoderContext<'_>) -> Result<Option<Value>> {
        match self.value_type {
            ValueType::UInt32
            | ValueType::Int32
            | ValueType::UInt64
            | ValueType::Int64
            | ValueType::Length
            | ValueType::Exponent
            | ValueType::Mantissa => match self.read_int64(s)? {
                None => Ok(None),
                Some(v) => Ok(Some(Value::Int64(v))),
            },
            ValueType::AsciiString | ValueType::UnicodeString | ValueType::Bytes => {
                // FAST §4.7: unsigned integer encoding with value incremented by one.
                // Read unsigned int, subtract 1 to get subtraction length.
                let sub_entity = s.rdr.read_uint().map_err(|_| Error::UnexpectedEof)?;
                let sub_len = if sub_entity > 0 {
                    sub_entity as usize - 1
                } else {
                    0
                };
                // Get base from context (or default empty)
                let base = match s.ctx_get(self)? {
                    Some(Some(base_val)) => base_val,
                    _ => self.value_type.default_value()?,
                };
                // Read new data and reconstruct: base[sub_len..] + new_data
                match self.value_type {
                    ValueType::AsciiString => {
                        let new_data = self.read_ascii_string(s)?.unwrap();
                        let result = match base {
                            Value::AsciiString(ref b) => {
                                let sub_idx = sub_len.min(b.len());
                                format!("{}{}", &b[sub_idx..], new_data)
                            }
                            _ => new_data,
                        };
                        Ok(Some(Value::AsciiString(result)))
                    }
                    ValueType::UnicodeString => {
                        let new_data = self.read_bytes(s)?.unwrap();
                        let result = match base {
                            Value::UnicodeString(ref b) => {
                                let sub_idx = sub_len.min(b.len());
                                let mut r = b.as_bytes()[sub_idx..].to_vec();
                                r.extend_from_slice(&new_data);
                                String::from_utf8_lossy(&r).into_owned()
                            }
                            _ => String::from_utf8_lossy(&new_data).into_owned(),
                        };
                        Ok(Some(Value::UnicodeString(result)))
                    }
                    ValueType::Bytes => {
                        let new_data = self.read_bytes(s)?.unwrap();
                        let result = match base {
                            Value::Bytes(ref b) => {
                                let sub_idx = sub_len.min(b.len());
                                let mut r = b[sub_idx..].to_vec();
                                r.extend_from_slice(&new_data);
                                r
                            }
                            _ => new_data,
                        };
                        Ok(Some(Value::Bytes(result)))
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }

    fn read_tail(&self, s: &mut DecoderContext<'_>) -> Result<Option<Value>> {
        match self.value_type {
            ValueType::AsciiString => Ok(self.read_ascii_string(s)?.map(Value::AsciiString)),
            ValueType::UnicodeString | ValueType::Bytes => {
                Ok(self.read_bytes(s)?.map(Value::Bytes))
            }
            _ => unreachable!(),
        }
    }

    fn read_decimal_components(&self, s: &mut DecoderContext<'_>) -> Result<Option<(i32, i64)>> {
        // For decimals with sub-instructions that have operators, check the pmap
        // to determine if sub-instruction data is present in the stream.
        let has_operator = self
            .instructions
            .iter()
            .any(|si| si.operator != Operator::None);
        if has_operator && !s.pmap_next_bit_set() {
            return Ok(None);
        }

        let exponent = self
            .instructions
            .first()
            .ok_or_else(|| Error::Runtime("exponent field not found".to_string()))?
            .extract(s)?;
        if exponent.is_none() {
            return Ok(None);
        }
        let mantissa = self
            .instructions
            .get(1)
            .ok_or_else(|| Error::Runtime("mantissa field not found".to_string()))?
            .extract(s)?;

        if let (Some(Value::Int32(e)), Some(Value::Int64(m))) = (exponent, mantissa) {
            Ok(Some((e, m)))
        } else {
            Err(Error::Runtime("exponent or mantissa not found".to_string()))
        }
    }

    fn read_exponent(&self, s: &mut DecoderContext<'_>) -> Result<Option<i32>> {
        let Some(e) = self.read_int32(s)? else {
            return Ok(None);
        };
        if !(MIN_EXPONENT..=MAX_EXPONENT).contains(&e) {
            return Err(Error::Dynamic(format!(
                "exponent value is out of range: {e}"
            )));
        }
        Ok(Some(e))
    }
}
