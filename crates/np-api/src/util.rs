//! Small helpers shared by routes.

/// `Option<Option<T>>` for PATCH bodies: field missing → `None`, `null` → `Some(None)`,
/// value → `Some(Some(v))`. Use with `#[serde(default, with = "crate::util::double_option")]`.
pub mod double_option {
    use serde::{Deserialize, Deserializer};

    pub fn deserialize<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(d).map(Some)
    }
}
