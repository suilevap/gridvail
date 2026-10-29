//! The one format for data files: RON, read straight into any type that
//! derives `Deserialize`. A new file is a new type; a new field is a new
//! field. Fields that are optional may be written bare (`weapon: "jab"`
//! rather than `weapon: Some("jab")`).

use ron::extensions::Extensions;
use serde::de::DeserializeOwned;

/// Reads `text`, the file called `what`, as a `T`.
pub fn read<T: DeserializeOwned>(what: &str, text: &str) -> Result<T, String> {
    ron::Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(text)
        .map_err(|error| format!("{what}: {error}"))
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Deserialize, Debug, PartialEq)]
    struct Thing {
        name: String,
        size: Option<i32>,
    }

    #[test]
    fn reads_any_type_with_bare_optional_fields() {
        let thing: Thing = super::read("thing", "(name: \"box\", size: 3)").unwrap();
        assert_eq!(
            thing,
            Thing {
                name: "box".into(),
                size: Some(3)
            }
        );
        let error = super::read::<Thing>("thing", "(size: 3)").unwrap_err();
        assert!(error.starts_with("thing:"), "{error}");
    }
}
