//! Route definitions for Framecast API

use axum::{
    routing::{delete, get, patch, post, put},
    Router,
};

use crate::{handlers::memberships, handlers::teams, handlers::users, middleware::AppState};

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

/// Create team membership routes
pub fn membership_routes() -> Router<AppState> {
    Router::new()
        // Team invitation endpoints
        .route(
            "/v1/teams/:team_id/invite",
            post(memberships::invite_member),
        )
        .route(
            "/v1/teams/:team_id/members/:user_id",
            delete(memberships::remove_member),
        )
        .route(
            "/v1/teams/:team_id/members/:user_id/role",
            put(memberships::update_member_role),
        )
        // Global invitation acceptance endpoints
        .route(
            "/v1/invitations/:invitation_id/accept",
            put(memberships::accept_invitation),
        )
        .route(
            "/v1/invitations/:invitation_id/decline",
            put(memberships::decline_invitation),
        )
}

/// Create all API routes
pub fn create_routes() -> Router<AppState> {
    Router::new()
        .merge(user_routes())
        .merge(team_routes())
        .merge(membership_routes())
}
