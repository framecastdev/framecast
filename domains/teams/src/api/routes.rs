//! Route definitions for Teams domain API

use axum::{
    routing::{delete, get, patch, post},
    Router,
};

use super::handlers::{api_keys, auth, memberships, teams, users};
use super::middleware::TeamsState;

/// Create user management routes
fn user_routes() -> Router<TeamsState> {
    Router::new()
        .route(
            "/v1/account",
            get(users::get_profile)
                .patch(users::update_profile)
                .delete(users::delete_account),
        )
        .route("/v1/account/upgrade", post(users::upgrade_tier))
}

/// Create team management routes
fn team_routes() -> Router<TeamsState> {
    Router::new()
        .route("/v1/teams", get(teams::list_teams))
        .route("/v1/teams", post(teams::create_team))
        .route("/v1/teams/{id}", get(teams::get_team))
        .route("/v1/teams/{id}", patch(teams::update_team))
        .route("/v1/teams/{id}", delete(teams::delete_team))
}

/// Create team membership routes
fn membership_routes() -> Router<TeamsState> {
    Router::new()
        .route(
            "/v1/teams/{team_id}/members",
            get(memberships::list_members),
        )
        .route(
            "/v1/teams/{team_id}/members/{user_id}",
            delete(memberships::remove_member).patch(memberships::update_member_role),
        )
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
        .route("/v1/teams/{team_id}/leave", post(memberships::leave_team))
        .route(
            "/v1/invitations/{invitation_id}/accept",
            post(memberships::accept_invitation),
        )
        .route(
            "/v1/invitations/{invitation_id}/decline",
            post(memberships::decline_invitation),
        )
}

/// Create API key management routes
fn api_key_routes() -> Router<TeamsState> {
    Router::new()
        .route(
            "/v1/auth/keys",
            get(api_keys::list_api_keys).post(api_keys::create_api_key),
        )
        .route(
            "/v1/auth/keys/{id}",
            get(api_keys::get_api_key)
                .patch(api_keys::update_api_key)
                .delete(api_keys::revoke_api_key),
        )
}

/// Create auth introspection routes
fn auth_routes() -> Router<TeamsState> {
    Router::new().route("/v1/auth/whoami", get(auth::whoami))
}

/// Create all Teams domain API routes
pub fn routes() -> Router<TeamsState> {
    Router::new()
        .merge(user_routes())
        .merge(team_routes())
        .merge(membership_routes())
        .merge(api_key_routes())
        .merge(auth_routes())
}
