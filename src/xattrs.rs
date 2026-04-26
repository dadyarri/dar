use eyre::{Result, eyre};
use std::path::Path;

pub const SYNTHETIC_INODE: &str = "__dari_inode__";
pub const SYNTHETIC_DEVICE: &str = "__dari_device__";
pub const SYNTHETIC_HARDLINK_TARGET: &str = "__dari_hardlink_target__";

pub type XattrPair = (String, Vec<u8>);

pub fn encode_xattr_blob(xattrs: &[XattrPair]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for (name, value) in xattrs {
        let name_bytes = name.as_bytes();
        let name_len: u16 = name_bytes
            .len()
            .try_into()
            .map_err(|_| eyre!("xattr name is too long"))?;
        let value_len: u32 = value
            .len()
            .try_into()
            .map_err(|_| eyre!("xattr value is too long"))?;
        out.extend_from_slice(&name_len.to_le_bytes());
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&value_len.to_le_bytes());
        out.extend_from_slice(value);
    }
    Ok(out)
}

pub fn decode_xattr_blob(bytes: &[u8]) -> Result<Vec<XattrPair>> {
    let mut pos = 0usize;
    let mut xattrs = Vec::new();

    while pos < bytes.len() {
        if pos + 2 > bytes.len() {
            return Err(eyre!("xattr blob truncated at name length"));
        }
        let name_len = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
        pos += 2;

        if pos + name_len > bytes.len() {
            return Err(eyre!("xattr blob truncated at name bytes"));
        }
        let name = String::from_utf8(bytes[pos..pos + name_len].to_vec())
            .map_err(|_| eyre!("xattr name is not valid UTF-8"))?;
        pos += name_len;

        if pos + 4 > bytes.len() {
            return Err(eyre!("xattr blob truncated at value length"));
        }
        let value_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if pos + value_len > bytes.len() {
            return Err(eyre!("xattr blob truncated at value bytes"));
        }
        let value = bytes[pos..pos + value_len].to_vec();
        pos += value_len;
        xattrs.push((name, value));
    }

    Ok(xattrs)
}

pub fn hardlink_target(xattrs: &[XattrPair]) -> Option<&str> {
    xattrs
        .iter()
        .find(|(name, _)| name == SYNTHETIC_HARDLINK_TARGET)
        .and_then(|(_, value)| std::str::from_utf8(value).ok())
}

pub fn non_synthetic_xattrs(xattrs: &[XattrPair]) -> impl Iterator<Item = (&str, &[u8])> {
    xattrs.iter().filter_map(|(name, value)| {
        if name == SYNTHETIC_INODE || name == SYNTHETIC_DEVICE || name == SYNTHETIC_HARDLINK_TARGET
        {
            None
        } else {
            Some((name.as_str(), value.as_slice()))
        }
    })
}

#[cfg(unix)]
pub fn collect_xattrs(path: &Path) -> Vec<XattrPair> {
    xattr::list(path)
        .into_iter()
        .flatten()
        .filter_map(|name| {
            xattr::get(path, &name)
                .ok()
                .flatten()
                .map(|value| (name.to_string_lossy().into_owned(), value))
        })
        .collect()
}

#[cfg(not(unix))]
pub fn collect_xattrs(_: &Path) -> Vec<XattrPair> {
    vec![]
}

#[cfg(unix)]
pub fn collect_device_inode(path: &Path) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;

    let meta = std::fs::symlink_metadata(path).ok()?;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
pub fn collect_device_inode(_: &Path) -> Option<(u64, u64)> {
    None
}

pub fn inode_xattrs(device: u64, inode: u64) -> Vec<XattrPair> {
    vec![
        (SYNTHETIC_DEVICE.to_string(), device.to_le_bytes().to_vec()),
        (SYNTHETIC_INODE.to_string(), inode.to_le_bytes().to_vec()),
    ]
}

pub fn hardlink_target_xattr(target: &str) -> XattrPair {
    (
        SYNTHETIC_HARDLINK_TARGET.to_string(),
        target.as_bytes().to_vec(),
    )
}

#[cfg(unix)]
pub fn restore_xattrs(path: &Path, xattrs: &[XattrPair]) -> Result<()> {
    for (name, value) in non_synthetic_xattrs(xattrs) {
        xattr::set(path, name, value)?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn restore_xattrs(_: &Path, _: &[XattrPair]) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_xattr_blob_roundtrip() {
        let input = vec![
            ("user.test".to_string(), b"value".to_vec()),
            ("user.bin".to_string(), vec![0, 1, 2, 3]),
        ];
        let encoded = encode_xattr_blob(&input).unwrap();
        let decoded = decode_xattr_blob(&encoded).unwrap();
        assert_eq!(decoded, input);
    }

    #[test]
    fn hardlink_target_reads_synthetic_value() {
        let xattrs = vec![hardlink_target_xattr("dir/file.txt")];
        assert_eq!(hardlink_target(&xattrs), Some("dir/file.txt"));
    }
}
