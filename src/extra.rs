pub const EXTRA_SEMICOLON_ESCAPE: &str = "%3B";

fn sanitize_extra_component(input: String) -> String {
    input.replace(';', EXTRA_SEMICOLON_ESCAPE)
}

pub fn upsert_extra_pair(
    pairs: &mut Vec<(String, String)>,
    key: impl Into<String>,
    value: impl Into<String>,
) {
    let key = sanitize_extra_component(key.into());
    let value = sanitize_extra_component(value.into());

    // Keep archive creation resilient: skip empty pairs, but preserve semicolons via escaping.
    if key.is_empty() || value.is_empty() {
        return;
    }

    pairs.retain(|(existing_key, _)| existing_key != &key);
    pairs.push((key, value));
}

pub fn encode_extra_pairs(pairs: Vec<(String, String)>) -> String {
    pairs
        .into_iter()
        .map(|(key, value)| format!("{}={}", key, value))
        .collect::<Vec<String>>()
        .join(";")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_extra_pair_skips_empty_values() {
        let mut pairs = vec![];
        upsert_extra_pair(&mut pairs, "artist", "");
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_upsert_extra_pair_replaces_older_value() {
        let mut pairs = vec![];
        upsert_extra_pair(&mut pairs, "artist", "older");
        upsert_extra_pair(&mut pairs, "artist", "newer");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("artist".to_string(), "newer".to_string()));
    }

    #[test]
    fn test_upsert_extra_pair_escapes_semicolon_entries() {
        let mut pairs = vec![];
        upsert_extra_pair(&mut pairs, "ar;tist", "va;lue");
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], ("ar%3Btist".to_string(), "va%3Blue".to_string()));
    }

    #[test]
    fn test_encode_extra_pairs_joins_with_semicolon() {
        let encoded = encode_extra_pairs(vec![
            ("k1".to_string(), "v1".to_string()),
            ("k2".to_string(), "v2".to_string()),
        ]);

        assert_eq!(encoded, "k1=v1;k2=v2");
    }
}

