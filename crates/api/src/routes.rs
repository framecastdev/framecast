//! Route definitions for Framecast API

use axum::{
    routing::{get, patch, post},
    Router,
};

use crate::{handlers::users, middleware::AppState};

/// Create user management routes
pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/account", get(users::get_profile))
        .route("/v1/account", patch(users::update_profile))
        .route("/v1/account/upgrade", post(users::upgrade_tier))
}

/// Create all API routes
pub fn create_routes() -> Router<AppState> {
    Router::new().merge(user_routes())
}
