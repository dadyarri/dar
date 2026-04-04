use crate::constants::format::CHUNK_SIZE;
use crate::pipeline::{CompressionPipeline, PipelineFileData};
use crate::utils::get_mode;
use eyre::Result;
use std::fs::{File, metadata};
use std::io::Read;
use std::path::PathBuf;
use std::time::SystemTime;

/// Holds all data needed to write a file into an archive.
///
/// Produced by [`prepare_file_from_disk`] so that file reading and compression
/// can run in parallel on multiple threads before the single-threaded write
/// phase begins.
pub struct PreparedFile {
    pub archive_path: String,
    pub pipeline_result: PipelineFileData,
    pub timestamp: u64,
    pub uid: u32,
    pub gid: u32,
    pub perm: u16,
}

/// Read file content into memory, using chunked I/O for files larger than
/// [`CHUNK_SIZE`] to bound peak heap usage.
///
/// Safe to call from multiple threads simultaneously — no shared state is
/// mutated.
fn read_file_content(file_path: &PathBuf, file_size: usize) -> Result<Vec<u8>> {
    if file_size > CHUNK_SIZE {
        let mut file = File::open(file_path)?;
        let mut content = Vec::with_capacity(file_size);
        let mut buffer = vec![0u8; CHUNK_SIZE];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            content.extend_from_slice(&buffer[..n]);
        }
        Ok(content)
    } else {
        std::fs::read(file_path).map_err(Into::into)
    }
}

/// Read, checksum, and compress a single file without touching the archive writer.
///
/// Safe to call from multiple threads simultaneously — the only shared state is
/// the [`CompressionPipeline`] which is stateless after construction.
///
/// # Errors
///
/// Returns an error if:
/// - the file's filesystem metadata cannot be read,
/// - reading the file content fails,
/// - or the pipeline fails (e.g. encryption error).
pub fn prepare_file_from_disk(
    pipeline: &CompressionPipeline,
    file_path: &PathBuf,
    archive_path: &str,
) -> Result<PreparedFile> {
    let fs_meta = metadata(file_path)?;
    let file_size = fs_meta.len() as usize;
    let (uid, gid, perm) = get_mode(&fs_meta);

    let timestamp = fs_meta
        .modified()?
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    let file_content = read_file_content(file_path, file_size)?;
    let pipeline_result = pipeline.process_file(file_path, file_content)?;

    Ok(PreparedFile {
        archive_path: archive_path.to_string(),
        pipeline_result,
        timestamp,
        uid,
        gid,
        perm,
    })
}
