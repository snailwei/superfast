#[cfg(test)]
mod tests {
    use crate::pmap::PresenceMap;

    #[test]
    fn test_next_bit_set() {
        let mut pmap = PresenceMap::new(0b1010110, 7);
        assert!(pmap.next_bit_set());
        assert!(!pmap.next_bit_set());
        assert!(pmap.next_bit_set());
        assert!(!pmap.next_bit_set());
        assert!(pmap.next_bit_set());
        assert!(pmap.next_bit_set());
        assert!(!pmap.next_bit_set());
        // Exhausted bits return false
        assert!(!pmap.next_bit_set());
    }
}
