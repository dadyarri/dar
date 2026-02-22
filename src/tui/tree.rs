use crate::models::archive::ArchiveIndexEntry;

#[derive(Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<TreeNode>,
    pub entry: Option<ArchiveIndexEntry>,
    pub is_expanded: bool,
    pub selected: bool,
}

impl TreeNode {
    pub fn new_dir(name: String, path: String) -> Self {
        Self {
            name,
            path,
            is_dir: true,
            children: Vec::new(),
            entry: None,
            is_expanded: false,
            selected: false,
        }
    }

    pub fn new_file(name: String, path: String, entry: Option<ArchiveIndexEntry>) -> Self {
        Self {
            name,
            path,
            is_dir: false,
            children: Vec::new(),
            entry,
            is_expanded: false,
            selected: false,
        }
    }

    pub fn add_child(&mut self, child: TreeNode) {
        self.children.push(child);
    }

    pub fn get_child_mut(&mut self, name: &str) -> Option<&mut TreeNode> {
        self.children.iter_mut().find(|c| c.name == name)
    }

    pub fn has_child(&self, name: &str) -> bool {
        self.children.iter().any(|c| c.name == name)
    }

    pub fn file_count(&self) -> usize {
        let mut count = if !self.is_dir && self.entry.is_some() {
            1
        } else {
            0
        };

        for child in &self.children {
            count += child.file_count();
        }

        count
    }

    pub fn total_uncompressed_size(&self) -> u64 {
        let mut total = if let Some(entry) = &self.entry {
            entry.uncompressed_size
        } else {
            0
        };

        for child in &self.children {
            total += child.total_uncompressed_size();
        }

        total
    }

    #[allow(dead_code)]
    pub fn total_compressed_size(&self) -> u64 {
        let mut total = if let Some(entry) = &self.entry {
            entry.compressed_size
        } else {
            0
        };

        for child in &self.children {
            total += child.total_compressed_size();
        }

        total
    }

    pub fn collect_selected(&self) -> Vec<String> {
        let mut selected = Vec::new();

        if self.selected && !self.is_dir && self.entry.is_some() {
            selected.push(self.path.clone());
        }

        if self.is_dir && self.selected {
            for child in &self.children {
                selected.extend(child.collect_all_files());
            }
        } else {
            for child in &self.children {
                selected.extend(child.collect_selected());
            }
        }

        selected
    }

    pub fn collect_all_files(&self) -> Vec<String> {
        let mut files = Vec::new();

        if !self.is_dir && self.entry.is_some() {
            files.push(self.path.clone());
        }

        for child in &self.children {
            files.extend(child.collect_all_files());
        }

        files
    }

    pub fn toggle_expanded(&mut self) {
        if self.is_dir && !self.children.is_empty() {
            self.is_expanded = !self.is_expanded;
        }
    }

    pub fn toggle_selected(&mut self) {
        self.selected = !self.selected;
    }

    #[allow(dead_code)]
    pub fn expand_all(&mut self) {
        if self.is_dir {
            self.is_expanded = true;
        }
        for child in &mut self.children {
            child.expand_all();
        }
    }

    #[allow(dead_code)]
    pub fn collapse_all(&mut self) {
        if self.is_dir {
            self.is_expanded = false;
        }
        for child in &mut self.children {
            child.collapse_all();
        }
    }
}

pub struct ArchiveTreeBuilder;

impl ArchiveTreeBuilder {
    pub fn from_entries(entries: Vec<ArchiveIndexEntry>) -> TreeNode {
        let mut root = TreeNode::new_dir(String::new(), String::new());

        for entry in entries {
            Self::insert_entry(&mut root, entry);
        }

        Self::sort_tree(&mut root);
        
        // Expand root by default so users can see contents immediately
        root.is_expanded = true;

        root
    }

    fn insert_entry(root: &mut TreeNode, entry: ArchiveIndexEntry) {
        let parts: Vec<&str> = entry.path.split('/').collect();
        let mut current = root;

        for (i, part) in parts.iter().enumerate() {
            if i == parts.len() - 1 {
                current.add_child(TreeNode::new_file(
                    part.to_string(),
                    entry.path.clone(),
                    Some(entry.clone()),
                ));
            } else {
                if !current.has_child(part) {
                    let path = parts[..=i].join("/");
                    current.add_child(TreeNode::new_dir(
                        part.to_string(),
                        path,
                    ));
                }
                current = current.get_child_mut(part).expect("Child just created");
            }
        }
    }

    fn sort_tree(node: &mut TreeNode) {
        node.children.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        for child in &mut node.children {
            Self::sort_tree(child);
        }
    }
}
