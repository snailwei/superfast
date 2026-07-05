//! Reader trait for FAST binary decoding.

use crate::pmap::PresenceMap;

/// Minimal reader for FAST-encoded data over a byte slice.
pub struct FastReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> FastReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    #[inline]
    #[allow(dead_code)]
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[inline]
    pub fn buf(&self) -> &'a [u8] {
        self.buf
    }

    #[inline]
    #[allow(dead_code)]
    pub fn advance(&mut self, n: usize) {
        self.pos += n;
    }

    #[allow(dead_code)]
    fn expect(&self, n: usize) {
        if self.pos + n > self.buf.len() {
            panic!(
                "FAST reader: needs {} bytes, have {} remaining (pos={})",
                n,
                self.buf.len() - self.pos,
                self.pos
            );
        }
    }

    #[inline]
    fn check_eof(&self) -> Result<(), &'static str> {
        if self.pos >= self.buf.len() {
            Err("unexpected EOF")
        } else {
            Ok(())
        }
    }

    /// Read presence map. Returns `(bitmap, total_bits)`.
    #[inline(always)]
    pub fn read_presence_map(&mut self) -> Result<PresenceMap, &'static str> {
        self.check_eof()?;
        let mut bitmap: u64 = 0;
        let mut size: u8 = 0;
        let mut byte = self.buf[self.pos];
        self.pos += 1;
        loop {
            bitmap <<= 7;
            bitmap |= u64::from(byte & 0x7F);
            size += 7;
            if byte & 0x80 == 0x80 {
                return Ok(PresenceMap::new(bitmap, size));
            }
            self.check_eof()?;
            byte = self.buf[self.pos];
            self.pos += 1;
        }
    }

    /// Read non-nullable unsigned varint.
    #[inline(always)]
    pub fn read_uint(&mut self) -> Result<u64, &'static str> {
        let mut value: u64 = 0;
        loop {
            self.check_eof()?;
            let byte = self.buf[self.pos];
            self.pos += 1;
            value <<= 7;
            value |= u64::from(byte & 0x7F);
            if byte & 0x80 == 0x80 {
                return Ok(value);
            }
        }
    }

    /// Read nullable unsigned varint. `0` → `None`, else `Some(value - 1)`.
    pub fn read_uint_nullable(&mut self) -> Result<Option<u64>, &'static str> {
        let value = self.read_uint()?;
        if value == 0 {
            Ok(None)
        } else {
            Ok(Some(value - 1))
        }
    }

    /// Read non-nullable signed varint (two's complement entity, stop-bit encoded).
    /// Per FAST §2.1: "Entity value = two's complement. MSB of entity value is sign bit."
    /// Zero = entity value 0x80 (single byte with stop bit, no data).
    #[inline(always)]
    pub fn read_int(&mut self) -> Result<i64, &'static str> {
        let mut entity: u64 = 0;
        let mut total_bits: u32 = 0;
        loop {
            self.check_eof()?;
            let byte = self.buf[self.pos];
            self.pos += 1;
            entity <<= 7;
            entity |= u64::from(byte & 0x7F);
            total_bits += 7;
            if byte & 0x80 != 0 {
                break;
            }
        }
        if entity == 0 {
            return Ok(0);
        }
        // Two's complement decode: MSB of entity (bit total_bits-1) is the sign bit.
        let sign_bit = 1u64 << (total_bits - 1);
        if entity & sign_bit != 0 {
            // Negative: sign-extend by setting all bits above total_bits
            let masked = entity | (!0u64 << total_bits);
            Ok(masked as i64)
        } else {
            Ok(entity as i64)
        }
    }

    /// Read nullable signed varint. Entity value 0 (`0x80`) → `None`.
    pub fn read_int_nullable(&mut self) -> Result<Option<i64>, &'static str> {
        let value = self.read_int()?;
        match value {
            0 => Ok(None),
            v if v < 0 => Ok(Some(v + 1)),
            v => Ok(Some(v - 1)),
        }
    }

    /// Read non-nullable ASCII string (bit 7 of each byte is stop flag).
    pub fn read_ascii_string(&mut self) -> Result<String, &'static str> {
        self.check_eof()?;
        let mut byte = self.buf[self.pos];
        self.pos += 1;
        if byte == 0x80 {
            return Ok(String::new());
        }
        let mut buf = Vec::new();
        loop {
            buf.push(byte & 0x7F);
            if byte & 0x80 == 0x80 {
                break;
            }
            self.check_eof()?;
            byte = self.buf[self.pos];
            self.pos += 1;
        }
        // SAFETY: all bytes are 7-bit ASCII
        Ok(unsafe { String::from_utf8_unchecked(buf) })
    }

    /// Read nullable ASCII string. `0x80` → None, `0x00 0x80` → Some("").
    pub fn read_ascii_string_nullable(&mut self) -> Result<Option<String>, &'static str> {
        self.check_eof()?;
        let mut byte = self.buf[self.pos];
        self.pos += 1;
        if byte == 0x80 {
            return Ok(None);
        } else if byte == 0x00 {
            self.check_eof()?;
            byte = self.buf[self.pos];
            self.pos += 1;
            if byte == 0x80 {
                return Ok(Some(String::new()));
            }
        }
        let mut buf = Vec::new();
        loop {
            buf.push(byte & 0x7F);
            if byte & 0x80 == 0x80 {
                break;
            }
            self.check_eof()?;
            byte = self.buf[self.pos];
            self.pos += 1;
        }
        Ok(Some(unsafe { String::from_utf8_unchecked(buf) }))
    }

    /// Read non-nullable Unicode string (varint length + raw bytes).
    pub fn read_unicode_string(&mut self) -> Result<String, &'static str> {
        let len = self.read_uint()? as usize;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            self.check_eof()?;
            bytes.push(self.buf[self.pos]);
            self.pos += 1;
        }
        String::from_utf8(bytes).map_err(|_| "invalid UTF-8")
    }

    /// Read nullable Unicode string. Per §2.5, unicode strings are byte vectors.
    /// Length uses nullable unsigned-integer encoding: NULL → None, Some(len) → read len bytes.
    pub fn read_unicode_string_nullable(&mut self) -> Result<Option<String>, &'static str> {
        match self.read_uint_nullable()? {
            None => Ok(None),
            Some(len) => {
                let mut bytes = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    self.check_eof()?;
                    bytes.push(self.buf[self.pos]);
                    self.pos += 1;
                }
                String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| "invalid UTF-8")
            }
        }
    }

    /// Read non-nullable bytes (varint length + raw bytes).
    pub fn read_bytes(&mut self) -> Result<Vec<u8>, &'static str> {
        let len = self.read_uint()? as usize;
        let mut buf = Vec::with_capacity(len);
        for _ in 0..len {
            self.check_eof()?;
            buf.push(self.buf[self.pos]);
            self.pos += 1;
        }
        Ok(buf)
    }

    /// Read nullable bytes. Per §2.4, length uses nullable unsigned-integer encoding.
    /// NULL (entity 0, 0x80) → None. Non-null → increment-by-1, then read len bytes.
    pub fn read_bytes_nullable(&mut self) -> Result<Option<Vec<u8>>, &'static str> {
        match self.read_uint_nullable()? {
            None => Ok(None),
            Some(len) => {
                let mut buf = Vec::with_capacity(len as usize);
                for _ in 0..len {
                    self.check_eof()?;
                    buf.push(self.buf[self.pos]);
                    self.pos += 1;
                }
                Ok(Some(buf))
            }
        }
    }
}
