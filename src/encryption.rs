use crate::constants::crypto;
use crate::i18n::Locale;
use clap::ArgMatches;
use eyre::{Context, eyre};
use rust_i18n::t;

/// Derive a ChaCha20-Poly1305 nonce from the first [`crypto::NONCE_LEN`] bytes of `checksum`.
///
/// Both the encryption path (`pipeline.rs`) and the decryption path (`extractor.rs`) use
/// this helper so the two never diverge.
pub fn nonce_from_checksum(checksum: &[u8; 32]) -> [u8; crypto::NONCE_LEN] {
    let mut nonce = [0u8; crypto::NONCE_LEN];
    nonce.copy_from_slice(&checksum[..crypto::NONCE_LEN]);
    nonce
}

/// Derive a per-segment nonce by XOR-ing the base nonce with the segment index.
pub fn nonce_for_segment(
    base_nonce: &[u8; crypto::NONCE_LEN],
    segment_index: u64,
) -> [u8; crypto::NONCE_LEN] {
    let mut nonce = *base_nonce;
    let counter = segment_index.to_le_bytes();
    for (idx, byte) in counter.iter().enumerate() {
        nonce[idx] ^= *byte;
    }
    nonce
}

pub fn resolve_encryption_passphrase(
    matches: &ArgMatches,
    locale: &Locale,
) -> eyre::Result<Option<String>> {
    if let Some(passphrase) = matches.get_one::<String>("encrypt-passphrase") {
        return Ok(Some(passphrase.clone()));
    }

    if matches.get_flag("encrypt") {
        let prompt = t!("cli.prompts.enter_passphrase", locale = locale.as_str());
        let passphrase = rpassword::prompt_password(prompt).wrap_err(
            t!(
                "cli.common.errors.encrypt_prompt_failed",
                locale = locale.as_str()
            )
            .to_string(),
        )?;

        if passphrase.is_empty() {
            return Err(eyre!(t!(
                "cli.common.errors.encrypt_prompt_empty",
                locale = locale.as_str()
            )));
        }

        return Ok(Some(passphrase));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::{nonce_for_segment, resolve_encryption_passphrase};
    use crate::cli::build_cli_with_translator;
    use crate::constants::crypto;
    use crate::i18n::Locale;

    fn parse(args: &[&str]) -> clap::ArgMatches {
        build_cli_with_translator(|key| rust_i18n::t!(key, locale = "en").to_string())
            .try_get_matches_from(args)
            .unwrap()
            .subcommand_matches("create")
            .unwrap()
            .clone()
    }

    #[test]
    fn test_no_encryption_flags_returns_none() {
        let matches = parse(&["dari", "create", "-f", "out.dar", "."]);
        let locale = Locale::new("en");
        let result = resolve_encryption_passphrase(&matches, &locale).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_encrypt_passphrase_flag_returns_passphrase() {
        let matches = parse(&[
            "dari",
            "create",
            "-f",
            "out.dar",
            "--encrypt-passphrase",
            "secret",
            ".",
        ]);
        let locale = Locale::new("en");
        let result = resolve_encryption_passphrase(&matches, &locale).unwrap();
        assert_eq!(result, Some("secret".to_string()));
    }

    #[test]
    fn test_nonce_for_segment_xors_le_counter_into_prefix() {
        let base = [0xAA; crypto::NONCE_LEN];
        let derived = nonce_for_segment(&base, 0x0102_0304_0506_0708);
        assert_eq!(
            derived[..8],
            [0xA2, 0xAD, 0xAC, 0xAF, 0xAE, 0xA9, 0xA8, 0xAB]
        );
        assert_eq!(derived[8..], [0xAA; 4]);
    }
}
