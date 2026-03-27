use crate::i18n::Locale;
use clap::parser::ValuesRef;
use eyre::{Context, Result};
use ignore::WalkBuilder;
use rust_i18n::t;
use std::fs::canonicalize;
use std::path::{Path, PathBuf};

pub fn scan_files(paths: ValuesRef<String>, locale: &Locale) -> Result<Vec<PathBuf>> {
    let mut files = vec![];

    for item in paths {
        let relative_path = Path::new(item);
        let absolute_path = canonicalize(relative_path).wrap_err_with(|| {
            t!(
                "error.canonicalize_failed",
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
                        "error.walk_failed",
                        locale = locale.as_str(),
                        path = absolute_path.display()
                    )
                })?;

                if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    files.push(entry.path().to_path_buf());
                }
            }
        }
    }

    Ok(files)
}
