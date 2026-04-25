use rust_i18n::t;
use eyre::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

fn sidecar_path_for(volume_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.b3", volume_path.display()))
}

fn compute_blake3_hex(volume_path: &Path) -> Result<String> {
    let mut file = File::open(volume_path)
        .wrap_err_with(|| {
            t!(
                "cli.common.errors.sidecar_open_volume_failed",
                file = volume_path.display().to_string()
            )
            .to_string()
        })?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buf)
            .wrap_err_with(|| {
                t!(
                    "cli.common.errors.sidecar_read_volume_failed",
                    file = volume_path.display().to_string()
                )
                .to_string()
            })?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

pub fn write_b3_sidecar(volume_path: &Path) -> Result<()> {
    let digest = compute_blake3_hex(volume_path)?;
    let file_name = volume_path
        .file_name()
        .and_then(|s| s.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| volume_path.to_string_lossy().into_owned());
    let sidecar_path = sidecar_path_for(volume_path);
    let mut out = File::create(&sidecar_path)
        .wrap_err_with(|| {
            t!(
                "cli.common.errors.sidecar_create_failed",
                file = sidecar_path.display().to_string()
            )
            .to_string()
        })?;
    writeln!(out, "{digest}  {file_name}")
        .wrap_err_with(|| {
            t!(
                "cli.common.errors.sidecar_write_failed",
                file = sidecar_path.display().to_string()
            )
            .to_string()
        })?;
    Ok(())
}

pub fn verify_b3_sidecar(volume_path: &Path) -> Result<bool> {
    let sidecar_path = sidecar_path_for(volume_path);
    let file = File::open(&sidecar_path)
        .wrap_err_with(|| {
            t!(
                "cli.common.errors.sidecar_open_failed",
                file = sidecar_path.display().to_string()
            )
            .to_string()
        })?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .wrap_err_with(|| {
            t!(
                "cli.common.errors.sidecar_read_failed",
                file = sidecar_path.display().to_string()
            )
            .to_string()
        })?;
    let expected = line.split_whitespace().next().unwrap_or("");
    if expected.len() != 64 {
        return Ok(false);
    }
    let actual = compute_blake3_hex(volume_path)?;
    Ok(actual.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_verify_b3_sidecar_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let volume = dir.path().join("archive.dar");
        std::fs::write(&volume, b"payload").unwrap();

        write_b3_sidecar(&volume).unwrap();

        assert!(sidecar_path_for(&volume).exists());
        assert!(verify_b3_sidecar(&volume).unwrap());
    }

    #[test]
    fn test_verify_b3_sidecar_detects_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let volume = dir.path().join("archive.dar");
        std::fs::write(&volume, b"payload").unwrap();

        write_b3_sidecar(&volume).unwrap();
        std::fs::write(&volume, b"tampered").unwrap();

        assert!(!verify_b3_sidecar(&volume).unwrap());
    }
}
