use crate::models::archive::{ArchiveHeader, ArchiveEndRecord};
use super::tree::TreeNode;
use std::fs::File;
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub enum TuiMode {
    Browse,
    Preview,
    ExtractDialog,
    Help,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum MessageType {
    Error,
    Success,
    Info,
}

pub struct TuiState {
    pub archive_path: String,
    pub file_handle: File,
    #[allow(dead_code)]
    pub header: ArchiveHeader,
    #[allow(dead_code)]
    pub end_record: ArchiveEndRecord,
    pub root_tree: TreeNode,

    pub mode: TuiMode,
    pub selected_index: usize,
    pub preview_scroll: usize,
    #[allow(dead_code)]
    pub scroll_offset: usize,
    pub message: Option<(String, Instant, MessageType)>,

    pub extract_output_dir: String,
    pub extract_selected: Vec<String>,
    pub preview_content: Option<Vec<u8>>,
    pub input_buffer: String,
}

impl TuiState {
    pub fn new(
        archive_path: String,
        file_handle: File,
        header: ArchiveHeader,
        end_record: ArchiveEndRecord,
        root_tree: TreeNode,
    ) -> Self {
        let extract_output_dir = dirs::download_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .to_string_lossy()
            .to_string();

        Self {
            archive_path,
            file_handle,
            header,
            end_record,
            root_tree,
            mode: TuiMode::Browse,
            selected_index: 0,
            preview_scroll: 0,
            scroll_offset: 0,
            message: None,
            extract_output_dir,
            extract_selected: Vec::new(),
            preview_content: None,
            input_buffer: String::new(),
        }
    }

    pub fn show_error(&mut self, message: String) {
        self.message = Some((message, Instant::now(), MessageType::Error));
    }

    pub fn show_success(&mut self, message: String) {
        self.message = Some((message, Instant::now(), MessageType::Success));
    }

    // pub fn show_info(&mut self, message: String) {
    //     self.message = Some((message, Instant::now(), MessageType::Info));
    // }

    pub fn should_clear_message(&self) -> bool {
        if let Some((_, timestamp, _)) = &self.message {
            timestamp.elapsed().as_secs() > 5
        } else {
            false
        }
    }

    pub fn set_preview(&mut self, content: Vec<u8>) {
        self.preview_content = Some(content);
        self.preview_scroll = 0;
    }

    pub fn clear_preview(&mut self) {
        self.preview_content = None;
        self.preview_scroll = 0;
    }

    pub fn get_visible_items(&self) -> Vec<TreeItemRef<'_>> {
        let mut items = Vec::new();
        self.collect_visible(&self.root_tree, &mut items, 0);
        items
    }

    fn collect_visible<'a>(
        &'a self,
        node: &'a TreeNode,
        items: &mut Vec<TreeItemRef<'a>>,
        depth: usize,
    ) {
        for child in &node.children {
            items.push(TreeItemRef {
                node: child,
                depth,
            });

            if child.is_dir && child.is_expanded {
                self.collect_visible(child, items, depth + 1);
            }
        }
    }
}

pub struct TreeItemRef<'a> {
    pub node: &'a TreeNode,
    pub depth: usize,
}
