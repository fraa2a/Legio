//! SHA-256 digests in the lowercase hex form stored and compared across the app.

use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub(crate) fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Feeds the whole reader to the hasher in fixed-size chunks.
pub(crate) fn update_from(hasher: &mut Sha256, reader: &mut impl Read) -> io::Result<()> {
    let mut buffer = [0; 64 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => return Ok(()),
            Ok(size) => hasher.update(&buffer[..size]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streamed_and_whole_digests_match_the_known_value() {
        const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(hex_digest(b"abc"), ABC);
        let mut hasher = Sha256::new();
        update_from(&mut hasher, &mut &b"abc"[..]).unwrap();
        assert_eq!(hex(&hasher.finalize()), ABC);
    }
}
