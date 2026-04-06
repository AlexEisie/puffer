use sha2::{Digest, Sha256};

const SALT: &str = "59cf53e54c78";
const INDICES: [usize; 3] = [4, 7, 20];

/// Computes the Claude-style attribution fingerprint from the first user text and version.
pub fn compute_fingerprint(message_text: &str, version: &str) -> String {
    let mut seeded = String::from(SALT);
    for index in INDICES {
        seeded.push(message_text.chars().nth(index).unwrap_or('0'));
    }
    seeded.push_str(version);
    let digest = Sha256::digest(seeded.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    hex[..3].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_for_known_input() {
        let fingerprint = compute_fingerprint("hello world from puffer", "1.2.3");
        assert_eq!(fingerprint.len(), 3);
        assert_eq!(
            fingerprint,
            compute_fingerprint("hello world from puffer", "1.2.3")
        );
    }
}
