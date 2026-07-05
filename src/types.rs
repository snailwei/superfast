//! Operator, Presence, Dictionary, and TypeRef enums.
use std::rc::Rc;

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

/// Dictionary scope for state storage.
#[derive(Debug, PartialEq, Clone)]
pub enum Dictionary {
    Inherit,
    Global,
    Template,
    Type,
    UserDefined(Rc<str>),
}

impl Dictionary {
    pub fn from_str(name: &str) -> Self {
        match name {
            "global" => Self::Global,
            "template" => Self::Template,
            "type" => Self::Type,
            _ => Self::UserDefined(Rc::from(name)),
        }
    }
}

/// Application type reference.
#[derive(Debug, PartialEq, Clone)]
pub enum TypeRef {
    Any,
    ApplicationType(Rc<str>),
}

impl TypeRef {
    pub fn from_str(name: &str) -> Self {
        Self::ApplicationType(Rc::from(name))
    }
}
