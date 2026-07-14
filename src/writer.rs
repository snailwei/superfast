//! Writer for FAST binary encoding.

pub struct FastWriter {
    pub buf: Vec<u8>,
}

/// Default buffer capacity for a new FastWriter (covers most single messages).
const DEFAULT_CAPACITY: usize = 64;

impl FastWriter {
    fn buf(&mut self) -> &mut Vec<u8> {
        &mut self.buf
    }
}

/// Macro that generates shared write methods for any type with `fn buf(&mut self) -> &mut Vec<u8>`.
macro_rules! gen_write_impl {
    ($ty:ty) => {
        #[allow(dead_code)]
        impl $ty {
            #[inline]
            fn write_u8(&mut self, b: u8) {
                self.buf().push(b);
            }

            #[inline]
            fn write_all(&mut self, slice: &[u8]) {
                self.buf().extend_from_slice(slice);
            }

            /// Write a raw byte directly (for replaying original bytes).
            pub fn write_raw_u8(&mut self, b: u8) {
                self.buf().push(b);
            }

            /// Write raw bytes directly (for replaying original bytes).
            pub fn write_raw_bytes(&mut self, slice: &[u8]) {
                self.buf().extend_from_slice(slice);
            }

            /// Write presence map (MSB-first, bit 7 = stop on last byte).
            #[inline]
            pub fn write_presence_map(&mut self, bitmap: u64, size: u8) {
                if size == 0 {
                    return;
                }
                let mut remaining = size;
                let mut total = remaining as u32;
                while remaining > 0 {
                    let take = remaining.min(7);
                    let shift = total - take as u32;
                    let byte = ((bitmap >> shift) & 0x7F) as u8;
                    if remaining <= 7 {
                        self.write_u8(byte | 0x80);
                    } else {
                        self.write_u8(byte);
                    }
                    remaining = remaining.saturating_sub(take);
                    total -= take as u32;
                }
            }

            /// Write non-nullable unsigned varint (MSB-first, bit 7 = stop on last byte).
            #[inline]
            pub fn write_uint(&mut self, value: u64) {
                if value == 0 {
                    self.write_u8(0x80);
                    return;
                }
                let bits = 64 - value.leading_zeros();
                let chunks = (bits + 6) / 7; // ceil(bits / 7)
                // Write all bytes except the last without a branch
                for i in 0..chunks - 1 {
                    let shift = (chunks - 1 - i) * 7;
                    self.write_u8(((value >> shift) & 0x7F) as u8);
                }
                // Last byte always has stop bit set
                self.write_u8(((value & 0x7F) as u8) | 0x80);
            }

            /// Write nullable unsigned varint. `None` → `0`, `Some(v)` → `v + 1`.
            #[inline]
            pub fn write_uint_nullable(&mut self, value: Option<u64>) {
                match value {
                    None => self.write_uint(0),
                    Some(v) => self.write_uint(v + 1),
                }
            }

            /// Write non-nullable signed varint (two's complement entity, stop-bit encoded).
            /// Per FAST §2.1: "Entity value = two's complement. MSB of entity value is sign bit."
            /// Sign-bit extension: if the sign bit falls at a 7-bit boundary, emit extra
            /// 7-bit zeros/ones so the MSB is the sign bit.
            #[inline]
            pub fn write_int(&mut self, value: i64) {
                if value == 0 {
                    self.write_u8(0x80);
                    return;
                }
                // Determine minimum entity bits needed for two's complement representation.
                // For positive: sig_bits = bit length of value (64 - leading_zeros).
                // For negative: sig_bits = bit length of ~value (complement).
                //   This is the number of non-sign-extension bits, excluding the sign.
                //   When ~value = 0 (value = -1), sig_bits = 0; use 1 to represent just the sign bit.
                // Boundary rule: if sig_bits is a multiple of 7, add 1 so the sign
                // bit doesn't fall exactly at a 7-bit chunk boundary.
                let value_bits = if value > 0 {
                    let sig_bits = 64u32 - value.leading_zeros();
                    if sig_bits % 7 == 0 && sig_bits > 0 {
                        sig_bits + 1
                    } else {
                        sig_bits
                    }
                } else {
                    let sig_bits = if value == -1 {
                        1
                    } else {
                        let s = 64u32 - (!(value as u64)).leading_zeros();
                        s.max(1)
                    };
                    if sig_bits % 7 == 0 {
                        sig_bits + 1
                    } else {
                        sig_bits
                    }
                };
                // Pad up to nearest multiple of 7 (one byte = 7 data bits).
                let entity_bits = ((value_bits + 6) / 7) * 7;
                let num_bytes = entity_bits as usize / 7;
                // Two's complement in entity_bits: use u128 to handle sign extension
                // beyond 64 bits (needed when boundary extension pushes entity_bits > 64).
                let shifted = (value as i128 as u128) & ((1u128 << entity_bits) - 1);
                // Encode as stop-bit varint, MSB-first — no branch in the loop
                for i in 0..num_bytes - 1 {
                    let shift = (num_bytes - 1 - i) * 7;
                    self.write_u8(((shifted >> shift) & 0x7F) as u8);
                }
                self.write_u8(((shifted & 0x7F) as u8) | 0x80);
            }

            /// Write nullable signed varint.
            #[inline]
            pub fn write_int_nullable(&mut self, value: Option<i64>) {
                match value {
                    None => self.write_int(0),
                    Some(v) if v >= 0 => self.write_int(v + 1),
                    Some(v) => self.write_int(v - 1),
                }
            }

            /// Write non-nullable ASCII string (bit 7 of each byte is stop flag).
            #[inline]
            pub fn write_ascii_string(&mut self, s: &str) {
                if s.is_empty() {
                    self.write_u8(0x80);
                    return;
                }
                let bytes = s.as_bytes();
                // Write all bytes except the last in one shot
                if bytes.len() > 1 {
                    self.write_all(&bytes[..bytes.len() - 1]);
                }
                // Last byte: set stop bit
                self.write_u8(bytes[bytes.len() - 1] | 0x80);
            }

            /// Write nullable ASCII string.
            #[inline]
            pub fn write_ascii_string_nullable(&mut self, value: Option<String>) {
                match value {
                    None => self.write_u8(0x80),
                    Some(s) if s.is_empty() => {
                        self.write_u8(0x00);
                        self.write_u8(0x80);
                    }
                    Some(s) => {
                        let bytes = s.as_bytes();
                        if bytes.len() > 1 {
                            self.write_all(&bytes[..bytes.len() - 1]);
                        }
                        self.write_u8(bytes[bytes.len() - 1] | 0x80);
                    }
                }
            }

            /// Write non-nullable Unicode string (varint length + raw bytes).
            #[inline]
            pub fn write_unicode_string(&mut self, s: &str) {
                self.write_uint(s.len() as u64);
                self.buf().extend_from_slice(s.as_bytes());
            }

            /// Write nullable Unicode string. Per §2.5, unicode strings are byte vectors.
            /// Uses nullable unsigned-integer length: None → 0x80, Some(s) → len+1.
            #[inline]
            pub fn write_unicode_string_nullable(&mut self, value: Option<String>) {
                match value {
                    None => self.write_uint(0),
                    Some(s) => {
                        self.write_uint(s.len() as u64 + 1);
                        self.buf().extend_from_slice(s.as_bytes());
                    }
                }
            }

            /// Write non-nullable bytes (varint length + raw bytes).
            #[inline]
            pub fn write_bytes(&mut self, b: &[u8]) {
                self.write_uint(b.len() as u64);
                self.buf().extend_from_slice(b);
            }

            /// Write nullable bytes. Per §2.4, length uses nullable unsigned-integer encoding.
            /// None → 0x80, Some(b) → len+1.
            #[inline]
            pub fn write_bytes_nullable(&mut self, value: Option<&[u8]>) {
                match value {
                    None => self.write_uint(0),
                    Some(b) => {
                        self.write_uint(b.len() as u64 + 1);
                        self.buf().extend_from_slice(b);
                    }
                }
            }
        }
    };
}

gen_write_impl!(FastWriter);

#[allow(dead_code)]
impl FastWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(DEFAULT_CAPACITY),
        }
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    /// Take the buffer contents, leaving an empty buffer.
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buf)
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Create a FastWriter that writes to an existing buffer.
    pub(crate) fn from_buf(buf: &mut Vec<u8>) -> FastWriterOwned<'_> {
        FastWriterOwned { buf }
    }
}

/// A thin wrapper around a mutable buffer reference, implementing FastWriter's write methods.
pub(crate) struct FastWriterOwned<'a> {
    pub(crate) buf: &'a mut Vec<u8>,
}

impl<'a> FastWriterOwned<'a> {
    #[inline]
    fn buf(&mut self) -> &mut Vec<u8> {
        self.buf
    }
}

gen_write_impl!(FastWriterOwned<'_>);
