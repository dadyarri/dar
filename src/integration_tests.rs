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
use std::io::{Seek, SeekFrom};
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

// ---------------------------------------------------------------------------
// Phase 2 — External index (.dari)
// ---------------------------------------------------------------------------

/// Build a v6 archive on disk (no .dari auto-generated by the raw builder).
fn make_v6_archive(dir: &tempfile::TempDir, name: &str, files: &[(&str, &[u8])]) -> PathBuf {
    crate::test_utils::build_v6_archive(dir, name, files)
}

/// Write a .dari sidecar for an existing archive.
fn make_dari(archive_path: &Path) {
    crate::test_utils::write_dari_sidecar(archive_path);
}

#[test]
fn test_v6_create_generates_dari_sidecar_via_helper() {
    // build_v6_archive + write_dari_sidecar must produce a parseable .dari.
    use crate::reader::load_index;

    let dir = tempfile::tempdir().unwrap();
    let archive = make_v6_archive(&dir, "gen_dari.dar", &[("hello.txt", b"hello from v6")]);
    make_dari(&archive);

    let dari_path = archive.with_extension("dari");
    assert!(dari_path.exists(), ".dari must exist after writing sidecar");

    let locale = en();
    let mut fh = fs::File::open(&dari_path).unwrap();
    let state = load_index(&mut fh, dari_path.to_str().unwrap(), &locale).unwrap();
    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.entries[0].path, "hello.txt");
}

#[test]
fn test_v6_dari_timestamp_matches_archive_header() {
    use crate::index_writer::{INDEX_SIGNATURE, IndexFileHeader};
    use bytemuck::from_bytes;

    let dir = tempfile::tempdir().unwrap();
    let archive = make_v6_archive(&dir, "ts_check.dar", &[("data.bin", b"binary data")]);
    make_dari(&archive);

    // Read the archive header timestamp (bytes 5..13 of the v6 17-byte header).
    let archive_bytes = fs::read(&archive).unwrap();
    let archive_ts = u64::from_le_bytes(archive_bytes[5..13].try_into().unwrap());

    // Read the .dari header.
    let dari_path = archive.with_extension("dari");
    let dari_bytes = fs::read(&dari_path).unwrap();
    let dari_hdr = *from_bytes::<IndexFileHeader>(&dari_bytes[..17]);
    let dari_ts = dari_hdr.archive_timestamp;

    assert_eq!(dari_ts, archive_ts, ".dari timestamp must match the archive header timestamp");
    assert_eq!(
        &dari_hdr.signature,
        INDEX_SIGNATURE,
        "dari signature must be DARIDX"
    );
}

#[test]
fn test_v6_auto_index_extract_roundtrip() {
    // v6 archive + fresh .dari → load via auto-index → extract → correct content.
    use crate::reader::load_with_auto_index;

    let dir = tempfile::tempdir().unwrap();
    let archive = make_v6_archive(
        &dir,
        "ext_rt.dar",
        &[("alpha.txt", b"alpha"), ("beta.rs", b"fn main() {}")],
    );
    make_dari(&archive);

    let locale = en();
    let mut fh = File::open(&archive).unwrap();
    let state = load_with_auto_index(&mut fh, &archive, false, &locale).unwrap();

    assert_eq!(state.entries.len(), 2);

    let dest = dir.path().join("out_ext_rt");
    let refs: Vec<_> = state.entries.iter().collect();
    extract_entries(&archive, &refs, &state.entries, &dest, None).unwrap();

    assert_eq!(fs::read(dest.join("alpha.txt")).unwrap(), b"alpha");
    assert_eq!(
        fs::read(dest.join("beta.rs")).unwrap(),
        b"fn main() {}"
    );
}

#[test]
fn test_v6_reindex_then_auto_index_loads_all_entries() {
    // reindex command → .dari created → auto-index uses it for all entries.
    use crate::commands::reindex;
    use crate::reader::load_with_auto_index;
    use clap::{Arg, ArgAction, Command};

    let dir = tempfile::tempdir().unwrap();
    let archive = make_v6_archive(
        &dir,
        "reindex_auto.dar",
        &[
            ("one.txt", b"first"),
            ("two.txt", b"second"),
            ("three.txt", b"third"),
        ],
    );

    // Run the reindex command to generate .dari.
    let cmd = Command::new("dari").subcommand(
        Command::new("reindex")
            .arg(
                Arg::new("file")
                    .short('f')
                    .long("file")
                    .action(ArgAction::Set)
                    .required(true),
            )
            .arg(
                Arg::new("encrypt-passphrase")
                    .long("encrypt-passphrase")
                    .action(ArgAction::Set),
            ),
    );
    let full_args = ["dari", "reindex", "-f", archive.to_str().unwrap()];
    let root_matches = cmd.get_matches_from(full_args);
    let sub = root_matches.subcommand_matches("reindex").unwrap().clone();

    let locale = en();
    reindex::call(&sub, &locale).unwrap();

    let dari_path = archive.with_extension("dari");
    assert!(dari_path.exists(), ".dari must be created by reindex");

    // auto-index should prefer the .dari and return all 3 entries.
    let mut fh = File::open(&archive).unwrap();
    let state = load_with_auto_index(&mut fh, &archive, false, &locale).unwrap();

    assert_eq!(state.entries.len(), 3);
    let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"one.txt"));
    assert!(paths.contains(&"two.txt"));
    assert!(paths.contains(&"three.txt"));
}

#[test]
fn test_v6_no_index_flag_bypasses_dari_during_extraction() {
    // --no-index: extraction falls back to the embedded index even with a fresh .dari.
    use crate::reader::load_with_auto_index;

    let dir = tempfile::tempdir().unwrap();
    let archive = make_v6_archive(
        &dir,
        "bypass_extract.dar",
        &[("file.txt", b"content for bypass test")],
    );
    make_dari(&archive);

    let locale = en();
    let mut fh = File::open(&archive).unwrap();
    // no_index = true → use embedded index, ignoring the fresh .dari.
    let state = load_with_auto_index(&mut fh, &archive, true, &locale).unwrap();

    assert_eq!(state.entries.len(), 1);
    assert_eq!(state.entries[0].path, "file.txt");

    let dest = dir.path().join("out_bypass");
    let refs: Vec<_> = state.entries.iter().collect();
    extract_entries(&archive, &refs, &state.entries, &dest, None).unwrap();
    assert_eq!(
        fs::read(dest.join("file.txt")).unwrap(),
        b"content for bypass test"
    );
}

#[test]
fn test_v6_append_regenerates_dari_with_all_entries() {
    // After appending new files to a v6 archive, the .dari should be regenerated
    // and load_with_auto_index should return all entries (old + new).
    use crate::format_version::FormatVersion;
    use crate::reader::load_with_auto_index;
    use std::io::BufWriter;

    let dir = tempfile::tempdir().unwrap();

    // Build the base v6 archive (no .dari yet).
    let archive_path = make_v6_archive(&dir, "append_v6.dar", &[("orig.txt", b"original")]);

    // Append a second file using ArchiveBuilder (simulating the append command path).
    {
        use crate::reader::load_archive;

        let mut fh = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive_path)
            .unwrap();
        let locale = en();
        let state = load_archive(&mut fh, archive_path.to_str().unwrap(), &locale).unwrap();
        let archive_ts = state.header.timestamp;
        let index_offset = state.index_offset;

        fh.seek(SeekFrom::Start(index_offset)).unwrap();
        fh.set_len(index_offset).unwrap();

        let mut builder = ArchiveBuilder::with_version(
            BufWriter::new(fh),
            PipelineConfig {
                compress_images: false,
                encryption_passphrase: None,
            },
            FormatVersion::V6,
        );
        builder.import_existing_entries(state.entries);

        // Attach an IndexWriter so the .dari is regenerated on build().
        let idx_path = crate::index_writer::index_path_for_archive(&archive_path);
        let iw =
            crate::index_writer::IndexWriter::new(&idx_path, archive_ts, 1).unwrap();
        builder.set_index_writer(iw);

        let new_file = dir.path().join("appended.txt");
        fs::write(&new_file, b"appended content").unwrap();
        builder.add_file(&new_file, "appended.txt").unwrap();
        builder.build().unwrap();
    }

    // .dari must now exist and reflect both entries.
    let dari_path = archive_path.with_extension("dari");
    assert!(dari_path.exists(), ".dari must exist after append");

    let locale = en();
    let mut fh = File::open(&archive_path).unwrap();
    let state = load_with_auto_index(&mut fh, &archive_path, false, &locale).unwrap();

    assert_eq!(state.entries.len(), 2, "both entries must be in the .dari");
    let paths: Vec<&str> = state.entries.iter().map(|e| e.path.as_str()).collect();
    assert!(paths.contains(&"orig.txt"));
    assert!(paths.contains(&"appended.txt"));
}
