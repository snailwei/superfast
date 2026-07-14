//! Decoder context — stores state across messages.

use std::sync::Arc;

use std::collections::HashMap;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DictionaryType {
    Global,
    Template(u32),
    Type(Arc<str>),
    UserDefined(Arc<str>),
}

type ValueKey = Arc<str>;

/// Decoder state that stores global state during all messages decoding.
#[derive(Debug, PartialEq, Default)]
pub(crate) struct Context {
    values: HashMap<(DictionaryType, ValueKey), Option<Value>>,
}

impl Context {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reset(&mut self) {
        self.values.clear();
    }

    pub(crate) fn set(&mut self, dict: DictionaryType, key: ValueKey, val: Option<Value>) {
        self.values.insert((dict, key), val);
    }

    pub(crate) fn get(&self, dict: DictionaryType, key: &ValueKey) -> Option<Option<Value>> {
        self.values.get(&(dict, key.clone())).cloned()
    }
}
