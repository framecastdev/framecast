//! URN (Uniform Resource Name) types and parsing for Framecast
//!
//! Framecast uses URNs to identify and scope resources:
//! - framecast:user:{user_id} - Personal resources
//! - framecast:team:{team_id} - Team-shared resources
//! - framecast:{team_id}:{user_id} - Team-private resources
//! - framecast:system:{category}:{name} - System assets

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::error::{Error, Result};

/// URN represents a Framecast Uniform Resource Name
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Urn(String);

impl Urn {
    /// Create a new URN from a string, validating format
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        let urn = Urn(s);
        urn.validate()?;
        Ok(urn)
    }

    /// Create a user URN
    pub fn user(user_id: Uuid) -> Self {
        Urn(format!("framecast:user:{}", user_id))
    }

    /// Create a team URN
    pub fn team(team_id: Uuid) -> Self {
        Urn(format!("framecast:team:{}", team_id))
    }

    /// Create a team-private URN (user's work within a team)
    pub fn team_user(team_id: Uuid, user_id: Uuid) -> Self {
        Urn(format!("framecast:{}:{}", team_id, user_id))
    }

    /// Create a system asset URN
    pub fn system(category: &str, name: &str) -> Self {
        Urn(format!("framecast:system:{}:{}", category, name))
    }

    /// Get the raw URN string
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse the URN and extract components
    pub fn parse(&self) -> Result<UrnComponents> {
        let parts: Vec<&str> = self.0.split(':').collect();

        if parts.len() < 2 || parts[0] != "framecast" {
            return Err(Error::Validation(
                "URN must start with 'framecast:'".to_string(),
            ));
        }

        match parts.as_slice() {
            ["framecast", "user", user_id] => {
                let user_uuid = Uuid::parse_str(user_id)
                    .map_err(|_| Error::Validation("Invalid user UUID".to_string()))?;
                Ok(UrnComponents::User { user_id: user_uuid })
            }
            ["framecast", "team", team_id] => {
                let team_uuid = Uuid::parse_str(team_id)
                    .map_err(|_| Error::Validation("Invalid team UUID".to_string()))?;
                Ok(UrnComponents::Team { team_id: team_uuid })
            }
            ["framecast", team_id, user_id] => {
                let team_uuid = Uuid::parse_str(team_id)
                    .map_err(|_| Error::Validation("Invalid team UUID".to_string()))?;
                let user_uuid = Uuid::parse_str(user_id)
                    .map_err(|_| Error::Validation("Invalid user UUID".to_string()))?;
                Ok(UrnComponents::TeamUser {
                    team_id: team_uuid,
                    user_id: user_uuid,
                })
            }
            ["framecast", "system", category, name] => {
                if category.is_empty() || name.is_empty() {
                    return Err(Error::Validation(
                        "System URN category and name cannot be empty".to_string(),
                    ));
                }
                Ok(UrnComponents::System {
                    category: category.to_string(),
                    name: name.to_string(),
                })
            }
            _ => Err(Error::Validation("Invalid URN format".to_string())),
        }
    }

    /// Validate URN format
    fn validate(&self) -> Result<()> {
        self.parse().map(|_| ())
    }

    /// Check if this is a user URN
    pub fn is_user(&self) -> bool {
        matches!(self.parse(), Ok(UrnComponents::User { .. }))
    }

    /// Check if this is a team URN
    pub fn is_team(&self) -> bool {
        matches!(self.parse(), Ok(UrnComponents::Team { .. }))
    }

    /// Check if this is a team-user URN
    pub fn is_team_user(&self) -> bool {
        matches!(self.parse(), Ok(UrnComponents::TeamUser { .. }))
    }

    /// Check if this is a system URN
    pub fn is_system(&self) -> bool {
        matches!(self.parse(), Ok(UrnComponents::System { .. }))
    }
}

/// Components of a parsed URN
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UrnComponents {
    User { user_id: Uuid },
    Team { team_id: Uuid },
    TeamUser { team_id: Uuid, user_id: Uuid },
    System { category: String, name: String },
}

impl fmt::Display for Urn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Urn {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Urn::new(s)
    }
}

impl From<Urn> for String {
    fn from(urn: Urn) -> Self {
        urn.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_urn() {
        let user_id = Uuid::new_v4();
        let urn = Urn::user(user_id);

        assert_eq!(urn.as_str(), format!("framecast:user:{}", user_id));
        assert!(urn.is_user());
        assert!(!urn.is_team());

        match urn.parse().unwrap() {
            UrnComponents::User { user_id: parsed_id } => assert_eq!(parsed_id, user_id),
            _ => panic!("Expected User component"),
        }
    }

    #[test]
    fn test_team_urn() {
        let team_id = Uuid::new_v4();
        let urn = Urn::team(team_id);

        assert_eq!(urn.as_str(), format!("framecast:team:{}", team_id));
        assert!(urn.is_team());
        assert!(!urn.is_user());

        match urn.parse().unwrap() {
            UrnComponents::Team { team_id: parsed_id } => assert_eq!(parsed_id, team_id),
            _ => panic!("Expected Team component"),
        }
    }

    #[test]
    fn test_team_user_urn() {
        let team_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();
        let urn = Urn::team_user(team_id, user_id);

        assert_eq!(urn.as_str(), format!("framecast:{}:{}", team_id, user_id));
        assert!(urn.is_team_user());

        match urn.parse().unwrap() {
            UrnComponents::TeamUser {
                team_id: parsed_team,
                user_id: parsed_user,
            } => {
                assert_eq!(parsed_team, team_id);
                assert_eq!(parsed_user, user_id);
            }
            _ => panic!("Expected TeamUser component"),
        }
    }

    #[test]
    fn test_system_urn() {
        let urn = Urn::system("sfx", "whoosh");

        assert_eq!(urn.as_str(), "framecast:system:sfx:whoosh");
        assert!(urn.is_system());

        match urn.parse().unwrap() {
            UrnComponents::System { category, name } => {
                assert_eq!(category, "sfx");
                assert_eq!(name, "whoosh");
            }
            _ => panic!("Expected System component"),
        }
    }

    #[test]
    fn test_invalid_urn() {
        assert!(Urn::new("invalid:urn").is_err());
        assert!(Urn::new("framecast:invalid").is_err());
        assert!(Urn::new("framecast:user:not-a-uuid").is_err());
    }

    #[test]
    fn test_team_user_urn_edge_cases() {
        // Test various team-user URN formats and edge cases
        let team_id = Uuid::new_v4();
        let user_id = Uuid::new_v4();

        // Valid team-user URN
        let urn = Urn::team_user(team_id, user_id);
        assert!(urn.is_team_user());
        assert!(!urn.is_user());
        assert!(!urn.is_team());
        assert!(!urn.is_system());

        // Parse and verify components
        match urn.parse().unwrap() {
            UrnComponents::TeamUser {
                team_id: parsed_team,
                user_id: parsed_user,
            } => {
                assert_eq!(parsed_team, team_id);
                assert_eq!(parsed_user, user_id);
            }
            _ => panic!("Expected TeamUser component"),
        }

        // Test string format
        let expected = format!("framecast:{}:{}", team_id, user_id);
        assert_eq!(urn.as_str(), expected);
    }

    #[test]
    fn test_system_urn_categories() {
        // Test various system URN categories
        let test_cases = [
            ("sfx", "whoosh"),
            ("music", "ambient-forest"),
            ("textures", "concrete-wall"),
            ("models", "character-base"),
            ("templates", "corporate-intro"),
        ];

        for (category, name) in test_cases {
            let urn = Urn::system(category, name);
            assert!(urn.is_system());
            assert!(!urn.is_user());
            assert!(!urn.is_team());
            assert!(!urn.is_team_user());

            let expected = format!("framecast:system:{}:{}", category, name);
            assert_eq!(urn.as_str(), expected);

            match urn.parse().unwrap() {
                UrnComponents::System {
                    category: parsed_category,
                    name: parsed_name,
                } => {
                    assert_eq!(parsed_category, category);
                    assert_eq!(parsed_name, name);
                }
                _ => panic!("Expected System component"),
            }
        }
    }

    #[test]
    fn test_urn_malformed_cases() {
        // Test various malformed URN cases
        let invalid_urns = [
            "",                                           // Empty string
            "framecast",                                  // Missing components
            "framecast:",                                 // Empty after prefix
            "framecast::",                                // Double colon
            "not-framecast:user:123",                     // Wrong prefix
            "framecast:user:",                            // Missing UUID
            "framecast:team:",                            // Missing UUID
            "framecast:user:invalid-uuid",                // Invalid UUID format
            "framecast:team:invalid-uuid",                // Invalid UUID format
            "framecast:unknown:123",                      // Unknown type
            "framecast:system",                           // Incomplete system URN
            "framecast:system:category",                  // Incomplete system URN
            "framecast:system::name",                     // Empty category
            "framecast:system:category:",                 // Empty name
            "framecast:user:valid-uuid:extra",            // Too many components
            "framecast:team:valid-uuid:extra:components", // Too many components
        ];

        for invalid_urn in invalid_urns {
            assert!(
                Urn::new(invalid_urn).is_err(),
                "Expected URN to be invalid: '{}'",
                invalid_urn
            );
        }
    }

    #[test]
    fn test_urn_from_str_trait() {
        // Test FromStr trait implementation
        use std::str::FromStr;

        let user_id = Uuid::new_v4();
        let urn_str = format!("framecast:user:{}", user_id);

        let urn = Urn::from_str(&urn_str).unwrap();
        assert_eq!(urn.as_str(), urn_str);
        assert!(urn.is_user());

        // Test invalid URN via FromStr
        assert!(Urn::from_str("invalid:urn").is_err());
    }

    #[test]
    fn test_urn_string_conversion() {
        // Test String conversion
        let user_id = Uuid::new_v4();
        let urn = Urn::user(user_id);

        let urn_string: String = urn.clone().into();
        assert_eq!(urn_string, urn.as_str());

        let expected = format!("framecast:user:{}", user_id);
        assert_eq!(urn_string, expected);
    }

    #[test]
    fn test_urn_display_formatting() {
        // Test Display trait implementation
        let team_id = Uuid::new_v4();
        let urn = Urn::team(team_id);

        let formatted = format!("{}", urn);
        let expected = format!("framecast:team:{}", team_id);
        assert_eq!(formatted, expected);

        // Test with system URN
        let system_urn = Urn::system("assets", "logo-template");
        let formatted = format!("{}", system_urn);
        assert_eq!(formatted, "framecast:system:assets:logo-template");
    }

    #[test]
    fn test_urn_boundary_conditions() {
        // Test boundary conditions for URN components

        // Test with minimal valid UUIDs
        let min_user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let urn = Urn::user(min_user_id);
        assert!(urn.is_user());

        // Test with maximum length system URN components
        let long_category = "a".repeat(50); // Reasonable limit
        let long_name = "b".repeat(100); // Reasonable limit
        let system_urn = Urn::system(&long_category, &long_name);
        assert!(system_urn.is_system());

        match system_urn.parse().unwrap() {
            UrnComponents::System { category, name } => {
                assert_eq!(category, long_category);
                assert_eq!(name, long_name);
            }
            _ => panic!("Expected System component"),
        }
    }
}
