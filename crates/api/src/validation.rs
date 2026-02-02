//! Validation helpers and constants for API handlers

use regex::Regex;

lazy_static::lazy_static! {
    /// Team slug validation regex
    /// Allows lowercase alphanumeric characters with hyphens
    /// No leading/trailing hyphens, minimum 1 character
    pub static ref TEAM_SLUG_REGEX: Regex =
        Regex::new(r"^[a-z0-9]([a-z0-9-]*[a-z0-9])?$").unwrap();
}

/// Validate a team slug according to the rules
pub fn validate_team_slug(slug: &str) -> bool {
    // Check basic format with regex
    if !TEAM_SLUG_REGEX.is_match(slug) {
        return false;
    }

    // Check for double hyphens
    if slug.contains("--") {
        return false;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_team_slug_regex() {
        // Test the validation function instead of just the regex

        // Valid slugs
        assert!(validate_team_slug("a"));
        assert!(validate_team_slug("test"));
        assert!(validate_team_slug("test-team"));
        assert!(validate_team_slug("my-awesome-team-2024"));
        assert!(validate_team_slug("team1"));
        assert!(validate_team_slug("a1b2c3"));

        // Invalid slugs
        assert!(!validate_team_slug(""));
        assert!(!validate_team_slug("-test"));
        assert!(!validate_team_slug("test-"));
        assert!(!validate_team_slug("-test-"));
        assert!(!validate_team_slug("Test"));
        assert!(!validate_team_slug("test_team"));
        assert!(!validate_team_slug("test.team"));
        assert!(!validate_team_slug("test team"));
        assert!(!validate_team_slug("test--team"));
    }
}
