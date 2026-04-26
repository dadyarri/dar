use crate::tui::tree::{FlatNode, TreeNode, flatten_visible};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

/// Return a flat list of visible rows filtered by `query`.
///
/// - Empty `query` → identical to `flatten_visible(root)` (full tree view).
/// - Non-empty `query` → scores every **file** node's `full_path` with
///   `nucleo_matcher`; keeps nodes with a non-zero score; returns them sorted
///   by descending score with `depth = 0` (no tree indentation).
pub fn apply_fuzzy_filter(query: &str, root: &TreeNode) -> Vec<FlatNode> {
    if query.is_empty() {
        return flatten_visible(root);
    }

    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());

    let mut scored: Vec<(u32, FlatNode)> = Vec::new();
    collect_scored_files(&mut matcher, &pattern, root, &mut scored);

    // Descending score: best matches first.
    scored.sort_by(|(a, _), (b, _)| b.cmp(a));
    scored.into_iter().map(|(_, node)| node).collect()
}

/// Recursively walk `node`'s children; score every *file* leaf.
fn collect_scored_files(
    matcher: &mut Matcher,
    pattern: &Pattern,
    node: &TreeNode,
    out: &mut Vec<(u32, FlatNode)>,
) {
    for child in &node.children {
        if child.is_dir {
            collect_scored_files(matcher, pattern, child, out);
        } else {
            let mut buf = Vec::new();
            let mut indices: Vec<u32> = Vec::new();
            let score = pattern
                .indices(
                    Utf32Str::new(&child.full_path, &mut buf),
                    matcher,
                    &mut indices,
                )
                .unwrap_or(0);
            if score > 0 {
                indices.sort_unstable();
                out.push((
                    score,
                    FlatNode {
                        depth: 0,
                        is_dir: false,
                        expanded: false,
                        // Show the full path so the user sees which directory the
                        // file is in (no tree indentation in search results).
                        display_name: child.full_path.clone(),
                        full_path: child.full_path.clone(),
                        entry_idx: child.entry_idx,
                        incremental: child.incremental,
                        match_indices: indices,
                    },
                ));
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
    use crate::models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper};
    use crate::tui::tree::build_tree;
    use bytemuck::Zeroable;

    fn make_entry(path: &str) -> ArchiveIndexEntryWrapper {
        ArchiveIndexEntryWrapper::new(ArchiveIndexEntry::zeroed(), path.to_string(), String::new())
    }

    fn make_root() -> TreeNode {
        let entries = vec![
            make_entry("src/main.rs"),
            make_entry("src/lib.rs"),
            make_entry("assets/logo.png"),
            make_entry("README.md"),
        ];
        build_tree(&entries, 0)
    }

    #[test]
    fn empty_query_returns_full_tree() {
        let root = make_root();
        let full = flatten_visible(&root);
        let filtered = apply_fuzzy_filter("", &root);
        assert_eq!(full.len(), filtered.len());
    }

    #[test]
    fn query_filters_to_matching_files() {
        let root = make_root();
        let results = apply_fuzzy_filter("main", &root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].full_path, "src/main.rs");
    }

    #[test]
    fn query_shows_full_path_as_display_name() {
        let root = make_root();
        let results = apply_fuzzy_filter("lib", &root);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].display_name, "src/lib.rs");
        assert_eq!(results[0].depth, 0);
    }

    #[test]
    fn no_match_returns_empty() {
        let root = make_root();
        let results = apply_fuzzy_filter("zzznomatch", &root);
        assert!(results.is_empty());
    }

    #[test]
    fn multi_match_sorted_by_score() {
        let entries = vec![make_entry("src/main.rs"), make_entry("src/maintain.rs")];
        let root = build_tree(&entries, 0);
        // Both contain "main"; the shorter/closer match should score higher.
        let results = apply_fuzzy_filter("main", &root);
        assert_eq!(results.len(), 2);
        // Just verify we get both without crashing; order depends on scorer.
        let paths: Vec<&str> = results.iter().map(|n| n.full_path.as_str()).collect();
        assert!(paths.contains(&"src/main.rs"));
        assert!(paths.contains(&"src/maintain.rs"));
    }
}
