//! Authentication configuration

/// Authentication configuration
#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub issuer: Option<String>,
    pub audience: Option<String>,
}
