//! Decoder context — stores state across messages.

use std::rc::Rc;

use crate::value::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DictionaryType {
    Global,
    Template(u32),
    Type(Rc<str>),
    UserDefined(Rc<str>),
}

type ValueKey = Rc<str>;

/// One entry in the context storage (linear-scan Vec, no hashing).
/// Values are stored owned; get() returns references when possible.
#[derive(Clone, Debug)]
struct Entry {
    dict: DictionaryType,
    key: ValueKey,
    val: Option<Value>,
}

/// Decoder state that stores global state during all messages decoding.
#[derive(Debug, Default)]
pub(crate) struct Context {
    entries: Vec<Entry>,
}

impl Context {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reset(&mut self) {
        self.entries.clear();
    }

    pub(crate) fn set(&mut self, dict: DictionaryType, key: ValueKey, val: Option<Value>) {
        // Fast path: pointer equality (same Instruction → same Rc<str> across messages).
        for entry in &mut self.entries {
            if entry.dict == dict && Rc::ptr_eq(&entry.key, &key) {
                entry.val = val;
                return;
            }
        }
        // Fallback: value equality for cross-template key sharing (Tick.Sym vs Txn.Sym).
        for entry in &mut self.entries {
            if entry.dict == dict && entry.key.as_ref() == key.as_ref() {
                entry.val = val;
                return;
            }
        }
        self.entries.push(Entry { dict, key, val });
    }

    /// Global dictionary fast path: skips dictionary type comparison.
    /// Used when we know the dictionary is Global (most common case).
    #[inline]
    pub(crate) fn set_global(&mut self, key: &ValueKey, val: Option<Value>) {
        for entry in &mut self.entries {
            if Rc::ptr_eq(&entry.key, key) {
                entry.val = val;
                return;
            }
        }
        // Fallback: value equality
        for entry in &mut self.entries {
            if entry.key.as_ref() == key.as_ref() && entry.dict == DictionaryType::Global {
                entry.val = val;
                return;
            }
        }
        self.entries.push(Entry {
            dict: DictionaryType::Global,
            key: key.clone(),
            val,
        });
    }

    /// Global dictionary fast path for references.
    #[inline]
    pub(crate) fn get_global_ref(&self, key: &ValueKey) -> Option<Option<&Value>> {
        for entry in &self.entries {
            if Rc::ptr_eq(&entry.key, key) {
                return Some(entry.val.as_ref());
            }
        }
        for entry in &self.entries {
            if entry.key.as_ref() == key.as_ref() && entry.dict == DictionaryType::Global {
                return Some(entry.val.as_ref());
            }
        }
        None
    }

    /// Clone-based get (used when ownership transfer is needed).
    pub(crate) fn get(&self, dict: DictionaryType, key: &ValueKey) -> Option<Option<Value>> {
        for entry in &self.entries {
            if entry.dict == dict && Rc::ptr_eq(&entry.key, key) {
                return Some(entry.val.clone());
            }
        }
        for entry in &self.entries {
            if entry.dict == dict && entry.key.as_ref() == key.as_ref() {
                return Some(entry.val.clone());
            }
        }
        None
    }
}