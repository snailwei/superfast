//! Presence map — variable-length bitmap for FAST field presence.
//!
//! Bits are consumed MSB-first. Each byte carries 7 bits; bit 7 (MSB) indicates
//! continuation (0) or end (1).

/// Tracks consumption of presence map bits.
#[derive(Debug)]
pub struct PresenceMap {
    bitmap: u64,
    mask: u64,
    size: u8,
    /// Actual number of bits set (for encoder).
    num_bits: u8,
}

#[allow(dead_code)]
impl PresenceMap {
    pub fn empty() -> Self {
        Self {
            bitmap: 0,
            mask: 0x40,
            size: 7,
            num_bits: 0,
        }
    }

    pub fn new(bitmap: u64, size: u8) -> Self {
        Self {
            bitmap,
            mask: 1u64 << (size - 1),
            size,
            num_bits: size,
        }
    }

    /// Return the capacity (multiple of 7) of this presence map.
    #[inline]
    pub fn size(&self) -> u8 {
        self.size
    }

    /// Return the actual number of bits set (for encoder).
    #[inline]
    pub fn num_bits(&self) -> u8 {
        self.num_bits
    }

    /// Return the raw bitmap value.
    #[inline]
    pub fn bitmap(&self) -> u64 {
        self.bitmap
    }

    /// Consume the next bit (MSB-first). Returns true if set.
    #[inline]
    pub fn next_bit_set(&mut self) -> bool {
        let res = self.bitmap & self.mask != 0;
        self.mask >>= 1;
        res
    }

    /// Set the next bit. Used by encoder (not decoder).
    #[inline]
    pub fn set_next_bit(&mut self, value: bool) {
        if self.mask == 0 {
            self.bitmap <<= 7;
            self.mask = 0x40;
            self.size += 7;
        }
        if value {
            self.bitmap |= self.mask;
        }
        self.mask >>= 1;
        self.num_bits += 1;
    }
}
