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

/// Recursively walk a parsed TOML table and collect all leaf-string keys as
/// dot-separated paths (e.g. `"cli.common.errors.header_write_failed"`).
/// The special `_version` key is intentionally skipped.
fn flatten_locale_keys(table: &Table, prefix: &str, out: &mut BTreeSet<String>) {
    for (k, v) in table {
        if k == "_version" {
            continue;
        }
        let full = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            Value::Table(child) => flatten_locale_keys(child, &full, out),
            Value::String(_) => {
                out.insert(full);
            }
            _ => {}
        }
    }
}

/// Collect every dot-separated string literal in `src/` that looks like a
/// locale key path (e.g. `"tui.inspect.preview.tags.imk"` listed in
/// `KNOWN_TAGS`).  These are used via `t!(&key, …)` and therefore not caught
/// by the static `t!(…)` / `translate(…)` regexes.
fn collect_source_key_strings() -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    // Match any double-quoted string literal whose value contains at least one
    // dot and consists only of lowercase letters, digits, underscores, and dots
    // – the exact character set used in all locale key paths.
    let key_re = Regex::new(r#""([a-z][a-z0-9_]*(?:\.[a-z][a-z0-9_]*)+)""#)
        .expect("Failed to compile key-string regex");

    let mut strings = BTreeSet::new();
    for file in &files {
        let content = fs::read_to_string(file)
            .unwrap_or_else(|err| panic!("Failed to read source file {file}: {err}"));
        for caps in key_re.captures_iter(&content) {
            strings.insert(caps[1].to_string());
        }
    }
    strings
}

/// Scan every `*.rs` file under `src/` for calls to `plural_key(…, "prefix", …)`
/// and return the set of prefix strings found.
///
/// `utils::plural_key` is the single production entry-point into
/// `utils::plural_suffix`.  A call like
/// ```ignore
/// plural_key(total, "tui.inspect.status_total", locale)
/// ```
/// causes four locale keys to be used at runtime:
/// `tui.inspect.status_total_one`, `…_few`, `…_many`, `…_other`.
/// By extracting the prefix here we can teach `warn_unused_locale_keys` about
/// all four variants without requiring each one to appear verbatim in a
/// `t!(…)` call.
fn collect_plural_key_prefixes() -> BTreeSet<String> {
    let mut files = Vec::new();
    collect_rs_files(Path::new("src"), &mut files);

    // Matches:  plural_key ( <anything except comma> , "the.prefix" , …
    // The prefix must be the *second* argument (the `n` count is first).
    let re = Regex::new(r#"plural_key\s*\([^,]+,\s*"([^"]+)""#)
        .expect("Failed to compile plural_key prefix regex");

    let mut prefixes = BTreeSet::new();
    for file in &files {
        let content = fs::read_to_string(file)
            .unwrap_or_else(|err| panic!("Failed to read source file {file}: {err}"));
        for caps in re.captures_iter(&content) {
            prefixes.insert(caps[1].to_string());
        }
    }
    prefixes
}

/// Emit `cargo:warning=` for every leaf key present in a locale file that is
/// not referenced anywhere in the source code.
///
/// A key is considered referenced if any of the following holds:
/// 1. It appears verbatim in `required_keys` (collected from `t!(…)` /
///    `translate(…)` call sites).
/// 2. It appears verbatim in `source_strings` (string literals used with the
///    `t!(&key, …)` dynamic form, e.g. entries in `KNOWN_TAGS`).
/// 3. It is a plural-suffix variant of a prefix in `plural_prefixes`
///    (collected from `plural_key(n, "prefix", locale)` call sites).
///    `utils::plural_suffix` returns one of the four CLDR categories
///    (`one`, `few`, `many`, `other`), so every `"{prefix}_{category}"`
///    combination is treated as used.
fn warn_unused_locale_keys(
    path: &Path,
    required_keys: &BTreeSet<String>,
    source_strings: &BTreeSet<String>,
    plural_prefixes: &BTreeSet<String>,
) {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read locale file {}: {err}", path.display()));
    let parsed: Table = toml::from_str(&raw)
        .unwrap_or_else(|err| panic!("Failed to parse locale file {}: {err}", path.display()));

    let mut all_keys = BTreeSet::new();
    flatten_locale_keys(&parsed, "", &mut all_keys);

    // The four CLDR plural categories returned by `utils::plural_suffix`.
    const PLURAL_SUFFIXES: &[&str] = &["one", "few", "many", "other"];

    for key in &all_keys {
        // 1. Statically required by a t!(…) / translate(…) call.
        if required_keys.contains(key) {
            continue;
        }

        // 2. Exact match among key-like string literals (e.g. KNOWN_TAGS entries
        //    passed to the dynamic `t!(&key, …)` form).
        if source_strings.contains(key) {
            continue;
        }

        // 3. Plural-suffix variant generated by `utils::plural_key` /
        //    `utils::plural_suffix`.
        let is_plural_form = plural_prefixes.iter().any(|prefix| {
            PLURAL_SUFFIXES
                .iter()
                .any(|sfx| key.as_str() == format!("{prefix}_{sfx}"))
        });
        if is_plural_form {
            continue;
        }

        println!(
            "cargo:warning=Unused locale key in {}: {key}",
            path.display()
        );
    }
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

    let source_strings = collect_source_key_strings();
    let plural_prefixes = collect_plural_key_prefixes();

    validate_locale_file(Path::new("locales/en.toml"), &required_keys);
    validate_locale_file(Path::new("locales/ru.toml"), &required_keys);

    warn_unused_locale_keys(
        Path::new("locales/en.toml"),
        &required_keys,
        &source_strings,
        &plural_prefixes,
    );
    warn_unused_locale_keys(
        Path::new("locales/ru.toml"),
        &required_keys,
        &source_strings,
        &plural_prefixes,
    );

    let mut app = build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string());
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut app, "dari", "completions/").ok();
    }
}
