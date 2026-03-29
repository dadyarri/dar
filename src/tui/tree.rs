use crate::models::archive::ArchiveIndexEntryWrapper;

/// A node in the directory tree built from archive entries.
#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub full_path: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub entry_idx: Option<usize>,
    pub children: Vec<TreeNode>,
}

/// A flattened, currently-visible row ready for table rendering.
#[derive(Debug, Clone)]
pub struct FlatNode {
    pub depth: usize,
    pub is_dir: bool,
    /// Mirrors `TreeNode::expanded`; always `false` for file nodes.
    pub expanded: bool,
    pub display_name: String,
    pub full_path: String,
    /// Index into the original `entries` slice; `None` for implicit directory nodes.
    pub entry_idx: Option<usize>,
    /// Character positions inside `display_name` that caused this node to match
    /// the fuzzy search query.  Empty when not in search mode.
    pub match_indices: Vec<u32>,
}

impl TreeNode {
    fn new_root() -> Self {
        Self {
            name: String::new(),
            full_path: String::new(),
            is_dir: true,
            expanded: true,
            entry_idx: None,
            children: Vec::new(),
        }
    }

    fn new_dir(name: String, full_path: String) -> Self {
        Self {
            name,
            full_path,
            is_dir: true,
            expanded: false,
            entry_idx: None,
            children: Vec::new(),
        }
    }

    fn new_file(name: String, full_path: String, entry_idx: usize) -> Self {
        Self {
            name,
            full_path,
            is_dir: false,
            expanded: false,
            entry_idx: Some(entry_idx),
            children: Vec::new(),
        }
    }
}

/// Build a directory tree from the flat list of archive entries.
///
/// The returned root node is virtual (never rendered directly); its children
/// represent the top-level entries of the archive.  All directory nodes start
/// in the expanded state.  Children at every level are sorted: directories
/// first, then files, both alphabetically.
pub fn build_tree(entries: &[ArchiveIndexEntryWrapper]) -> TreeNode {
    let mut root = TreeNode::new_root();

    for (idx, wrapper) in entries.iter().enumerate() {
        let parts: Vec<&str> = wrapper.path.split('/').filter(|p| !p.is_empty()).collect();
        if !parts.is_empty() {
            insert_node(&mut root, &parts, idx, &wrapper.path);
        }
    }

    sort_children(&mut root);
    root
}

fn insert_node(parent: &mut TreeNode, parts: &[&str], entry_idx: usize, full_path: &str) {
    if parts.len() == 1 {
        // Leaf: this is the actual file entry.
        parent.children.push(TreeNode::new_file(
            parts[0].to_string(),
            full_path.to_string(),
            entry_idx,
        ));
        return;
    }

    // Intermediate directory segment — find an existing child or create one.
    let dir_name = parts[0];
    let dir_full = if parent.full_path.is_empty() {
        dir_name.to_string()
    } else {
        format!("{}/{}", parent.full_path, dir_name)
    };

    if let Some(child) = parent
        .children
        .iter_mut()
        .find(|c| c.is_dir && c.name == dir_name)
    {
        insert_node(child, &parts[1..], entry_idx, full_path);
    } else {
        let mut new_dir = TreeNode::new_dir(dir_name.to_string(), dir_full);
        insert_node(&mut new_dir, &parts[1..], entry_idx, full_path);
        parent.children.push(new_dir);
    }
}

fn sort_children(node: &mut TreeNode) {
    node.children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    for child in &mut node.children {
        sort_children(child);
    }
}

/// Depth-first walk that emits only nodes whose ancestors are all expanded.
/// The virtual root node is never included in the output.
pub fn flatten_visible(root: &TreeNode) -> Vec<FlatNode> {
    let mut out = Vec::new();
    for child in &root.children {
        flatten_node(child, 0, &mut out);
    }
    out
}

fn flatten_node(node: &TreeNode, depth: usize, out: &mut Vec<FlatNode>) {
    out.push(FlatNode {
        depth,
        is_dir: node.is_dir,
        expanded: node.expanded,
        display_name: node.name.clone(),
        full_path: node.full_path.clone(),
        entry_idx: node.entry_idx,
        match_indices: Vec::new(),
    });
    if node.is_dir && node.expanded {
        for child in &node.children {
            flatten_node(child, depth + 1, out);
        }
    }
}

/// Flip the `expanded` flag of the directory node identified by `full_path`.
/// Returns `true` if the node was found and toggled.
pub fn toggle_expanded(root: &mut TreeNode, full_path: &str) -> bool {
    for child in &mut root.children {
        if do_toggle(child, full_path) {
            return true;
        }
    }
    false
}

fn do_toggle(node: &mut TreeNode, full_path: &str) -> bool {
    if node.full_path == full_path {
        node.expanded = !node.expanded;
        return true;
    }
    for child in &mut node.children {
        if do_toggle(child, full_path) {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::archive::{ArchiveIndexEntry, ArchiveIndexEntryWrapper};
    use bytemuck::Zeroable;

    fn make_entry(path: &str) -> ArchiveIndexEntryWrapper {
        ArchiveIndexEntryWrapper::new(ArchiveIndexEntry::zeroed(), path.to_string(), String::new())
    }

    #[test]
    fn flat_files_at_root() {
        let entries = vec![make_entry("b.txt"), make_entry("a.txt")];
        let root = build_tree(&entries);
        // sorted alphabetically
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].name, "a.txt");
        assert!(!root.children[0].is_dir);
        assert_eq!(root.children[0].entry_idx, Some(1)); // a.txt was index 1
        assert_eq!(root.children[1].name, "b.txt");
        assert_eq!(root.children[1].entry_idx, Some(0));
    }

    #[test]
    fn nested_dirs_created_implicitly() {
        let entries = vec![
            make_entry("src/main.rs"),
            make_entry("src/lib.rs"),
            make_entry("README.md"),
        ];
        let root = build_tree(&entries);
        // dirs before files → src/ then README.md
        assert_eq!(root.children.len(), 2);
        let src = &root.children[0];
        assert!(src.is_dir);
        assert_eq!(src.name, "src");
        assert_eq!(src.children.len(), 2);
        let readme = &root.children[1];
        assert!(!readme.is_dir);
        assert_eq!(readme.name, "README.md");
    }

    #[test]
    fn flatten_collapsed_by_default() {
        // Dirs start collapsed, so only top-level nodes are visible.
        let entries = vec![make_entry("src/main.rs"), make_entry("README.md")];
        let root = build_tree(&entries);
        let visible = flatten_visible(&root);
        // src/ (collapsed, depth 0) + README.md (depth 0) — main.rs is hidden
        assert_eq!(visible.len(), 2);
        assert!(visible[0].is_dir);
        assert!(!visible[0].expanded);
        assert_eq!(visible[0].display_name, "src");
        assert_eq!(visible[0].depth, 0);
        assert_eq!(visible[1].display_name, "README.md");
        assert_eq!(visible[1].depth, 0);
    }

    #[test]
    fn expand_dir_shows_children() {
        // After explicitly expanding src/, its child becomes visible.
        let entries = vec![make_entry("src/main.rs"), make_entry("README.md")];
        let mut root = build_tree(&entries);
        toggle_expanded(&mut root, "src");
        let visible = flatten_visible(&root);
        // src/ (expanded, depth 0) + main.rs (depth 1) + README.md (depth 0)
        assert_eq!(visible.len(), 3);
        assert!(visible[0].is_dir);
        assert!(visible[0].expanded);
        assert_eq!(visible[0].display_name, "src");
        assert_eq!(visible[0].depth, 0);
        assert_eq!(visible[1].display_name, "main.rs");
        assert_eq!(visible[1].depth, 1);
        assert_eq!(visible[2].display_name, "README.md");
        assert_eq!(visible[2].depth, 0);
    }

    #[test]
    fn collapsed_dir_hides_children() {
        // Dirs are collapsed by default, so children are never visible without a toggle.
        let entries = vec![make_entry("src/main.rs"), make_entry("README.md")];
        let root = build_tree(&entries);
        let visible = flatten_visible(&root);
        // src/ (collapsed) + README.md
        assert_eq!(visible.len(), 2);
        assert!(visible[0].is_dir);
        assert!(!visible[0].expanded);
    }

    #[test]
    fn double_toggle_restores_collapsed() {
        // Start collapsed → expand → collapse again.
        let entries = vec![make_entry("src/main.rs")];
        let mut root = build_tree(&entries);
        toggle_expanded(&mut root, "src"); // now expanded
        toggle_expanded(&mut root, "src"); // back to collapsed
        let visible = flatten_visible(&root);
        assert_eq!(visible.len(), 1); // only src/ — main.rs hidden again
        assert!(!visible[0].expanded);
    }

    #[test]
    fn nested_collapse() {
        // Nested dirs all start collapsed; only the outermost dir is visible.
        let entries = vec![make_entry("a/b/c.txt")];
        let root = build_tree(&entries);
        let visible = flatten_visible(&root);
        // Only a/ is visible (collapsed); a/b/ and a/b/c.txt are hidden.
        assert_eq!(visible.len(), 1);
        assert!(visible[0].is_dir);
        assert!(!visible[0].expanded);
    }

    #[test]
    fn nested_expand_all_shows_file() {
        // After expanding every level, the file entry should be reachable.
        let entries = vec![make_entry("a/b/c.txt")];
        let mut root = build_tree(&entries);
        toggle_expanded(&mut root, "a");
        toggle_expanded(&mut root, "a/b");
        let visible = flatten_visible(&root);
        // a/ + a/b/ + a/b/c.txt
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[2].entry_idx, Some(0));
    }
}
