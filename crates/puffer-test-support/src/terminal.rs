use anyhow::{anyhow, Result};

/// Normalizes snapshot text to make terminal output comparisons stable.
pub fn normalize_snapshot_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Asserts that one normalized text blob contains another.
pub fn assert_contains(haystack: &str, needle: &str) -> Result<()> {
    let haystack = normalize_snapshot_text(haystack);
    let needle = normalize_snapshot_text(needle);
    if haystack.contains(&needle) {
        Ok(())
    } else {
        Err(anyhow!("expected normalized output to contain `{needle}`"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_removes_crlf_and_trailing_spaces() {
        let normalized = normalize_snapshot_text("a  \r\nb\t \r\n");
        assert_eq!(normalized, "a\nb");
    }
}
