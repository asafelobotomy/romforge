//! The region order as people type it.

/// `"USA, Europe,Japan"` as the list [`super::Settings::regions`] keeps
/// (trimmed, empty entries dropped).
pub fn parse_regions(text: &str) -> Vec<String> {
    text.split(',')
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn trims_and_drops_empty_entries() {
        assert_eq!(
            super::parse_regions(" USA, Europe,,Japan "),
            ["USA", "Europe", "Japan"]
        );
        assert!(super::parse_regions("").is_empty());
    }
}
