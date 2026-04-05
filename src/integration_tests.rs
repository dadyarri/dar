//! End-to-end integration tests covering the full create → append → extract → verify workflow.
//!
//! These tests exercise the public API of [`ArchiveBuilder`], [`load_archive`], and
//! the extractor together, providing a safety net for serialisation/deserialisation
//! regressions that no individual unit test catches today.
//!
//! Roadmap items covered:
//! - **5.1** create → append → extract → verify
//! - **5.2** encryption end-to-end (correct / wrong / no passphrase; append enforcement)
//! - **5.6** empty-file and single-byte-file round-trip across all compression methods

use crate::archive_builder::ArchiveBuilder;
use crate::extractor::{extract_entries, extract_entry};
use crate::i18n::Locale;
use crate::pipeline::PipelineConfig;
use crate::reader::load_archive;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn en() -> Locale {
    Locale::new("en")
}

/// Build an in-memory archive from `(archive_name, content)` pairs, write it
/// to `dir/name`, and return the path.
fn make_archive(
    dir: &tempfile::TempDir,
    name: &str,
    files: &[(&str, &[u8])],
    passphrase: Option<&str>,
) -> PathBuf {
    let archive_path = dir.path().join(name);
    let fh = File::create(&archive_path).unwrap();
    let mut builder = ArchiveBuilder::with_config(
        fh,
        PipelineConfig {
            compress_images: false,
            encryption_passphrase: passphrase.map(str::to_owned),
        },
    );
    builder.write_header().unwrap();
    for (arc_name, content) in files {
        let src = dir.path().join(arc_name);
        // Create parent directories if the archive path contains slashes.
        if let Some(parent) = Path::new(arc_name).parent() {
            if parent != Path::new("") {
                fs::create_dir_all(dir.path().join(parent)).unwrap();
            }
        }
        fs::write(&src, content).unwrap();
        builder.add_file(&src, arc_name).unwrap();
    }
    builder.build().unwrap();
    archive_path
}

/// Append additional files into an existing archive at `archive_path`.
fn append_to_archive(
    dir: &tempfile::TempDir,
    archive_path: &Path,
    files: &[(&str, &[u8])],
    passphrase: Option<&str>,
) {
    use crate::reader::load_archive;
    use std::io::{BufWriter, SeekFrom};

    let mut fh = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(archive_path)
        .unwrap();
    let state = {
        let locale = en();
        load_archive(&mut fh, archive_path.to_str().unwrap(), &locale).unwrap()
    };
    // Truncate back to index_offset so new data overwrites the old index.
    fh.seek(SeekFrom::Start(state.index_offset)).unwrap();
    let inner = fh;
    let mut bw = BufWriter::new(inner);

    let mut builder = ArchiveBuilder::with_config(
        &mut bw,
        PipelineConfig {
            compress_images: false,
            encryption_passphrase: passphrase.map(str::to_owned),
        },
    );
    builder.import_existing_entries(state.entries);
    for (arc_name, content) in files {
        let src = dir.path().join(arc_name);
        fs::write(&src, content).unwrap();
        builder.add_file(&src, arc_name).unwrap();
    }
    builder.build().unwrap();
}

/// Load all entries from an archive on disk.
fn load_entries(archive_path: &Path) -> Vec<crate::models::archive::ArchiveIndexEntryWrapper> {
    let locale = en();
    let mut f = File::open(archive_path).unwrap();
    load_archive(&mut f, archive_path.to_str().unwrap(), &locale)
        .unwrap()
        .entries
}

/// Extract all entries and return the contents of `file_name` as bytes.
fn extract_and_read(
    archive_path: &Path,
    dest: &Path,
    passphrase: Option<&str>,
    file_name: &str,
) -> Vec<u8> {
    let entries = load_entries(archive_path);
    let refs: Vec<_> = entries.iter().collect();
    extract_entries(archive_path, &refs, &entries, dest, passphrase).unwrap();
    fs::read(dest.join(file_name)).unwrap()
}

// ---------------------------------------------------------------------------
// 5.1  create → extract → verify
// ---------------------------------------------------------------------------

#[test]
fn test_create_single_file_extract_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "t.dar", &[("hello.txt", b"hello world")], None);
    let dest = dir.path().join("out");
    let got = extract_and_read(&archive, &dest, None, "hello.txt");
    assert_eq!(got, b"hello world");
}

#[test]
fn test_create_multiple_files_extract_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "multi.dar",
        &[("a.txt", b"alpha"), ("b.txt", b"beta"), ("c.txt", b"gamma")],
        None,
    );
    let entries = load_entries(&archive);
    let refs: Vec<_> = entries.iter().collect();
    let dest = dir.path().join("out_multi");
    extract_entries(&archive, &refs, &entries, &dest, None).unwrap();

    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"alpha");
    assert_eq!(fs::read(dest.join("b.txt")).unwrap(), b"beta");
    assert_eq!(fs::read(dest.join("c.txt")).unwrap(), b"gamma");
}

#[test]
fn test_create_nested_path_extract_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "nested.dar",
        &[("sub/dir/file.txt", b"nested content")],
        None,
    );
    let dest = dir.path().join("out_nested");
    let got = extract_and_read(&archive, &dest, None, "sub/dir/file.txt");
    assert_eq!(got, b"nested content");
}

// ---------------------------------------------------------------------------
// 5.1  create → append → extract → verify
// ---------------------------------------------------------------------------

#[test]
fn test_append_adds_new_file_to_archive() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "base.dar", &[("original.txt", b"original")], None);

    // Append a second file.
    append_to_archive(&dir, &archive, &[("appended.txt", b"appended")], None);

    let entries = load_entries(&archive);
    assert_eq!(entries.len(), 2);

    let dest = dir.path().join("out_append");
    let refs: Vec<_> = entries.iter().collect();
    extract_entries(&archive, &refs, &entries, &dest, None).unwrap();

    assert_eq!(fs::read(dest.join("original.txt")).unwrap(), b"original");
    assert_eq!(fs::read(dest.join("appended.txt")).unwrap(), b"appended");
}

#[test]
fn test_append_multiple_files_all_extractable() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "multi_append.dar", &[("f1.txt", b"file1")], None);

    append_to_archive(
        &dir,
        &archive,
        &[("f2.txt", b"file2"), ("f3.txt", b"file3")],
        None,
    );

    let entries = load_entries(&archive);
    assert_eq!(entries.len(), 3);

    let dest = dir.path().join("out_multi_append");
    let refs: Vec<_> = entries.iter().collect();
    extract_entries(&archive, &refs, &entries, &dest, None).unwrap();

    assert_eq!(fs::read(dest.join("f1.txt")).unwrap(), b"file1");
    assert_eq!(fs::read(dest.join("f2.txt")).unwrap(), b"file2");
    assert_eq!(fs::read(dest.join("f3.txt")).unwrap(), b"file3");
}

// ---------------------------------------------------------------------------
// 5.2  Encryption end-to-end
// ---------------------------------------------------------------------------

#[test]
fn test_encrypted_create_extract_correct_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "enc.dar",
        &[("secret.txt", b"top secret")],
        Some("hunter2"),
    );
    let dest = dir.path().join("out_enc");
    let got = extract_and_read(&archive, &dest, Some("hunter2"), "secret.txt");
    assert_eq!(got, b"top secret");
}

#[test]
fn test_encrypted_create_extract_wrong_passphrase_fails() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "enc_wrong.dar",
        &[("secret.txt", b"top secret")],
        Some("correct"),
    );
    let entries = load_entries(&archive);
    let dest = dir.path().join("out_enc_wrong");
    let result = extract_entry(&archive, &entries[0], &entries, &dest, Some("wrong"));
    assert!(result.is_err(), "wrong passphrase must fail");
}

#[test]
fn test_encrypted_create_extract_no_passphrase_fails() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "enc_none.dar",
        &[("secret.txt", b"top secret")],
        Some("pass"),
    );
    let entries = load_entries(&archive);
    let dest = dir.path().join("out_enc_none");
    let result = extract_entry(&archive, &entries[0], &entries, &dest, None);
    assert!(result.is_err(), "missing passphrase must fail");
}

#[test]
fn test_encrypted_create_multiple_files_extract_all() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "enc_multi.dar",
        &[("a.txt", b"alpha encrypted"), ("b.txt", b"beta encrypted")],
        Some("secret"),
    );
    let entries = load_entries(&archive);
    let refs: Vec<_> = entries.iter().collect();
    let dest = dir.path().join("out_enc_multi");
    extract_entries(&archive, &refs, &entries, &dest, Some("secret")).unwrap();

    assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"alpha encrypted");
    assert_eq!(fs::read(dest.join("b.txt")).unwrap(), b"beta encrypted");
}

#[test]
fn test_append_to_encrypted_archive_with_correct_passphrase() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "enc_append.dar",
        &[("orig.txt", b"original")],
        Some("mypass"),
    );

    append_to_archive(&dir, &archive, &[("new.txt", b"new file")], Some("mypass"));

    let entries = load_entries(&archive);
    assert_eq!(entries.len(), 2);

    let dest = dir.path().join("out_enc_append");
    let refs: Vec<_> = entries.iter().collect();
    extract_entries(&archive, &refs, &entries, &dest, Some("mypass")).unwrap();

    assert_eq!(fs::read(dest.join("orig.txt")).unwrap(), b"original");
    assert_eq!(fs::read(dest.join("new.txt")).unwrap(), b"new file");
}

// ---------------------------------------------------------------------------
// 5.6  Empty-file and single-byte-file round-trip
// ---------------------------------------------------------------------------

#[test]
fn test_empty_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "empty.dar", &[("empty.bin", b"")], None);
    let dest = dir.path().join("out_empty");
    let got = extract_and_read(&archive, &dest, None, "empty.bin");
    assert_eq!(got, b"", "empty file must round-trip as empty");
}

#[test]
fn test_single_byte_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "single.dar", &[("single.bin", b"\x42")], None);
    let dest = dir.path().join("out_single");
    let got = extract_and_read(&archive, &dest, None, "single.bin");
    assert_eq!(got, b"\x42");
}

#[test]
fn test_empty_file_with_brotli_extension_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    // .html triggers BrotliCompressor
    let archive = make_archive(&dir, "empty_html.dar", &[("empty.html", b"")], None);
    let dest = dir.path().join("out_empty_html");
    let got = extract_and_read(&archive, &dest, None, "empty.html");
    assert_eq!(got, b"");
}

#[test]
fn test_empty_file_with_zstd_extension_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    // .rs triggers ZStandardCompressor
    let archive = make_archive(&dir, "empty_rs.dar", &[("empty.rs", b"")], None);
    let dest = dir.path().join("out_empty_rs");
    let got = extract_and_read(&archive, &dest, None, "empty.rs");
    assert_eq!(got, b"");
}

#[test]
fn test_empty_file_with_lzma_extension_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    // .tex triggers LzmaCompressor
    let archive = make_archive(&dir, "empty_tex.dar", &[("empty.tex", b"")], None);
    let dest = dir.path().join("out_empty_tex");
    let got = extract_and_read(&archive, &dest, None, "empty.tex");
    assert_eq!(got, b"");
}

#[test]
fn test_empty_file_encrypted_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(&dir, "empty_enc.dar", &[("empty.bin", b"")], Some("pass"));
    let dest = dir.path().join("out_empty_enc");
    let got = extract_and_read(&archive, &dest, Some("pass"), "empty.bin");
    assert_eq!(got, b"");
}

#[test]
fn test_single_byte_file_encrypted_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "single_enc.dar",
        &[("single.bin", b"\xff")],
        Some("abc"),
    );
    let dest = dir.path().join("out_single_enc");
    let got = extract_and_read(&archive, &dest, Some("abc"), "single.bin");
    assert_eq!(got, b"\xff");
}

#[test]
fn test_index_entry_count_matches_file_count() {
    let dir = tempfile::tempdir().unwrap();
    let archive = make_archive(
        &dir,
        "count.dar",
        &[("a.txt", b"1"), ("b.txt", b"2"), ("c.txt", b"3")],
        None,
    );
    let entries = load_entries(&archive);
    assert_eq!(entries.len(), 3);
}
