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
