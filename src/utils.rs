use std::path::{Component, Path, PathBuf};

pub fn calculate_archive_path(file_path: &Path, dir_root: &Path) -> String {
    // Calculate relative path from directory root
    let relative = file_path.strip_prefix(dir_root).unwrap_or(file_path);

    // Sanitize path: convert to string and normalize
    let path_str = relative.to_string_lossy().to_string();
    sanitize_path(&path_str)
}

fn sanitize_path(path: &str) -> String {
    let mut components = Vec::new();
    let path_obj = PathBuf::from(path);

    for component in path_obj.components() {
        match component {
            Component::ParentDir | Component::RootDir => {
                // Skip parent directory and root directory references
            }
            Component::Normal(name) => {
                if let Some(s) = name.to_str() {
                    components.push(s.to_string());
                }
            }
            Component::CurDir => {
                // Skip current directory references
            }
            Component::Prefix(_) => {
                // Skip prefix components (Windows drive letters, etc.)
            }
        }
    }

    components.join("/")
}
