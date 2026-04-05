use crate::constants::extra_keys;
use crate::extra::parse_extra_pairs;
use crate::models::archive::ArchiveIndexEntryWrapper;
use crate::tui::tree::{FlatNode, TreeNode, flatten_visible};

// ---------------------------------------------------------------------------
// Tag alias table
// ---------------------------------------------------------------------------

/// Maps user-friendly tag aliases to their internal extra-field keys.
///
/// Multiple aliases can map to the same internal key (e.g. "camera" and "make"
/// both resolve to "imk").  Order matters: the first entry for each internal
/// key is the *primary* alias shown in help text; secondary aliases are
/// alternatives the user may also type.
pub const TAG_ALIASES: &[(&str, &str)] = &[
    ("artist", extra_keys::AUDIO_ARTIST),
    ("album", extra_keys::AUDIO_ALBUM),
    ("title", extra_keys::AUDIO_TITLE),
    ("genre", extra_keys::AUDIO_GENRE),
    ("make", extra_keys::IMG_MAKE),
    ("model", extra_keys::IMG_MODEL),
    ("date", extra_keys::IMG_DATETIME_ORIGINAL),
];

/// Full tag information: `(alias, internal_key, i18n_description_key)`.
///
/// Used by the help panel to render a table of available tags with their
/// localised descriptions.
pub const TAG_ALIASES_FULL: &[(&str, &str, &str)] = &[
    (
        "artist",
        extra_keys::AUDIO_ARTIST,
        "tui.inspect.preview.tags.aar",
    ),
    (
        "album",
        extra_keys::AUDIO_ALBUM,
        "tui.inspect.preview.tags.aal",
    ),
    (
        "title",
        extra_keys::AUDIO_TITLE,
        "tui.inspect.preview.tags.atl",
    ),
    (
        "genre",
        extra_keys::AUDIO_GENRE,
        "tui.inspect.preview.tags.agn",
    ),
    ("make", extra_keys::IMG_MAKE, "tui.inspect.preview.tags.imk"),
    (
        "model",
        extra_keys::IMG_MODEL,
        "tui.inspect.preview.tags.imd",
    ),
    (
        "date",
        extra_keys::IMG_DATETIME_ORIGINAL,
        "tui.inspect.preview.tags.idt",
    ),
];

/// Resolve a user-facing alias (case-insensitive) to an internal extra-field key.
pub fn resolve_alias(alias: &str) -> Option<&'static str> {
    let lower = alias.to_ascii_lowercase();
    TAG_ALIASES
        .iter()
        .find(|(a, _)| *a == lower.as_str())
        .map(|(_, k)| *k)
}

// ---------------------------------------------------------------------------
// Predicate
// ---------------------------------------------------------------------------

/// A single parsed filter criterion: match entries whose `internal_key` field
/// contains `value` (case-insensitive substring).
#[derive(Debug, Clone, PartialEq)]
pub struct MetadataPredicate {
    /// Internal extra-field key (e.g. `"aar"` for artist).
    pub internal_key: String,
    /// Case-insensitive substring to search for in the field value.
    pub value: String,
}

// ---------------------------------------------------------------------------
// Query parser
// ---------------------------------------------------------------------------

/// Parse a metadata search query string into a list of [`MetadataPredicate`]s.
///
/// ## Syntax
/// ```text
/// tag:value [tag:value ...]
/// ```
/// - `tag` is a user-friendly alias (e.g. `artist`, `camera`).
/// - `value` extends until the next `tag:` token, so multi-word values work
///   naturally: `album:abbey road artist:beatles` → two predicates where the
///   album value is `"abbey road"`.
/// - All predicates are combined with AND logic during filtering.
/// - An empty string returns `Ok(vec![])`.
///
/// ## Errors
/// Returns a human-readable `Err` string when:
/// - A token without `:` is encountered (not a valid `tag:value` pair).
/// - An unknown tag alias is used.
/// - A tag is supplied without a value.
pub fn parse_meta_query(input: &str, locale: &str) -> Result<Vec<MetadataPredicate>, String> {
    if input.trim().is_empty() {
        return Ok(Vec::new());
    }

    let tokens: Vec<&str> = input.split_whitespace().collect();
    let mut predicates: Vec<MetadataPredicate> = Vec::new();
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];

        if let Some(colon_pos) = token.find(':') {
            let alias = &token[..colon_pos].to_ascii_lowercase();
            let value_start = &token[colon_pos + 1..];

            let internal_key = resolve_alias(alias).ok_or_else(|| {
                let known: Vec<&str> = TAG_ALIASES.iter().map(|(a, _)| *a).collect();
                let known_str = known.join(", ");
                rust_i18n::t!(
                    "tui.meta_search.errors.unknown_tag",
                    locale = locale,
                    tag = alias.as_str(),
                    known = known_str.as_str()
                )
                .into_owned()
            })?;

            // Greedily collect following tokens that do not look like `key:…`.
            let mut value_parts: Vec<&str> = if value_start.is_empty() {
                vec![]
            } else {
                vec![value_start]
            };
            i += 1;
            while i < tokens.len() && !tokens[i].contains(':') {
                value_parts.push(tokens[i]);
                i += 1;
            }

            let value = value_parts.join(" ").trim().to_string();
            if value.is_empty() {
                return Err(rust_i18n::t!(
                    "tui.meta_search.errors.missing_value",
                    locale = locale,
                    tag = alias.as_str()
                )
                .into_owned());
            }

            predicates.push(MetadataPredicate {
                internal_key: internal_key.to_string(),
                value,
            });
        } else {
            // A bare word without ':' is not a valid predicate.
            return Err(rust_i18n::t!(
                "tui.meta_search.errors.bare_word",
                locale = locale,
                token = token
            )
            .into_owned());
        }
    }

    Ok(predicates)
}

// ---------------------------------------------------------------------------
// Filter
// ---------------------------------------------------------------------------

/// Filter archive entries by metadata predicates and return a flat list.
///
/// - Empty `predicates` → returns all visible nodes (identical to
///   [`flatten_visible`]).
/// - Non-empty `predicates` → walks every **file** leaf in the tree and keeps
///   those whose extra-field values satisfy **all** predicates (AND logic).
///   Matching nodes are returned at `depth = 0` with their full path as the
///   display name (same convention as fuzzy filename search).
pub fn apply_meta_filter(
    predicates: &[MetadataPredicate],
    entries: &[ArchiveIndexEntryWrapper],
    root: &TreeNode,
) -> Vec<FlatNode> {
    if predicates.is_empty() {
        return flatten_visible(root);
    }

    let mut out = Vec::new();
    collect_meta_filtered(predicates, entries, root, &mut out);
    out
}

/// Recursive helper that walks `node` depth-first and pushes matching leaves.
fn collect_meta_filtered(
    predicates: &[MetadataPredicate],
    entries: &[ArchiveIndexEntryWrapper],
    node: &TreeNode,
    out: &mut Vec<FlatNode>,
) {
    for child in &node.children {
        if child.is_dir {
            collect_meta_filtered(predicates, entries, child, out);
        } else if let Some(idx) = child.entry_idx {
            let extra_pairs = parse_extra_pairs(&entries[idx].extra);
            let matches = predicates.iter().all(|pred| {
                extra_pairs.iter().any(|(k, v)| {
                    k == &pred.internal_key
                        && v.to_ascii_lowercase()
                            .contains(&pred.value.to_ascii_lowercase())
                })
            });
            if matches {
                out.push(FlatNode {
                    depth: 0,
                    is_dir: false,
                    expanded: false,
                    display_name: child.full_path.clone(),
                    full_path: child.full_path.clone(),
                    entry_idx: child.entry_idx,
                    match_indices: Vec::new(),
                });
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extra::encode_extra_pairs;
    use crate::models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper};
    use crate::tui::tree::build_tree;
    use bytemuck::Zeroable;

    // ── Parser tests ────────────────────────────────────────────────────────

    #[test]
    fn empty_query_returns_empty() {
        assert!(parse_meta_query("", "en").unwrap().is_empty());
        assert!(parse_meta_query("   ", "en").unwrap().is_empty());
    }

    #[test]
    fn single_predicate() {
        let preds = parse_meta_query("artist:beatles", "en").unwrap();
        assert_eq!(preds.len(), 1);
        assert_eq!(preds[0].internal_key, "aar");
        assert_eq!(preds[0].value, "beatles");
    }

    #[test]
    fn multi_word_value() {
        let preds = parse_meta_query("album:abbey road", "en").unwrap();
        assert_eq!(preds.len(), 1);
        assert_eq!(preds[0].internal_key, "aal");
        assert_eq!(preds[0].value, "abbey road");
    }

    #[test]
    fn multiple_predicates() {
        let preds = parse_meta_query("artist:beatles album:abbey road", "en").unwrap();
        assert_eq!(preds.len(), 2);
        assert_eq!(preds[0].value, "beatles");
        assert_eq!(preds[1].internal_key, "aal");
        assert_eq!(preds[1].value, "abbey road");
    }

    #[test]
    fn camera_and_make_alias() {
        assert_eq!(
            parse_meta_query("make:nikon", "en").unwrap()[0].internal_key,
            "imk"
        );
    }

    #[test]
    fn model_resolves_to_imd() {
        let preds = parse_meta_query("model:5D", "en").unwrap();
        assert_eq!(preds[0].internal_key, "imd");
    }

    #[test]
    fn unknown_tag_returns_error() {
        let err = parse_meta_query("foo:bar", "en").unwrap_err();
        assert!(err.contains("foo"), "error should mention the bad tag");
    }

    #[test]
    fn missing_value_returns_error() {
        let err = parse_meta_query("artist:", "en").unwrap_err();
        assert!(err.contains("artist"));
    }

    #[test]
    fn bare_word_returns_error() {
        let err = parse_meta_query("beatles", "en").unwrap_err();
        assert!(err.contains("beatles"));
    }

    // ── Filter tests ────────────────────────────────────────────────────────

    fn make_entry_with_extra(path: &str, extra: &[(&str, &str)]) -> ArchiveIndexEntryWrapper {
        let pairs: Vec<(String, String)> = extra
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        ArchiveIndexEntryWrapper::new(
            ArchiveIndexEntry::zeroed(),
            path.to_string(),
            encode_extra_pairs(pairs),
        )
    }

    #[test]
    fn filter_by_artist() {
        let entries = vec![
            make_entry_with_extra("song1.mp3", &[("aar", "The Beatles")]),
            make_entry_with_extra("song2.mp3", &[("aar", "Led Zeppelin")]),
            make_entry_with_extra("photo.jpg", &[("imk", "Canon")]),
        ];
        let root = build_tree(&entries);
        let preds = parse_meta_query("artist:beatles", "en").unwrap();
        let results = apply_meta_filter(&preds, &entries, &root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].full_path, "song1.mp3");
    }

    #[test]
    fn filter_and_logic() {
        let entries = vec![
            make_entry_with_extra("a.mp3", &[("aar", "Beatles"), ("aal", "Abbey Road")]),
            make_entry_with_extra("b.mp3", &[("aar", "Beatles"), ("aal", "Help")]),
            make_entry_with_extra("c.mp3", &[("aar", "Zeppelin"), ("aal", "Abbey Road")]),
        ];
        let root = build_tree(&entries);
        let preds = parse_meta_query("artist:beatles album:abbey", "en").unwrap();
        let results = apply_meta_filter(&preds, &entries, &root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].full_path, "a.mp3");
    }

    #[test]
    fn filter_case_insensitive() {
        let entries = vec![make_entry_with_extra("x.mp3", &[("aar", "The BEATLES")])];
        let root = build_tree(&entries);
        let preds = parse_meta_query("artist:beatles", "en").unwrap();
        let results = apply_meta_filter(&preds, &entries, &root);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn filter_no_match_returns_empty() {
        let entries = vec![make_entry_with_extra("x.mp3", &[("aar", "Zeppelin")])];
        let root = build_tree(&entries);
        let preds = parse_meta_query("artist:beatles", "en").unwrap();
        let results = apply_meta_filter(&preds, &entries, &root);
        assert!(results.is_empty());
    }
}
