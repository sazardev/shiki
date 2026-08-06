//! Per-notebook encryption at rest via `age`'s passphrase-based (scrypt)
//! symmetric encryption — the same passphrase encrypts and decrypts, no
//! keypair/identity file to lose. Chosen over a long-lived identity file
//! (the other model considered) because it trades a little friction (the
//! passphrase has to be typed in, on every machine, when a locked notebook
//! is first touched) for not depending on a single file that, if lost,
//! makes the notebook permanently unreadable — the passphrase lives in the
//! user's head instead.
//!
//! Whole files (frontmatter + body) are encrypted as one ASCII-armored
//! blob, so git still treats them as text (`git diff`/`git log -p` don't
//! flip to "binary files differ"), even though the diff itself is opaque
//! ciphertext noise for an encrypted notebook — see the history modal's
//! decrypt-then-diff handling in `shiki-tui` for how that's worked around.

use std::io::{Read, Write};

use crate::{Error, Result};

/// The exact header `age`'s ASCII-armor format always starts a blob with —
/// sniffing for this is how `Note::from_file` tells "this file is
/// encrypted, and needs a passphrase to read" apart from "this file just
/// has no frontmatter," rather than one silently masquerading as the other.
const ARMOR_HEADER: &str = "-----BEGIN AGE ENCRYPTED FILE-----";

/// True if `contents` looks like an age-armored blob — a cheap prefix
/// check, not a real parse. Used to route a file through decryption (or
/// report a clear "needs a passphrase" error) before it ever reaches
/// `Note::try_parse_frontmatter`/`synthesize_frontmatter`, which have no
/// notion of ciphertext and would otherwise treat an encrypted file as
/// plain text with no frontmatter.
pub fn looks_encrypted(contents: &str) -> bool {
    contents.trim_start().starts_with(ARMOR_HEADER)
}

/// A notebook's encryption key — really just its passphrase, held for the
/// lifetime of one unlock (a TUI session's in-memory cache, or one CLI
/// invocation). `age::scrypt` derives the actual encryption key from this
/// plus a random salt embedded in each ciphertext, so the same passphrase
/// still produces different ciphertext every time it's used.
#[derive(Clone)]
pub struct NotebookCrypto {
    passphrase: String,
}

/// Deliberately never prints `passphrase` — `Notebook` derives `Debug`,
/// and a notebook's crypto field ending up in a log line or `{:?}` status
/// message would otherwise leak the passphrase in plain text.
impl std::fmt::Debug for NotebookCrypto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotebookCrypto").finish_non_exhaustive()
    }
}

impl NotebookCrypto {
    pub fn new(passphrase: impl Into<String>) -> Self {
        Self {
            passphrase: passphrase.into(),
        }
    }

    /// Encrypts `plaintext` to an ASCII-armored blob.
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let recipient = age::scrypt::Recipient::new(self.passphrase.clone().into());
        let encryptor =
            age::Encryptor::with_recipients(std::iter::once(&recipient as &dyn age::Recipient))
                .map_err(|e| Error::Encryption(format!("could not set up encryption: {e}")))?;

        let mut armored = Vec::new();
        {
            let armored_writer = age::armor::ArmoredWriter::wrap_output(
                &mut armored,
                age::armor::Format::AsciiArmor,
            )
            .map_err(|e| Error::Encryption(e.to_string()))?;
            let mut writer = encryptor
                .wrap_output(armored_writer)
                .map_err(|e| Error::Encryption(e.to_string()))?;
            writer
                .write_all(plaintext.as_bytes())
                .map_err(|e| Error::Encryption(e.to_string()))?;
            writer
                .finish()
                .map_err(|e| Error::Encryption(e.to_string()))?
                .finish()
                .map_err(|e| Error::Encryption(e.to_string()))?;
        }
        Ok(String::from_utf8_lossy(&armored).into_owned())
    }

    /// Decrypts an ASCII-armored blob back to plaintext. A wrong passphrase
    /// (or genuinely corrupted ciphertext) surfaces as `Error::Encryption`
    /// with a message clear enough to act on, never a panic or silent
    /// garbage output.
    pub fn decrypt(&self, armored: &str) -> Result<String> {
        let identity = age::scrypt::Identity::new(self.passphrase.clone().into());
        let reader = age::armor::ArmoredReader::new(armored.as_bytes());
        let decryptor = age::Decryptor::new(reader)
            .map_err(|e| Error::Encryption(format!("could not read ciphertext: {e}")))?;
        let mut reader = decryptor
            .decrypt(std::iter::once(&identity as &dyn age::Identity))
            .map_err(|e| {
                Error::Encryption(format!("could not decrypt (wrong passphrase?): {e}"))
            })?;
        let mut plaintext = String::new();
        reader
            .read_to_string(&mut plaintext)
            .map_err(|e| Error::Encryption(e.to_string()))?;
        Ok(plaintext)
    }
}

/// A fixed plaintext used to validate a passphrase without touching any
/// real note — committed as `.shiki-encryption` at a notebook's root the
/// moment encryption is enabled, so a typo'd passphrase is caught
/// immediately rather than discovered only after bulk re-encrypting every
/// note with the wrong key.
pub const CANARY_FILE: &str = ".shiki-encryption";
const CANARY_PLAINTEXT: &str = "shiki-encryption-canary";

/// Encrypts the canary plaintext — the content `CANARY_FILE` should hold.
pub fn canary_blob(crypto: &NotebookCrypto) -> Result<String> {
    crypto.encrypt(CANARY_PLAINTEXT)
}

/// Verifies `passphrase` against an already-written canary blob. `Ok(true)`
/// means the passphrase is correct; `Ok(false)` means decryption succeeded
/// but produced something other than the expected canary text (a corrupted
/// canary file, not a wrong passphrase — decrypt would normally error
/// first); `Err` is the common case, a wrong passphrase failing to decrypt
/// at all.
pub fn verify_canary(crypto: &NotebookCrypto, armored: &str) -> Result<bool> {
    Ok(crypto.decrypt(armored)? == CANARY_PLAINTEXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypts_and_decrypts_round_trip() {
        let crypto = NotebookCrypto::new("correct horse battery staple");
        let armored = crypto.encrypt("---\ntitle: T\n---\n\nbody").unwrap();
        assert!(looks_encrypted(&armored));
        let plain = crypto.decrypt(&armored).unwrap();
        assert_eq!(plain, "---\ntitle: T\n---\n\nbody");
    }

    #[test]
    fn wrong_passphrase_fails_to_decrypt() {
        let crypto = NotebookCrypto::new("correct passphrase");
        let armored = crypto.encrypt("secret content").unwrap();
        let wrong = NotebookCrypto::new("wrong passphrase");
        assert!(wrong.decrypt(&armored).is_err());
    }

    #[test]
    fn plain_text_does_not_look_encrypted() {
        assert!(!looks_encrypted("---\ntitle: T\n---\n\nbody"));
        assert!(!looks_encrypted(""));
    }

    #[test]
    fn canary_round_trips_and_rejects_wrong_passphrase() {
        let crypto = NotebookCrypto::new("open sesame");
        let blob = canary_blob(&crypto).unwrap();
        assert!(verify_canary(&crypto, &blob).unwrap());

        let wrong = NotebookCrypto::new("not sesame");
        assert!(verify_canary(&wrong, &blob).is_err());
    }
}
