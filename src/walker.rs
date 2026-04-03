use crate::i18n::Locale;
use crate::utils::calculate_archive_path;
use clap::parser::ValuesRef;
use eyre::{Context, Result};
use ignore::WalkBuilder;
use rust_i18n::t;
use std::fs::canonicalize;
use std::path::{Path, PathBuf};

pub struct ScannedFile {
    pub source_path: PathBuf,
    pub archive_path: String,
}

pub fn scan_files(paths: ValuesRef<String>, locale: &Locale) -> Result<Vec<ScannedFile>> {
    let mut files = vec![];

    for item in paths {
        let relative_path = Path::new(item);
        let absolute_path = canonicalize(relative_path).wrap_err_with(|| {
            t!(
                "cli.common.errors.canonicalize_failed",
                locale = locale.as_str(),
                path = relative_path.display()
            )
        })?;

        if absolute_path.is_dir() {
            let walker = WalkBuilder::new(&absolute_path)
                .git_ignore(true)
                .add_custom_ignore_filename(".darignore")
                .hidden(false)
                .build();

            for entry in walker {
                let entry = entry.wrap_err_with(|| {
                    t!(
                        "cli.common.errors.walk_failed",
                        locale = locale.as_str(),
                        path = absolute_path.display()
                    )
                })?;

                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    files.push(ScannedFile {
                        source_path: entry.path().to_path_buf(),
                        archive_path: calculate_archive_path(&absolute_path, entry.path()),
                    });
                }
            }
        } else if absolute_path.is_file() {
            let archive_path = if let Some(name) = absolute_path.file_name() {
                name.to_string_lossy().into_owned()
            } else {
                absolute_path.to_string_lossy().into_owned()
            };
            files.push(ScannedFile {
                source_path: absolute_path,
                archive_path,
            });
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::scan_files;
    use crate::cli::build_cli_with_translator;
    use crate::i18n::Locale;
    use std::fs;

    fn scan_with_args(paths: &[&str]) -> Vec<super::ScannedFile> {
        // Build a real ArgMatches via the CLI so we get a proper ValuesRef<String>.
        let mut args = vec!["dari", "create", "-f", "out.dar"];
        args.extend_from_slice(paths);
        let matches =
            build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
                .try_get_matches_from(&args)
                .unwrap();
        let sub = matches.subcommand_matches("create").unwrap();
        let content = sub.get_many::<String>("content").unwrap();
        let locale = Locale::new("en");
        scan_files(content, &locale).unwrap()
    }

    #[test]
    fn test_directory_is_walked() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        fs::write(dir.path().join("b.txt"), b"world").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        assert_eq!(files.len(), 2);
        let mut names: Vec<_> = files.iter().map(|f| f.archive_path.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn test_single_file_path_is_added() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("only.txt");
        fs::write(&file_path, b"data").unwrap();

        let path_str = file_path.to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        assert_eq!(files.len(), 1, "scan_files should add a bare file path");
        assert_eq!(files[0].archive_path, "only.txt");
    }

    #[test]
    fn test_nested_directory_is_walked_recursively() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.rs"), b"fn main() {}").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].archive_path, "sub/nested.rs");
    }

    // --- 5.7 .gitignore / .darignore respecting ---

    #[test]
    fn test_gitignore_excludes_matched_files() {
        let dir = tempfile::tempdir().unwrap();
        // Create a minimal .git directory so the ignore crate recognises this as a git repo
        // and honours the .gitignore file (git_ignore only applies inside a git repository).
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".gitignore"), b"secret.txt\n").unwrap();
        fs::write(dir.path().join("public.txt"), b"ok").unwrap();
        fs::write(dir.path().join("secret.txt"), b"hidden").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        let names: Vec<_> = files.iter().map(|f| f.archive_path.as_str()).collect();
        assert!(
            names.contains(&"public.txt"),
            "public.txt should be included"
        );
        assert!(
            !names.contains(&"secret.txt"),
            "secret.txt should be excluded by .gitignore"
        );
    }

    #[test]
    fn test_darignore_excludes_matched_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".darignore"), b"private.log\n").unwrap();
        fs::write(dir.path().join("app.rs"), b"fn main(){}").unwrap();
        fs::write(dir.path().join("private.log"), b"logs").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        let names: Vec<_> = files.iter().map(|f| f.archive_path.as_str()).collect();
        assert!(names.contains(&"app.rs"), "app.rs should be included");
        assert!(
            !names.contains(&"private.log"),
            "private.log should be excluded by .darignore"
        );
    }

    #[test]
    fn test_file_matched_by_both_gitignore_and_darignore_is_excluded() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".git")).unwrap();
        fs::write(dir.path().join(".gitignore"), b"both.tmp\n").unwrap();
        fs::write(dir.path().join(".darignore"), b"both.tmp\n").unwrap();
        fs::write(dir.path().join("both.tmp"), b"x").unwrap();
        fs::write(dir.path().join("keep.txt"), b"y").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        let names: Vec<_> = files.iter().map(|f| f.archive_path.as_str()).collect();
        assert!(names.contains(&"keep.txt"), "keep.txt must be included");
        assert!(
            !names.contains(&"both.tmp"),
            "both.tmp excluded by both ignore files"
        );
    }

    #[test]
    fn test_dotfiles_are_included_by_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden"), b"dot").unwrap();
        fs::write(dir.path().join("normal.txt"), b"norm").unwrap();

        let path_str = dir.path().to_str().unwrap().to_string();
        let files = scan_with_args(&[&path_str]);

        let names: Vec<_> = files.iter().map(|f| f.archive_path.as_str()).collect();
        // .hidden should be included (hidden=false in WalkBuilder)
        assert!(
            names.contains(&".hidden"),
            "dotfiles must be included when hidden=false"
        );
        assert!(names.contains(&"normal.txt"));
    }
}
