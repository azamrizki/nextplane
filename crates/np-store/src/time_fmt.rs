//! Timestamps are stored as fixed-width UTC text, `2026-11-20T08:39:28.057676Z`, so that
//! comparing the strings in SQL compares the instants.

use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

const FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:6]Z");

pub fn fmt_ts(t: OffsetDateTime) -> String {
    t.to_offset(UtcOffset::UTC)
        .format(FORMAT)
        .expect("fixed format cannot fail")
}

pub fn now_ts() -> String {
    fmt_ts(OffsetDateTime::now_utc())
}

pub fn parse_ts(s: &str) -> Option<OffsetDateTime> {
    PrimitiveDateTime::parse(s, FORMAT)
        .ok()
        .map(|p| p.assume_utc())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn fixed_width_and_sortable() {
        let a = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        let b = a + Duration::microseconds(1);
        assert_eq!(fmt_ts(a), "2023-11-14T22:13:20.000000Z");
        assert!(fmt_ts(a) < fmt_ts(b));
        assert_eq!(parse_ts(&fmt_ts(b)), Some(b));
    }
}
