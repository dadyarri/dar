use crate::i18n::Locale;
use crate::reader;
use crate::tui::{
    App,
    state::{
        AppState, ExtractDialog, Focus, MetaSearchState, PreviewMode, PreviewState, SearchState,
    },
    tree,
};
use clap::ArgMatches;
use eyre::{Context, Result, eyre};
use ratatui::widgets::TableState;
use std::fs::File;
use std::path::PathBuf;

pub fn call(matches: &ArgMatches, locale: &Locale) -> Result<()> {
    let file_path = matches.get_one::<String>("file").ok_or_else(|| {
        eyre!(rust_i18n::t!(
            "cli.common.errors.file_required",
            locale = locale.as_str()
        ))
    })?;
    let passphrase = matches.get_one::<String>("encrypt-passphrase").cloned();

    let mut file = File::open(file_path).wrap_err_with(|| {
        rust_i18n::t!(
            "cli.inspect.errors.open_failed",
            locale = locale.as_str(),
            file = file_path
        )
        .to_string()
    })?;

    let archive_state = reader::load_archive(&mut file, file_path, locale)?;

    let tree_root = tree::build_tree(&archive_state.entries);
    let visible = tree::flatten_visible(&tree_root);

    let mut table_state = TableState::default();
    if !visible.is_empty() {
        table_state.select(Some(0));
    }

    let app_state = AppState {
        archive_path: PathBuf::from(file_path),
        entries: archive_state.entries,
        passphrase,
        locale: locale.clone(),
        powerline: crate::tui::icons::detect_powerline(),
        tree_root,
        visible,
        table_state,
        search: SearchState {
            query: String::new(),
            active: false,
        },
        meta_search: MetaSearchState {
            query: String::new(),
            active: false,
            error: None,
        },
        extract: ExtractDialog {
            active: false,
            path: std::env::current_dir()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|_| String::from(".")),
            resolved: None,
            error: None,
        },
        preview: PreviewState {
            mode: PreviewMode::Closed,
            focus: Focus::List,
            scroll: 0,
            line_count: 0,
            viewport_height: 0,
            cache: None,
        },
    };

    App::run(app_state)
}
