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
        .route("/v1/teams", get(teams::list_teams))
        .route("/v1/teams", post(teams::create_team))
        .route("/v1/teams/{id}", get(teams::get_team))
        .route("/v1/teams/{id}", patch(teams::update_team))
        .route("/v1/teams/{id}", delete(teams::delete_team))
}

/// Create team membership routes
pub fn membership_routes() -> Router<AppState> {
    Router::new()
        // Team member endpoints
        .route(
            "/v1/teams/{team_id}/members",
            get(memberships::list_members),
        )
        .route(
            "/v1/teams/{team_id}/members/{user_id}",
            delete(memberships::remove_member).patch(memberships::update_member_role),
        )
        // Team invitation endpoints
        .route(
            "/v1/teams/{team_id}/invitations",
            get(memberships::list_invitations).post(memberships::invite_member),
        )
        .route(
            "/v1/teams/{team_id}/invitations/{invitation_id}",
            delete(memberships::revoke_invitation),
        )
        .route(
            "/v1/teams/{team_id}/invitations/{invitation_id}/resend",
            post(memberships::resend_invitation),
        )
        // Leave team
        .route("/v1/teams/{team_id}/leave", post(memberships::leave_team))
        // Global invitation acceptance endpoints
        .route(
            "/v1/invitations/{invitation_id}/accept",
            post(memberships::accept_invitation),
        )
        .route(
            "/v1/invitations/{invitation_id}/decline",
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
