#[cfg(test)]
mod tests {
    use crate::reader::FastReader;
    use crate::writer::FastWriter;

    #[test]
    fn test_uint_roundtrip() {
        for val in [
            0u64,
            1,
            127,
            128,
            255,
            256,
            942755,
            u32::MAX as u64,
            u64::MAX,
        ] {
            let mut w = FastWriter::new();
            w.write_uint(val);
            let mut r = FastReader::new(&w.buf);
            assert_eq!(r.read_uint().unwrap(), val, "uint mismatch at {}", val);
        }
    }

    #[test]
    fn test_int_roundtrip() {
        for val in [
            0i64,
            1,
            -1,
            127,
            -128,
            942755,
            -7942755,
            i32::MAX as i64,
            i32::MIN as i64,
        ] {
            let mut w = FastWriter::new();
            w.write_int(val);
            let mut r = FastReader::new(&w.buf);
            assert_eq!(r.read_int().unwrap(), val, "int mismatch at {}", val);
        }
    }

    #[test]
    fn test_nullable_uint_roundtrip() {
        for val in [None, Some(0u64), Some(1), Some(942755)] {
            let mut w = FastWriter::new();
            w.write_uint_nullable(val);
            let mut r = FastReader::new(&w.buf);
            assert_eq!(
                r.read_uint_nullable().unwrap(),
                val,
                "uint_nullable mismatch at {:?}",
                val
            );
        }
    }

    #[test]
    fn test_nullable_int_roundtrip() {
        for val in [
            None,
            Some(0i64),
            Some(1),
            Some(-1),
            Some(942755),
            Some(-7942755),
        ] {
            let mut w = FastWriter::new();
            w.write_int_nullable(val);
            let mut r = FastReader::new(&w.buf);
            assert_eq!(
                r.read_int_nullable().unwrap(),
                val,
                "int_nullable mismatch at {:?}",
                val
            );
        }
    }

    #[test]
    fn test_ascii_string_roundtrip() {
        for s in ["", "A", "ABC", "hello world"] {
            let mut w = FastWriter::new();
            w.write_ascii_string(s);
            let mut r = FastReader::new(&w.buf);
            assert_eq!(
                r.read_ascii_string().unwrap(),
                s,
                "ascii mismatch at {:?}",
                s
            );
        }
    }

    #[test]
    fn test_nullable_ascii_roundtrip() {
        for val in [None, Some(String::new()), Some("ABC".to_string())] {
            let mut w = FastWriter::new();
            w.write_ascii_string_nullable(val.clone());
            let mut r = FastReader::new(&w.buf);
            assert_eq!(
                r.read_ascii_string_nullable().unwrap(),
                val,
                "ascii_nullable mismatch at {:?}",
                val
            );
        }
    }

    #[test]
    fn test_exact_bytes() {
        let mut w = FastWriter::new();
        w.write_uint(0);
        assert_eq!(w.buf, vec![0x80]);

        let mut w = FastWriter::new();
        w.write_uint(1);
        assert_eq!(w.buf, vec![0x81]);

        let mut w = FastWriter::new();
        w.write_uint(942755);
        assert_eq!(w.buf, vec![0x39, 0x45, 0xA3]);

        let mut w = FastWriter::new();
        w.write_int(-7942755);
        // Two's complement: entity=28 bits, value=0x7C1B1B9D >> wire: 0x7C 0x1B 0x1B 0x9D
        assert_eq!(w.buf, vec![0x7C, 0x1B, 0x1B, 0x9D]);
    }

    #[test]
    fn test_presence_map_roundtrip() {
        // size=7, bitmap=0x40 (64) — fits in 7 bits
        let mut w = FastWriter::new();
        w.write_presence_map(0x40, 7);
        let mut r = FastReader::new(&w.buf);
        let p = r.read_presence_map().unwrap();
        assert_eq!(p.bitmap(), 0x40);
        assert_eq!(p.size(), 7);

        // size=14, bitmap=0x80 (128) — needs 2 chunks
        let mut w = FastWriter::new();
        w.write_presence_map(0x80, 14);
        let mut r = FastReader::new(&w.buf);
        let p = r.read_presence_map().unwrap();
        assert_eq!(p.bitmap(), 0x80);
        assert_eq!(p.size(), 14);

        // size=14, bitmap=0x7FF (2047) — max for 14 bits
        let mut w = FastWriter::new();
        w.write_presence_map(0x7FF, 14);
        let mut r = FastReader::new(&w.buf);
        let p = r.read_presence_map().unwrap();
        assert_eq!(p.bitmap(), 0x7FF);
        assert_eq!(p.size(), 14);
    }
}
