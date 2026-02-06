//! Team membership management API handlers
//!
//! This module implements invitation and membership management operations
//! with proper authorization and business rule enforcement.

use anyhow::Context;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use framecast_common::{Error, Result};
use framecast_db::repositories::{
    create_membership_tx, mark_invitation_accepted_tx, upgrade_user_tier_tx,
};
use framecast_domain::entities::{
    Invitation, InvitationRole, InvitationState, Membership, MembershipRole, UserTier,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::middleware::{AppState, AuthUser};

/// Request for inviting a new team member
#[derive(Debug, Deserialize, Validate)]
pub struct InviteMemberRequest {
    /// Email address of the user to invite
    #[validate(email)]
    pub email: String,

    /// Role to assign to the invited user
    pub role: MembershipRole,
}

/// Request for updating a member's role
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMemberRoleRequest {
    /// New role for the member
    pub role: MembershipRole,
}

/// Response for invitation operations
#[derive(Debug, Serialize)]
pub struct InvitationResponse {
    pub id: Uuid,
    pub team_id: Uuid,
    pub email: String,
    pub role: InvitationRole,
    pub state: InvitationState,
    pub invited_by: Uuid,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Invitation> for InvitationResponse {
    fn from(invitation: Invitation) -> Self {
        Self {
            id: invitation.id,
            team_id: invitation.team_id,
            email: invitation.email.clone(),
            role: invitation.role.clone(),
            state: invitation.state(),
            invited_by: invitation.invited_by,
            expires_at: invitation.expires_at,
            created_at: invitation.created_at,
        }
    }
}

/// Response for membership operations
#[derive(Debug, Serialize)]
pub struct MembershipResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub role: MembershipRole,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<Membership> for MembershipResponse {
    fn from(membership: Membership) -> Self {
        Self {
            id: membership.id,
            user_id: membership.user_id,
            role: membership.role,
            created_at: membership.created_at,
        }
    }
}

/// Send invitation to join a team
///
/// **POST /v1/teams/{team_id}/invite**
///
/// Sends an invitation email to a user to join the team.
/// Only team owners and admins can send invitations.
///
/// **Business Rules:**
/// - Only owner/admin can send invitations (permission matrix)
/// - Cannot invite to owner role (INV-INV1)
/// - Cannot invite already existing members
/// - Max 50 pending invitations per team (CARD-4)
pub async fn invite_member(
    auth_context: AuthUser,
    State(state): State<AppState>,
    Path(team_id): Path<Uuid>,
    Json(request): Json<InviteMemberRequest>,
) -> Result<Json<InvitationResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // Validate role - cannot invite owners
    if request.role == MembershipRole::Owner {
        return Err(Error::Validation(
            "Cannot invite users to owner role".to_string(),
        ));
    }

    // Check if team exists and user has permission (owner or admin)
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    // Permission check: only owners and admins can invite
    if !matches!(
        membership.role,
        MembershipRole::Owner | MembershipRole::Admin
    ) {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to invite members".to_string(),
        ));
    }

    // Business rule: Admins cannot promote to owner
    if membership.role == MembershipRole::Admin && request.role == MembershipRole::Owner {
        return Err(Error::Authorization(
            "Admins cannot invite users to owner role".to_string(),
        ));
    }

    // Check if user is already a member
    let existing_membership = state
        .repos
        .users
        .find_by_email(&request.email)
        .await
        .map_err(|e| Error::Internal(format!("Failed to find user: {}", e)))?;

    if let Some(existing_user) = existing_membership {
        let existing_membership = state
            .repos
            .memberships
            .get_by_team_and_user(team_id, existing_user.id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to check membership: {}", e)))?;

        if existing_membership.is_some() {
            return Err(Error::Conflict(
                "User is already a member of this team".to_string(),
            ));
        }
    }

    // Check for existing pending invitation
    let existing_invitation = state
        .repos
        .invitations
        .get_by_team_and_email(team_id, &request.email)
        .await
        .map_err(|e| Error::Internal(format!("Failed to check existing invitation: {}", e)))?;

    if let Some(existing) = existing_invitation {
        if existing.state() == InvitationState::Pending {
            return Err(Error::Conflict(
                "User already has a pending invitation to this team".to_string(),
            ));
        }
    }

    // Business rule: Check max pending invitations (CARD-4)
    let pending_count = state
        .repos
        .invitations
        .count_pending_for_team(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count pending invitations: {}", e)))?;

    if pending_count >= 50 {
        return Err(Error::Conflict(
            "Team has reached maximum pending invitations limit (50)".to_string(),
        ));
    }

    // Convert MembershipRole to InvitationRole (validates that Owner cannot be invited)
    let invitation_role = InvitationRole::try_from(request.role)?;
    let role_display = invitation_role.to_string();
    let recipient_email = request.email.clone();

    // Create invitation
    let invitation = Invitation::new(team_id, user.id, request.email, invitation_role)?;

    let created_invitation = state
        .repos
        .invitations
        .create(&invitation)
        .await
        .map_err(|e| Error::Internal(format!("Failed to create invitation: {}", e)))?;

    // Send invitation email
    let team = state
        .repos
        .teams
        .get_by_id(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get team: {}", e)))?
        .ok_or_else(|| Error::Internal("Team not found after creating invitation".to_string()))?;
    let inviter_name = user.name.clone().unwrap_or_else(|| user.email.clone());
    state
        .email
        .send_team_invitation(
            &team.name,
            team_id,
            created_invitation.id,
            &recipient_email,
            &inviter_name,
            &role_display,
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to send invitation email: {}", e)))?;

    Ok(Json(InvitationResponse::from(created_invitation)))
}

/// Accept a team invitation
///
/// **PUT /v1/invitations/{invitation_id}/accept**
///
/// Accepts an invitation to join a team. The user must be the recipient.
/// Starter users are automatically upgraded to Creator tier.
pub async fn accept_invitation(
    auth_context: AuthUser,
    State(state): State<AppState>,
    Path(invitation_id): Path<Uuid>,
) -> Result<Json<MembershipResponse>> {
    let user = &auth_context.0.user;

    // Get invitation
    let invitation = state
        .repos
        .invitations
        .get_by_id(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get invitation: {}", e)))?
        .ok_or_else(|| Error::NotFound("Invitation not found".to_string()))?;

    // Get team_id from the invitation
    let team_id = invitation.team_id;

    // Validate invitation is for this user
    if invitation.email != user.email {
        return Err(Error::Authorization(
            "Access denied: Invitation is for a different email".to_string(),
        ));
    }

    // Validate invitation state
    match invitation.state() {
        InvitationState::Pending => {}
        InvitationState::Accepted => {
            return Err(Error::Conflict("Invitation already accepted".to_string()))
        }
        InvitationState::Expired => {
            return Err(Error::Conflict("Invitation has expired".to_string()))
        }
        InvitationState::Revoked => {
            return Err(Error::Conflict("Invitation has been revoked".to_string()))
        }
    }

    // Check if user is already a member
    let existing_membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to check membership: {}", e)))?;

    if existing_membership.is_some() {
        return Err(Error::Conflict(
            "User is already a member of this team".to_string(),
        ));
    }

    // Create membership (convert InvitationRole to MembershipRole)
    let membership = Membership::new(team_id, user.id, invitation.role.to_membership_role());

    // Begin transaction — all mutations happen atomically (Zero2Prod Ch.7 pattern)
    let mut transaction = state
        .repos
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool")
        .map_err(|e| Error::Internal(e.to_string()))?;

    // Auto-upgrade Starter → Creator if needed
    if user.tier == UserTier::Starter {
        upgrade_user_tier_tx(&mut transaction, user.id, UserTier::Creator)
            .await
            .context("Failed to upgrade user tier to Creator")
            .map_err(|e| Error::Internal(e.to_string()))?;
    }

    let created_membership = create_membership_tx(&mut transaction, &membership)
        .await
        .context("Failed to create team membership")
        .map_err(|e| Error::Internal(e.to_string()))?;

    mark_invitation_accepted_tx(&mut transaction, invitation_id)
        .await
        .context("Failed to mark invitation as accepted")
        .map_err(|e| Error::Internal(e.to_string()))?;

    // Explicit commit — Drop without commit = rollback (RAII)
    transaction
        .commit()
        .await
        .context("Failed to commit invitation acceptance transaction")
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(MembershipResponse::from(created_membership)))
}

/// Decline a team invitation
///
/// **PUT /v1/invitations/{invitation_id}/decline**
///
/// Declines an invitation to join a team. The user must be the recipient.
pub async fn decline_invitation(
    auth_context: AuthUser,
    State(state): State<AppState>,
    Path(invitation_id): Path<Uuid>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Get invitation
    let invitation = state
        .repos
        .invitations
        .get_by_id(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get invitation: {}", e)))?
        .ok_or_else(|| Error::NotFound("Invitation not found".to_string()))?;

    // Validate invitation is for this user
    if invitation.email != user.email {
        return Err(Error::Authorization(
            "Access denied: Invitation is for a different email".to_string(),
        ));
    }

    // Validate invitation state
    if invitation.state() != InvitationState::Pending {
        return Err(Error::Conflict("Invitation is not pending".to_string()));
    }

    // Mark invitation as revoked (using revoke since we don't have a "declined" state)
    state
        .repos
        .invitations
        .revoke(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to decline invitation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Remove a team member
///
/// **DELETE /v1/teams/:team_id/members/:user_id**
///
/// Removes a member from the team. Only owners and admins can remove members.
/// Admins cannot remove owners.
///
/// **Business Rules:**
/// - Only owner/admin can remove members (permission matrix)
/// - Admins cannot remove owners
/// - Cannot remove the last owner (INV-T2)
pub async fn remove_member(
    auth_context: AuthUser,
    State(state): State<AppState>,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Check if acting user has permission (owner or admin)
    let acting_membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    // Permission check: only owners and admins can remove members
    if !matches!(
        acting_membership.role,
        MembershipRole::Owner | MembershipRole::Admin
    ) {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to remove members".to_string(),
        ));
    }

    // Get target member
    let target_membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get target membership: {}", e)))?
        .ok_or_else(|| Error::NotFound("Member not found in this team".to_string()))?;

    // Business rule: Admins cannot remove owners
    if acting_membership.role == MembershipRole::Admin
        && target_membership.role == MembershipRole::Owner
    {
        return Err(Error::Authorization(
            "Admins cannot remove team owners".to_string(),
        ));
    }

    // Business rule: Cannot remove the last owner (INV-T2)
    if target_membership.role == MembershipRole::Owner {
        let owner_count = state
            .repos
            .memberships
            .count_owners(team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owners: {}", e)))?;

        if owner_count <= 1 {
            return Err(Error::Conflict(
                "Cannot remove the last owner from the team".to_string(),
            ));
        }
    }

    // Remove membership
    state
        .repos
        .memberships
        .delete(team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to remove member: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Update a team member's role
///
/// **PUT /v1/teams/:team_id/members/:user_id/role**
///
/// Updates a member's role in the team. Only owners can update roles.
/// Admins can update roles but cannot promote to owner.
///
/// **Business Rules:**
/// - Only owner/admin can update roles (permission matrix)
/// - Admins cannot promote to owner
/// - Cannot demote the last owner (INV-T2)
pub async fn update_member_role(
    auth_context: AuthUser,
    State(state): State<AppState>,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMemberRoleRequest>,
) -> Result<Json<MembershipResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // Check if acting user has permission (owner or admin)
    let acting_membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    // Permission check: only owners and admins can update roles
    if !matches!(
        acting_membership.role,
        MembershipRole::Owner | MembershipRole::Admin
    ) {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to update member roles".to_string(),
        ));
    }

    // Get target member
    let target_membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get target membership: {}", e)))?
        .ok_or_else(|| Error::NotFound("Member not found in this team".to_string()))?;

    // Business rule: Admins cannot promote to owner
    if acting_membership.role == MembershipRole::Admin && request.role == MembershipRole::Owner {
        return Err(Error::Authorization(
            "Admins cannot promote members to owner role".to_string(),
        ));
    }

    // Business rule: Cannot demote the last owner (INV-T2)
    if target_membership.role == MembershipRole::Owner && request.role != MembershipRole::Owner {
        let owner_count = state
            .repos
            .memberships
            .count_owners(team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owners: {}", e)))?;

        if owner_count <= 1 {
            return Err(Error::Conflict(
                "Cannot demote the last owner from the team".to_string(),
            ));
        }
    }

    // Update role in database
    let updated_membership = state
        .repos
        .memberships
        .update_role(team_id, member_user_id, request.role)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update member role: {}", e)))?;

    Ok(Json(MembershipResponse::from(updated_membership)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_member_request_validation() {
        // Valid request
        let valid_request = InviteMemberRequest {
            email: "test@example.com".to_string(),
            role: MembershipRole::Member,
        };
        assert!(valid_request.validate().is_ok());

        // Invalid email
        let invalid_email = InviteMemberRequest {
            email: "not-an-email".to_string(),
            role: MembershipRole::Member,
        };
        assert!(invalid_email.validate().is_err());
    }

    #[test]
    fn test_update_member_role_request_validation() {
        let valid_request = UpdateMemberRoleRequest {
            role: MembershipRole::Admin,
        };
        assert!(valid_request.validate().is_ok());
    }

    #[test]
    fn test_invitation_response_serialization() {
        let invitation_response = InvitationResponse {
            id: Uuid::new_v4(),
            team_id: Uuid::new_v4(),
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
            state: InvitationState::Pending,
            invited_by: Uuid::new_v4(),
            expires_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&invitation_response).unwrap();
        assert!(json.contains("test@example.com"));
        assert!(json.contains("pending"));
    }

    #[test]
    fn test_membership_response_serialization() {
        let membership_response = MembershipResponse {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role: MembershipRole::Admin,
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&membership_response).unwrap();
        assert!(json.contains("admin"));
    }
}
