//! Self-contained FAST v1.1 decoder
//!
//! Provides schema-driven decoding: parse XML templates, build an instruction tree,
//! and decode binary buffers into any `serde::Deserialize` type.
//!
//! Integrates directly with serde's `Deserialize` for
//! zero-allocation message deserialization.

mod context;
pub mod decimal;
mod definitions;
mod errors;
pub(crate) mod instruction;
pub mod model;
mod pmap;
mod reader;
mod stacked;
mod template;
pub(crate) mod types;
mod value;
mod writer;

mod decoder;
mod encoder;

#[cfg(test)]
mod tests;

pub use decoder::FastDecoder;
pub use encoder::FastEncoder;
pub use errors::{Error, Result};
pub use types::Dictionary;
