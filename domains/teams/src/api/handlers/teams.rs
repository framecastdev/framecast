//! Team management API handlers
//!
//! This module implements team CRUD operations with proper authorization
//! and business rule enforcement as defined in the API specification.

use crate::{MembershipRole, Team, UserTier, MAX_OWNED_TEAMS};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use framecast_common::{Error, Result, Urn};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::{AuthUser, TeamsState};

/// Request for creating a new team
#[derive(Debug, Deserialize, Validate)]
pub struct CreateTeamRequest {
    /// Team display name (3-100 chars)
    #[validate(length(min = 3, max = 100))]
    pub name: String,

    /// Optional team slug (if not provided, generated from name)
    #[validate(
        length(min = 1, max = 50),
        custom(function = "validate_slug_format", message = "Invalid slug format")
    )]
    pub slug: Option<String>,

    /// Initial credit allocation for the team
    #[validate(range(min = 0))]
    pub initial_credits: Option<i32>,
}

/// Custom validation function for slug format
fn validate_slug_format(slug: &str) -> std::result::Result<(), validator::ValidationError> {
    crate::Team::validate_slug(slug).map_err(|_| validator::ValidationError::new("invalid_format"))
}

/// Request for updating a team
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateTeamRequest {
    /// Updated team display name
    #[validate(length(min = 3, max = 100))]
    pub name: Option<String>,

    /// Team settings (JSON object)
    pub settings: Option<serde_json::Value>,
}

/// Team response for API operations
#[derive(Debug, Serialize)]
pub struct TeamResponse {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub credits: i32,
    pub ephemeral_storage_bytes: i64,
    pub settings: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// User's role in this team (if they're a member)
    pub user_role: Option<MembershipRole>,
    /// User's URN for this team
    pub user_urn: Option<String>,
}

impl TeamResponse {
    /// Convert a Team entity to response format with user context
    pub fn from_team_with_context(
        team: Team,
        user_role: Option<MembershipRole>,
        user_id: Uuid,
    ) -> Self {
        let user_urn = user_role
            .as_ref()
            .map(|_| Urn::team_user(team.id, user_id).to_string());

        Self {
            id: team.id,
            name: team.name,
            slug: team.slug,
            credits: team.credits,
            ephemeral_storage_bytes: team.ephemeral_storage_bytes,
            settings: serde_json::to_value(team.settings.0).unwrap_or_default(),
            created_at: team.created_at,
            updated_at: team.updated_at,
            user_role,
            user_urn,
        }
    }
}

/// List teams for the current user
///
/// **GET /v1/teams**
///
/// Returns all teams the authenticated user is a member of, with their role.
pub async fn list_teams(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
) -> Result<Json<Vec<TeamResponse>>> {
    let user = &auth_context.0.user;

    let teams_with_roles = state
        .repos
        .teams
        .find_by_user(user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to list teams: {}", e)))?;

    let responses: Vec<TeamResponse> = teams_with_roles
        .into_iter()
        .map(|(team, role)| TeamResponse::from_team_with_context(team, Some(role), user.id))
        .collect();

    Ok(Json(responses))
}

/// Create a new team
///
/// **POST /v1/teams**
///
/// Creates a new team with the authenticated user as owner.
/// Only creator tier users can create teams.
///
/// **Business Rules:**
/// - User must be creator tier (INV-U3: starters can't have memberships)
/// - User can't own more than 10 teams (INV-T7)
/// - Team slug must be unique (INV-T3)
/// - Team gets at least one owner (INV-T2) - the creator
pub async fn create_team(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Json(request): Json<CreateTeamRequest>,
) -> Result<(StatusCode, Json<TeamResponse>)> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // Business Rule: Only creator tier users can create teams (INV-M4)
    if user.tier != UserTier::Creator {
        return Err(Error::Authorization(
            "Only creator tier users can create teams".to_string(),
        ));
    }

    // Business Rule: Check max owned teams limit (INV-T7)
    let owned_teams_count = state
        .repos
        .memberships
        .count_owned_teams(user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count owned teams: {}", e)))?;

    if owned_teams_count >= MAX_OWNED_TEAMS {
        return Err(Error::Conflict(format!(
            "Cannot own more than {} teams",
            MAX_OWNED_TEAMS
        )));
    }

    // Create team (handles slug generation from name if not provided)
    let mut team = Team::new(request.name, request.slug)?;

    // Check slug uniqueness (INV-T3)
    if state
        .repos
        .teams
        .get_by_slug(&team.slug)
        .await
        .map_err(|e| Error::Internal(format!("Failed to check slug uniqueness: {}", e)))?
        .is_some()
    {
        return Err(Error::Conflict(format!(
            "Team slug '{}' already exists",
            team.slug
        )));
    }

    // Set initial credits if provided
    if let Some(credits) = request.initial_credits {
        team.credits = credits;
    }

    // Atomically create team + owner membership in a transaction (INV-T1, INV-T2)
    let mut tx = state
        .repos
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

    let created_team = crate::create_team_tx(&mut tx, &team)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create team: {}", e)))?;

    let membership = crate::Membership::new(created_team.id, user.id, MembershipRole::Owner);

    crate::create_membership_tx(&mut tx, &membership)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create membership: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    // Return response with user context
    let response =
        TeamResponse::from_team_with_context(created_team, Some(MembershipRole::Owner), user.id);

    Ok((StatusCode::CREATED, Json(response)))
}

/// Get team details
///
/// **GET /v1/teams/:id**
///
/// Retrieves team details if the user has access.
/// User must be a member of the team.
pub async fn get_team(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
) -> Result<Json<TeamResponse>> {
    let user = &auth_context.0.user;

    // Get team
    let team = state
        .repos
        .teams
        .get_by_id(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get team: {}", e)))?
        .ok_or_else(|| Error::NotFound("Team not found".to_string()))?;

    // Check if user has membership (required for access)
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?;

    let user_role = membership.map(|m| m.role);

    // User must be a member to access team
    if user_role.is_none() {
        return Err(Error::Authorization(
            "Access denied: Not a member of this team".to_string(),
        ));
    }

    // Return response with user context
    let response = TeamResponse::from_team_with_context(team, user_role, user.id);

    Ok(Json(response))
}

/// Update team settings
///
/// **PATCH /v1/teams/:id**
///
/// Updates team information. User must be owner or admin.
pub async fn update_team(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
    Json(request): Json<UpdateTeamRequest>,
) -> Result<Json<TeamResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // Get team
    let mut team = state
        .repos
        .teams
        .get_by_id(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get team: {}", e)))?
        .ok_or_else(|| Error::NotFound("Team not found".to_string()))?;

    // Check user's permission (must be owner or admin)
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    // Permission check: owner or admin can edit team settings
    if !membership.role.can_modify_team() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to update team".to_string(),
        ));
    }

    // Apply updates
    if let Some(name) = request.name {
        team.name = name;
    }

    if let Some(settings) = request.settings {
        // Convert JSON value to HashMap
        if let Ok(settings_map) =
            serde_json::from_value::<HashMap<String, serde_json::Value>>(settings)
        {
            team.settings = sqlx::types::Json(settings_map);
        }
    }

    // Update in database
    let updated_team = state
        .repos
        .teams
        .update(&team)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update team: {}", e)))?;

    // Return response with user context
    let response =
        TeamResponse::from_team_with_context(updated_team, Some(membership.role), user.id);

    Ok(Json(response))
}

/// Delete a team
///
/// **DELETE /v1/teams/:id**
///
/// Deletes a team. Only the owner can delete, and only if there are no active jobs.
///
/// **Business Rules:**
/// - Only team owner can delete (not admin)
/// - Cannot delete if there are active jobs
/// - Must handle membership cleanup (INV-T1, INV-T2 become irrelevant)
pub async fn delete_team(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Get team (just verify it exists, don't need the data)
    let _team = state
        .repos
        .teams
        .get_by_id(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get team: {}", e)))?
        .ok_or_else(|| Error::NotFound("Team not found".to_string()))?;

    // Check user's permission (must be owner)
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    // Permission check: only owners can delete teams
    if !membership.role.is_owner() {
        return Err(Error::Authorization(
            "Access denied: Only team owners can delete teams".to_string(),
        ));
    }

    // Business rule: Team must have no other members before deletion
    let member_count = state
        .repos
        .memberships
        .count_for_team(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    if member_count > 1 {
        return Err(Error::Conflict(
            "Team must have no other members before deletion".to_string(),
        ));
    }

    // Business rule: Cannot delete if there are active jobs
    let active_jobs_count = state
        .repos
        .teams
        .count_active_jobs_for_team(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count active jobs: {}", e)))?;

    if active_jobs_count > 0 {
        return Err(Error::Conflict(
            "Cannot delete team with active jobs".to_string(),
        ));
    }

    // Delete team (cascades to memberships via database constraints)
    state
        .repos
        .teams
        .delete(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to delete team: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_team_request_validation() {
        // Valid request
        let valid_request = CreateTeamRequest {
            name: "Test Team".to_string(),
            slug: Some("test-team".to_string()),
            initial_credits: Some(1000),
        };
        assert!(valid_request.validate().is_ok());

        // Invalid name (too short)
        let invalid_name = CreateTeamRequest {
            name: "AB".to_string(),
            slug: None,
            initial_credits: None,
        };
        assert!(invalid_name.validate().is_err());

        // Invalid slug format
        let invalid_slug = CreateTeamRequest {
            name: "Test Team".to_string(),
            slug: Some("-invalid-".to_string()),
            initial_credits: None,
        };
        assert!(invalid_slug.validate().is_err());

        // Invalid negative credits
        let invalid_credits = CreateTeamRequest {
            name: "Test Team".to_string(),
            slug: None,
            initial_credits: Some(-100),
        };
        assert!(invalid_credits.validate().is_err());
    }

    #[test]
    fn test_update_team_request_validation() {
        // Valid request
        let valid_request = UpdateTeamRequest {
            name: Some("Updated Team".to_string()),
            settings: Some(serde_json::json!({"notifications": true})),
        };
        assert!(valid_request.validate().is_ok());

        // Invalid name (too short)
        let invalid_name = UpdateTeamRequest {
            name: Some("AB".to_string()),
            settings: None,
        };
        assert!(invalid_name.validate().is_err());
    }

    #[test]
    fn test_team_response_serialization() {
        let team_response = TeamResponse {
            id: Uuid::new_v4(),
            name: "Test Team".to_string(),
            slug: "test-team".to_string(),
            credits: 1000,
            ephemeral_storage_bytes: 0,
            settings: serde_json::json!({}),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            user_role: Some(MembershipRole::Owner),
            user_urn: Some("framecast:tm_123:usr_456".to_string()),
        };

        let json = serde_json::to_string(&team_response).unwrap();
        assert!(json.contains("test-team"));
        assert!(json.contains("owner"));
    }
}
