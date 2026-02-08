//! JWT claims types

use serde::{Deserialize, Serialize};

/// JWT claims from Supabase
#[derive(Debug, Serialize, Deserialize)]
pub struct SupabaseClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Email
    pub email: Option<String>,
    /// Issued at
    pub iat: u64,
    /// Expires at
    pub exp: u64,
    /// Audience
    pub aud: String,
    /// Role (authenticated user)
    pub role: String,
}
