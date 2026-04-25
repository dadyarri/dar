/// Binary format identifiers and version constants.
pub mod format {
    pub const SIGNATURE: &[u8; 4] = b"DARI";
    pub const FOOTER_SIGNATURE: &[u8; 7] = b"DARIEND";
    /// Default write version (v5 is the stable format).
    pub const VERSION: u8 = 5;
    /// Maximum format version this binary can read.
    pub const MAX_SUPPORTED_VERSION: u8 = 6;
    pub const CHUNK_SIZE: usize = 512 * 1024;
}

/// Bit-flag values stored in `ArchiveIndexEntry::bitflags`.
pub mod flags {
    pub const LINKED_DATA: u16 = 0b0000_0000_0000_0001;
    pub const ENCRYPTED_DATA: u16 = 0b0000_0000_0000_0010;
    pub const CHUNKED_ENCRYPTION: u16 = 0b0000_0000_0000_0100;
}

/// Short string keys used in the `extra` field of index entries.
pub mod extra_keys {
    // Encryption subsystem
    pub const ENC_ALGO: &str = "e";
    pub const ENC_NONCE: &str = "en";
    pub const ENC_TAG: &str = "et";
    pub const ENC_SEGMENTS: &str = "es";

    // Image EXIF
    pub const IMG_MAKE: &str = "imk";
    pub const IMG_MODEL: &str = "imd";
    pub const IMG_DATETIME_ORIGINAL: &str = "idt";

    // Audio tags
    pub const AUDIO_TITLE: &str = "atl";
    pub const AUDIO_ARTIST: &str = "aar";
    pub const AUDIO_ALBUM: &str = "aal";
    pub const AUDIO_GENRE: &str = "agn";
}

/// Cryptographic length constants for ChaCha20-Poly1305.
pub mod crypto {
    pub const NONCE_LEN: usize = 12;
    pub const TAG_LEN: usize = 16;
    pub const SEGMENT_SIZE: usize = 1_048_576;
}
