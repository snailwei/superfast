//! Operator, Presence, Dictionary, and TypeRef enums.
use std::sync::Arc;

use crate::errors::{Error, Result};

/// FAST field operator.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Operator {
    None,
    Constant,
    Default,
    Copy,
    Increment,
    Delta,
    Tail,
}

impl Operator {
    pub fn from_tag(t: &str) -> Result<Self> {
        match t {
            "constant" => Ok(Self::Constant),
            "default" => Ok(Self::Default),
            "copy" => Ok(Self::Copy),
            "increment" => Ok(Self::Increment),
            "delta" => Ok(Self::Delta),
            "tail" => Ok(Self::Tail),
            _ => Err(Error::Static(format!("Unknown operator: {t}"))),
        }
    }
}

/// Field presence: mandatory or optional.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Presence {
    Mandatory,
    Optional,
}

impl Presence {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "mandatory" => Ok(Self::Mandatory),
            "optional" => Ok(Self::Optional),
            _ => Err(Error::Static(format!("Unknown presence: {s}"))),
        }
    }
}

/// Dictionary scope for operator state storage (FAST spec §4.1).
///
/// Named dictionaries store the previous values used by stateful operators
/// (copy, increment, delta, tail).  Each operator inherits its `dictionary`
/// attribute from the nearest ancestor element; if no ancestor specifies one,
/// the global dictionary is used.
///
/// | Variant | XML value | Scope |
/// |---|---|---|
/// | `Global` | `global` | Shared across all templates (spec default) |
/// | `Template` | `template` | Isolated per template |
/// | `Type` | `type` | Isolated per application `typeRef` |
/// | `UserDefined` | custom string | Isolated by named dictionary |
///
/// # Examples
///
/// ```ignore
/// use superfast::{Dictionary, FastEncoder};
///
/// // Single-template workload — share state across messages (spec default)
/// let enc = FastEncoder::new(xml, Dictionary::Global).unwrap();
///
/// // Multi-template workload — isolate state per template
/// let enc = FastEncoder::new(xml, Dictionary::Template).unwrap();
/// ```
#[derive(Debug, PartialEq, Clone)]
pub enum Dictionary {
    /// Shared across all templates (spec default).
    Global,
    /// Isolated per template — prevents cross-template state pollution.
    Template,
    /// Isolated per application `typeRef`.
    Type,
    /// Isolated by custom named dictionary (e.g., `dictionary="symDict"`).
    UserDefined(Arc<str>),
}

impl Dictionary {
    pub fn from_str(name: &str) -> Self {
        match name {
            "global" => Self::Global,
            "template" => Self::Template,
            "type" => Self::Type,
            _ => Self::UserDefined(Arc::from(name)),
        }
    }
}

/// Application type reference.
#[derive(Debug, PartialEq, Clone)]
pub enum TypeRef {
    Any,
    ApplicationType(Arc<str>),
}

impl TypeRef {
    pub fn from_str(name: &str) -> Self {
        Self::ApplicationType(Arc::from(name))
    }
}
