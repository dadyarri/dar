//! Shared test helpers compiled only when running tests.
use crate::archive_builder::ArchiveBuilder;
use crate::pipeline::PipelineConfig;
use std::fs::File;
use std::path::PathBuf;

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
