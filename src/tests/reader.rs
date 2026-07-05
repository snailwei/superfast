#[cfg(test)]
mod tests {
    use crate::reader::FastReader;

    #[test]
    fn test_read_presence_map() {
        let mut r = FastReader::new(&[0x80]);
        let pmap = r.read_presence_map().unwrap();
        assert_eq!(pmap.size(), 7);

        let mut r = FastReader::new(&[0x0F, 0x8F]);
        let pmap = r.read_presence_map().unwrap();
        assert_eq!(pmap.size(), 14);
    }

    #[test]
    fn test_read_uint() {
        let mut r = FastReader::new(&[0x80]);
        assert_eq!(r.read_uint().unwrap(), 0);

        let mut r = FastReader::new(&[0x81]);
        assert_eq!(r.read_uint().unwrap(), 1);

        let mut r = FastReader::new(&[0x39, 0x45, 0xA3]);
        assert_eq!(r.read_uint().unwrap(), 942755);
    }

    #[test]
    fn test_read_int() {
        // Two's complement: per FAST §2.1, entity value = two's complement.
        // MSB of entity value is sign bit. Sign-bit extension when sign falls at 7-bit boundary.

        // +942755: spec example, entity=21 bits (no extension)
        let mut r = FastReader::new(&[0x39, 0x45, 0xA3]);
        assert_eq!(r.read_int().unwrap(), 942755);

        // -942755: two's complement in 21 bits (sig_bits=20, no boundary extension)
        // entity = 2^21 - 942755 = 1154397 = 0x11B6DD
        // Chunks: 0x46 0x3A 0x5D → wire: 0x46 0x3A 0xDD
        let mut r = FastReader::new(&[0x46, 0x3A, 0xDD]);
        assert_eq!(r.read_int().unwrap(), -942755);

        // -7942755: two's complement in 28 bits (value_bits=24, entity_bits=28)
        // Encoded as: 0x7C 0x1B 0x1B 0x9D
        let mut r = FastReader::new(&[0x7C, 0x1B, 0x1B, 0x9D]);
        assert_eq!(r.read_int().unwrap(), -7942755);

        // Zero → 0x80
        let mut r = FastReader::new(&[0x80]);
        assert_eq!(r.read_int().unwrap(), 0);

        // +64: spec example, entity=14 bits (sign extension: 7%7==0), 0x00 0xC0
        let mut r = FastReader::new(&[0x00, 0xC0]);
        assert_eq!(r.read_int().unwrap(), 64);

        // -64: two's complement in 7 bits = 1000000 → 0xC0
        let mut r = FastReader::new(&[0xC0]);
        assert_eq!(r.read_int().unwrap(), -64);

        // +1, +2, +5: fit in 1 byte
        let mut r = FastReader::new(&[0x81]);
        assert_eq!(r.read_int().unwrap(), 1);
        let mut r = FastReader::new(&[0x82]);
        assert_eq!(r.read_int().unwrap(), 2);
        let mut r = FastReader::new(&[0x85]);
        assert_eq!(r.read_int().unwrap(), 5);

        // -1, -2, -5: two's complement in 7 bits
        // -1 = 1111111 → 0xFF, -2 = 1111110 → 0xFE, -5 = 1111011 → 0xFB
        let mut r = FastReader::new(&[0xFF]);
        assert_eq!(r.read_int().unwrap(), -1);
        let mut r = FastReader::new(&[0xFE]);
        assert_eq!(r.read_int().unwrap(), -2);
        let mut r = FastReader::new(&[0xFB]);
        assert_eq!(r.read_int().unwrap(), -5);

        // Boundary: ±127 (7-bit magnitude)
        // +127: two's complement in 14 bits (boundary: sig_bits=7, 7%7==0 → extension)
        // entity = 0000000 01111111 → wire: 0x00 0xFF
        let mut r = FastReader::new(&[0x00, 0xFF]);
        assert_eq!(r.read_int().unwrap(), 127);
        // -127: two's complement in 14 bits (boundary: sig_bits=7, extension to 8)
        // entity = 2^14 - 127 = 16257 = 1111111 0000001 → wire: 0x7F 0x81
        let mut r = FastReader::new(&[0x7F, 0x81]);
        assert_eq!(r.read_int().unwrap(), -127);

        // Boundary: ±128 (8 bits for magnitude, 9 with sign)
        // +128: two's complement in 14 bits (value_bits=9, entity_bits=14)
        // entity = 0000000 10000000 → wire: 0x01 0x80
        let mut r = FastReader::new(&[0x01, 0x80]);
        assert_eq!(r.read_int().unwrap(), 128);
        // -128: two's complement in 14 bits (boundary: sig_bits=7, extension to 8 bits)
        // entity = 1111111 00000000 → wire: 0x7F 0x80
        let mut r = FastReader::new(&[0x7F, 0x80]);
        assert_eq!(r.read_int().unwrap(), -128);
    }

    #[test]
    fn test_read_ascii_string() {
        let mut r = FastReader::new(&[0x41, 0x42, 0xC3]);
        assert_eq!(r.read_ascii_string().unwrap(), "ABC");

        let mut r = FastReader::new(&[0x80]);
        assert_eq!(r.read_ascii_string().unwrap(), "");
    }
}
