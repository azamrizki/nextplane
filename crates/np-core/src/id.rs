//! IDs are UUIDv7 (time-ordered), stored as 16-byte BLOBs (SPEC §7).

pub use uuid::Uuid;

pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_v7_and_ordered() {
        let a = super::new_id();
        let b = super::new_id();
        assert_eq!(a.get_version_num(), 7);
        assert!(a < b);
    }
}
