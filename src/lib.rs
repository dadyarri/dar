//! `dari` — binary archiver library.
//!
//! All application modules are re-exported here so that benchmark harnesses
//! and future integration-test crates can access internal types without
//! duplicating source files.

rust_i18n::i18n!("locales", fallback = "en");

pub mod archive_builder;
pub mod cli;
pub mod commands;
pub mod conflict;
pub mod constants;
pub mod counting_writer;
pub mod encryption;
pub mod errors;
pub mod extra;
pub mod extractor;
pub mod file_reader;
pub mod i18n;
pub mod models;
pub mod pipeline;
pub mod reader;
pub mod traits;
pub mod tui;
pub mod utils;
pub mod walker;

#[cfg(test)]
pub mod integration_tests;
#[cfg(test)]
pub mod test_utils;
