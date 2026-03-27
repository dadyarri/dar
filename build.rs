use clap::ValueEnum;
use clap_complete::{generate_to, Shell};
use regex::Regex;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use toml::{Table, Value};

rust_i18n::i18n!("locales", fallback = "en");

include!("src/cli.rs");

fn collect_rs_files(dir: &Path, out: &mut Vec<String>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("Failed to read source directory {}: {err}", dir.display()));

    for entry in entries {
        let entry = entry.unwrap_or_else(|err| {
            panic!("Failed to read source entry in {}: {err}", dir.display())
        });
        let path = entry.path();

        if path.is_dir() {
            collect_rs_files(&path, out);
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}

fn collect_required_locale_keys() -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    let translate_re =
        Regex::new(r#"translate\(\s*"([^"]+)""#).expect("Failed to compile translate() key regex");
    let t_macro_re = Regex::new(r#"(?:^|[^A-Za-z0-9_])(?:rust_i18n::)?t!\(\s*"([^"]+)""#)
        .expect("Failed to compile t! key regex");

    let mut keys = BTreeSet::new();

    for file in files {
        println!("cargo:rerun-if-changed={file}");

        let content = fs::read_to_string(&file)
            .unwrap_or_else(|err| panic!("Failed to read source file {file}: {err}"));

        for caps in translate_re.captures_iter(&content) {
            keys.insert(caps[1].to_string());
        }

        for caps in t_macro_re.captures_iter(&content) {
            keys.insert(caps[1].to_string());
        }
    }

    keys
}

fn has_key(root: &Table, dotted_key: &str) -> bool {
    let mut current = root;
    let mut parts = dotted_key.split('.').peekable();

    while let Some(part) = parts.next() {
        let Some(next) = current.get(part) else {
            return false;
        };

        if parts.peek().is_none() {
            return matches!(next, Value::String(_));
        }

        let Value::Table(next_table) = next else {
            return false;
        };

        current = next_table;
    }

    false
}

fn validate_locale_file(path: &Path, required_keys: &BTreeSet<String>) {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read locale file {}: {err}", path.display()));

    let parsed: Table = toml::from_str(&raw)
        .unwrap_or_else(|err| panic!("Failed to parse locale file {}: {err}", path.display()));

    let missing: Vec<&str> = required_keys
        .iter()
        .map(String::as_str)
        .filter(|key| !has_key(&parsed, key))
        .collect();

    if !missing.is_empty() {
        panic!(
            "Missing locale keys in {}: {}",
            path.display(),
            missing.join(", ")
        );
    }
}

fn main() {
    println!("cargo:rerun-if-changed=locales/en.toml");
    println!("cargo:rerun-if-changed=locales/ru.toml");

    let required_keys = collect_required_locale_keys();
    if required_keys.is_empty() {
        panic!("No localization keys were discovered in source code");
    }

    validate_locale_file(Path::new("locales/en.toml"), &required_keys);
    validate_locale_file(Path::new("locales/ru.toml"), &required_keys);

    let mut app = build_cli();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut app, "dari", "completions/").ok();
    }
}
