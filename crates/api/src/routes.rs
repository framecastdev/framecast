//! Route definitions for Framecast API

use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use crate::{handlers::teams, handlers::users, middleware::AppState};

/// Create user management routes
pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/account", get(users::get_profile))
        .route("/v1/account", patch(users::update_profile))
        .route("/v1/account/upgrade", post(users::upgrade_tier))
}

/// Create team management routes
pub fn team_routes() -> Router<AppState> {
    Router::new()
        .route("/v1/teams", post(teams::create_team))
        .route("/v1/teams/:id", get(teams::get_team))
        .route("/v1/teams/:id", patch(teams::update_team))
        .route("/v1/teams/:id", delete(teams::delete_team))
}

/// Create all API routes
pub fn create_routes() -> Router<AppState> {
    Router::new().merge(user_routes()).merge(team_routes())
}
