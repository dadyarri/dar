//! Shared test helpers compiled only when running tests.
use crate::archive_builder::{ArchiveBuilder, PreparedFile};
use crate::pipeline::{CompressionPipeline, PipelineConfig};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Build a scratch `.dar` archive from a list of `(archive_name, content)` pairs and return
/// the path to the created file.
///
/// If `passphrase` is `Some`, the archive is encrypted with ChaCha20-Poly1305.
pub fn build_archive(
    dir: &tempfile::TempDir,
    name: &str,
    files: &[(&str, &[u8])],
    passphrase: Option<&str>,
) -> PathBuf {
    let archive_path = dir.path().join(name);
    let file_handle = File::create(&archive_path).unwrap();
    let mut builder = ArchiveBuilder::with_config(
        file_handle,
        PipelineConfig {
            compress_images: false,
            encryption_passphrase: passphrase.map(str::to_owned),
        },
    );
    builder.write_header().unwrap();
    for (archive_name, content) in files {
        let tmp = dir.path().join(archive_name);
        std::fs::write(&tmp, content).unwrap();
        builder.add_file(&tmp, archive_name).unwrap();
    }
    builder.build().unwrap();
    archive_path
}

/// Build a `.dar` archive entirely in memory and return the raw bytes.
///
/// Unlike [`build_archive`], this function never touches the filesystem and is therefore
/// faster in unit tests that only need to round-trip through the parser.
///
/// If `passphrase` is `Some`, the archive is encrypted with ChaCha20-Poly1305.
pub fn build_archive_bytes(files: &[(&str, &[u8])], passphrase: Option<&str>) -> Vec<u8> {
    let pipeline = CompressionPipeline::new(PipelineConfig {
        compress_images: false,
        encryption_passphrase: passphrase.map(str::to_owned),
    });
    let cursor = std::io::Cursor::new(Vec::<u8>::new());
    let mut builder = ArchiveBuilder::with_config(
        cursor,
        PipelineConfig {
            compress_images: false,
            encryption_passphrase: passphrase.map(str::to_owned),
        },
    );
    builder.write_header().unwrap();
    for (archive_name, content) in files {
        let pipeline_result = pipeline
            .process_file(Path::new(archive_name), content.to_vec())
            .unwrap();
        let prepared = PreparedFile {
            archive_path: archive_name.to_string(),
            pipeline_result,
            timestamp: 0,
            uid: 1000,
            gid: 1000,
            perm: 0o644,
        };
        builder.commit_prepared(prepared).unwrap();
    }
    builder.build().unwrap();
    builder.into_inner().into_inner()
}

/// Build a v6 `.dar` archive on disk.
///
/// Like [`build_archive`] but targets format version 6 so that external index
/// (`.dari`) tests can rely on a v6 source archive.
pub fn build_v6_archive(
    dir: &tempfile::TempDir,
    name: &str,
    files: &[(&str, &[u8])],
) -> PathBuf {
    use crate::format_version::FormatVersion;

    let archive_path = dir.path().join(name);
    let file_handle = File::create(&archive_path).unwrap();
    let mut builder = ArchiveBuilder::with_version(
        file_handle,
        PipelineConfig {
            compress_images: false,
            encryption_passphrase: None,
        },
        FormatVersion::V6,
    );
    builder.write_header().unwrap();
    for (archive_name, content) in files {
        let tmp = dir.path().join(archive_name);
        std::fs::write(&tmp, content).unwrap();
        builder.add_file(&tmp, archive_name).unwrap();
    }
    builder.build().unwrap();
    archive_path
}

/// Write a `.dari` external index sidecar for the existing archive at `archive_path`.
///
/// Reads the embedded index from `archive_path`, then writes a fresh `.dari` alongside
/// it.  The `.dari` timestamp is taken from the archive header so that
/// [`crate::reader::load_with_auto_index`] considers it fresh.
///
/// Panics on any I/O or parse error — intended only for use in tests.
pub fn write_dari_sidecar(archive_path: &Path) {
    use crate::i18n::Locale;
    use crate::index_writer::{IndexWriter, index_path_for_archive};
    use crate::reader::load_archive;

    let locale = Locale::new("en");
    let mut fh = File::open(archive_path).unwrap();
    let state = load_archive(&mut fh, archive_path.to_str().unwrap(), &locale).unwrap();
    let idx_path = index_path_for_archive(archive_path);
    let mut iw = IndexWriter::new(&idx_path, state.header.timestamp, 1).unwrap();
    for wrapper in &state.entries {
        iw.write_entry(wrapper).unwrap();
    }
    iw.finish().unwrap();
}
