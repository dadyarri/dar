use std::collections::HashSet;

/// How to handle an archive-relative path that already exists in the archive
/// when `dari append` is called.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ConflictMode {
    /// Abort the entire operation with an error (default).
    #[default]
    Error,
    /// Automatically suffix the conflicting path with `-1`, `-2`, … until a free name is found.
    Rename,
    /// Replace the existing entry; the old data block becomes dead bytes.
    Overwrite,
}

/// Compute the renamed path by appending `-N` before the file extension until a
/// name that is not in `path_set` is found.
///
/// Dotfiles (e.g. `.hidden`) are treated as having no extension — the leading
/// dot is part of the stem, not a separator.
///
/// # Examples
///
/// ```
/// use dari::conflict::make_renamed_path;
/// use std::collections::HashSet;
///
/// let set: HashSet<String> = ["a.txt".to_string()].into_iter().collect();
/// assert_eq!(make_renamed_path("a.txt", &set), "a-1.txt");
/// ```
#[must_use]
pub fn make_renamed_path(path: &str, path_set: &HashSet<String>) -> String {
    let slash_pos = path.rfind('/').map_or(0, |p| p + 1);
    let filename = &path[slash_pos..];
    let (dir_prefix, stem, ext) = if let Some(dot_pos) = filename.rfind('.') {
        if dot_pos > 0 {
            (
                &path[..slash_pos],
                &filename[..dot_pos],
                &filename[dot_pos..],
            )
        } else {
            // Leading dot (dotfile like ".hidden") — treat entire name as stem
            (&path[..slash_pos], filename, "")
        }
    } else {
        (&path[..slash_pos], filename, "")
    };
    let mut n = 1u32;
    loop {
        let candidate = format!("{dir_prefix}{stem}-{n}{ext}");
        if !path_set.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rename_adds_suffix_before_extension() {
        let set: HashSet<String> = ["a.txt".to_string()].into();
        assert_eq!(make_renamed_path("a.txt", &set), "a-1.txt");
    }

    #[test]
    fn test_rename_increments_when_first_candidate_taken() {
        let set: HashSet<String> = ["a.txt".to_string(), "a-1.txt".to_string()].into();
        assert_eq!(make_renamed_path("a.txt", &set), "a-2.txt");
    }

    #[test]
    fn test_rename_no_extension() {
        let set: HashSet<String> = ["README".to_string()].into();
        assert_eq!(make_renamed_path("README", &set), "README-1");
    }

    #[test]
    fn test_rename_preserves_directory_prefix() {
        let set: HashSet<String> = ["src/main.rs".to_string()].into();
        assert_eq!(make_renamed_path("src/main.rs", &set), "src/main-1.rs");
    }

    #[test]
    fn test_rename_dotfile_uses_whole_name_as_stem() {
        let set: HashSet<String> = [".hidden".to_string()].into();
        assert_eq!(make_renamed_path(".hidden", &set), ".hidden-1");
    }
}
