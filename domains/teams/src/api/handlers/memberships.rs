//! Team membership management API handlers
//!
//! This module implements invitation and membership management operations
//! with proper authorization and business rule enforcement.

use crate::{
    count_for_user_tx, count_members_for_team_tx, count_owned_teams_tx, count_owners_for_team_tx,
    count_pending_for_team_tx, create_invitation_tx, create_membership_tx, delete_membership_tx,
    delete_team_tx, get_membership_by_team_and_user_tx, mark_invitation_accepted_tx,
    revoke_invitation_tx, update_role_tx, upgrade_user_tier_tx, Invitation, InvitationRole,
    InvitationState, Membership, MembershipRole, MembershipWithUser, UserTier, MAX_OWNED_TEAMS,
    MAX_TEAM_MEMBERSHIPS,
};
use anyhow::Context;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use framecast_common::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::api::middleware::{AuthUser, TeamsState};

/// Request for inviting a new team member
#[derive(Debug, Deserialize, Validate)]
pub struct InviteMemberRequest {
    /// Email address of the user to invite
    #[validate(email)]
    pub email: String,

    /// Role to assign to the invited user (excludes Owner per INV-INV1)
    pub role: InvitationRole,
}

/// Request for updating a member's role
#[derive(Debug, Deserialize, Validate)]
pub struct UpdateMemberRoleRequest {
    /// New role for the member
    pub role: MembershipRole,
}

/// Query parameters for listing invitations
#[derive(Debug, Deserialize, Default)]
pub struct InvitationListQuery {
    /// Filter by invitation state (pending, accepted, declined, expired, revoked)
    pub state: Option<InvitationState>,
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
    pub team_id: Uuid,
    pub user_id: Uuid,
    pub role: MembershipRole,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Enriched user fields
    pub user_email: String,
    pub user_name: Option<String>,
    pub user_avatar_url: Option<String>,
}

impl From<MembershipWithUser> for MembershipResponse {
    fn from(m: MembershipWithUser) -> Self {
        Self {
            id: m.id,
            team_id: m.team_id,
            user_id: m.user_id,
            role: m.role,
            created_at: m.created_at,
            user_email: m.user_email,
            user_name: m.user_name,
            user_avatar_url: m.user_avatar_url,
        }
    }
}

/// List team members
///
/// **GET /v1/teams/{team_id}/members**
///
/// Returns all members of a team. Any team member can view the list.
pub async fn list_members(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
) -> Result<Json<Vec<MembershipResponse>>> {
    let user = &auth_context.0.user;

    // Check if user is a member of the team
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?;

    if membership.is_none() {
        return Err(Error::Authorization(
            "Access denied: Not a member of this team".to_string(),
        ));
    }

    // Get all members
    let members = state
        .repos
        .memberships
        .find_by_team(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to list members: {}", e)))?;

    let responses: Vec<MembershipResponse> =
        members.into_iter().map(MembershipResponse::from).collect();

    Ok(Json(responses))
}

/// Leave a team
///
/// **POST /v1/teams/{team_id}/leave**
///
/// Removes the authenticated user from the team.
/// The last owner cannot leave (INV-T2).
///
/// All checks and mutations run inside a single transaction with `FOR UPDATE`
/// row locking to prevent concurrent leave/remove operations from violating
/// the "every team has ≥ 1 owner" invariant.
pub async fn leave_team(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Begin transaction — all checks and mutations happen atomically
    let mut tx = state
        .repos
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

    // Lock all membership rows for this team (FOR UPDATE) and count members.
    // The lock prevents concurrent leave/remove from proceeding until this TX completes.
    let member_count = count_members_for_team_tx(&mut tx, team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    // Verify user is a member (rows are locked, membership state is current)
    let membership = get_membership_by_team_and_user_tx(&mut tx, team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| Error::NotFound("Not a member of this team".to_string()))?;

    if membership.role.is_owner() {
        let owner_count = count_owners_for_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owners: {}", e)))?;

        if owner_count <= 1 {
            if member_count <= 1 {
                // Last member AND last owner — check INV-U2 before auto-deleting
                if user.tier == UserTier::Creator {
                    let user_membership_count =
                        count_for_user_tx(&mut tx, user.id).await.map_err(|e| {
                            Error::Internal(format!("Failed to count user memberships: {}", e))
                        })?;
                    if user_membership_count <= 1 {
                        return Err(Error::Conflict(
                            "Cannot leave: creators must belong to at least one team (INV-U2)"
                                .to_string(),
                        ));
                    }
                }
                // Auto-delete the team (cascades memberships)
                delete_team_tx(&mut tx, team_id).await.map_err(|e| {
                    Error::Internal(format!("Failed to auto-delete empty team: {}", e))
                })?;
                tx.commit()
                    .await
                    .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;
                return Ok(StatusCode::NO_CONTENT);
            }
            // Last owner but other members exist — cannot leave (INV-T2)
            return Err(Error::Conflict(
                "Cannot leave: you are the last owner of this team".to_string(),
            ));
        }
    }

    // INV-U2: Creator must belong to at least one team
    if user.tier == UserTier::Creator {
        let user_membership_count = count_for_user_tx(&mut tx, user.id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count user memberships: {}", e)))?;
        if user_membership_count <= 1 {
            return Err(Error::Conflict(
                "Cannot leave: creators must belong to at least one team (INV-U2)".to_string(),
            ));
        }
    }

    // Remove membership
    delete_membership_tx(&mut tx, team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to leave team: {}", e)))?;

    // Auto-delete team if no members remain
    let remaining_members = count_members_for_team_tx(&mut tx, team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    if remaining_members == 0 {
        delete_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to auto-delete empty team: {}", e)))?;
    }

    // Explicit commit — drop without commit = rollback (RAII)
    tx.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Send invitation to join a team
///
/// **POST /v1/teams/{team_id}/invitations**
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
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
    Json(request): Json<InviteMemberRequest>,
) -> Result<Json<InvitationResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // INV-I7: Cannot invite yourself
    if request.email.to_lowercase() == user.email.to_lowercase() {
        return Err(Error::Validation(
            "Cannot invite yourself to a team".to_string(),
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
    if !membership.role.can_invite() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to invite members".to_string(),
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

    // Check for existing pending invitation — if one exists, revoke it and create a new one
    let existing_invitation = state
        .repos
        .invitations
        .get_by_team_and_email(team_id, &request.email)
        .await
        .map_err(|e| Error::Internal(format!("Failed to check existing invitation: {}", e)))?;

    let existing_pending_id = existing_invitation.and_then(|inv| {
        if inv.state() == InvitationState::Pending {
            Some(inv.id)
        } else {
            None
        }
    });

    let role_display = request.role.to_string();
    let recipient_email = request.email.clone();

    // Create invitation (revoke existing pending one if present, atomically)
    let invitation = Invitation::new(team_id, user.id, request.email, request.role)?;

    // Both branches use a transaction:
    // - Revoke branch: revoke old + create new atomically
    // - New branch: CARD-4 count + create atomically (prevents concurrent invitations
    //   from both seeing 49 pending and both creating, resulting in 51)
    let mut tx = state
        .repos
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

    let created_invitation = if let Some(old_id) = existing_pending_id {
        // Revoke + create in a transaction
        revoke_invitation_tx(&mut tx, old_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to revoke existing invitation: {}", e)))?;

        create_invitation_tx(&mut tx, &invitation)
            .await
            .map_err(|e| Error::Internal(format!("Failed to create invitation: {}", e)))?
    } else {
        // CARD-4: Check max pending invitations inside the TX
        let pending_count = count_pending_for_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count pending invitations: {}", e)))?;

        if pending_count >= 50 {
            return Err(Error::Conflict(
                "Team has reached maximum pending invitations limit (50)".to_string(),
            ));
        }

        create_invitation_tx(&mut tx, &invitation)
            .await
            .map_err(|e| Error::Internal(format!("Failed to create invitation: {}", e)))?
    };

    tx.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

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
/// **POST /v1/invitations/{invitation_id}/accept**
///
/// Accepts an invitation to join a team. The user must be the recipient.
/// Starter users are automatically upgraded to Creator tier.
pub async fn accept_invitation(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
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
        InvitationState::Declined => {
            return Err(Error::Conflict("Invitation has been declined".to_string()))
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

    // Business rule: Max 50 memberships per user (INV-T8)
    let user_membership_count = state
        .repos
        .memberships
        .count_for_user(user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count user memberships: {}", e)))?;

    if user_membership_count >= MAX_TEAM_MEMBERSHIPS {
        return Err(Error::Conflict(format!(
            "User has reached maximum team memberships limit ({})",
            MAX_TEAM_MEMBERSHIPS
        )));
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
        .map_err(Error::from)?;

    // Explicit commit — Drop without commit = rollback (RAII)
    transaction
        .commit()
        .await
        .context("Failed to commit invitation acceptance transaction")
        .map_err(|e| Error::Internal(e.to_string()))?;

    Ok(Json(MembershipResponse {
        id: created_membership.id,
        team_id: created_membership.team_id,
        user_id: created_membership.user_id,
        role: created_membership.role,
        created_at: created_membership.created_at,
        user_email: user.email.clone(),
        user_name: user.name.clone(),
        user_avatar_url: user.avatar_url.clone(),
    }))
}

/// Decline a team invitation
///
/// **POST /v1/invitations/{invitation_id}/decline**
///
/// Declines an invitation to join a team. The user must be the recipient.
pub async fn decline_invitation(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
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

    // Mark invitation as declined (invitee-initiated, distinct from admin revoke)
    state
        .repos
        .invitations
        .decline(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to decline invitation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// List team invitations
///
/// **GET /v1/teams/{team_id}/invitations**
///
/// Returns all invitations for a team. Only owners and admins can view.
pub async fn list_invitations(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path(team_id): Path<Uuid>,
    Query(query): Query<InvitationListQuery>,
) -> Result<Json<Vec<InvitationResponse>>> {
    let user = &auth_context.0.user;

    // Check permission: owner or admin only
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    if !membership.role.can_admin() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to view invitations".to_string(),
        ));
    }

    let invitations = state
        .repos
        .invitations
        .find_by_team(team_id, query.state)
        .await
        .map_err(|e| Error::Internal(format!("Failed to list invitations: {}", e)))?;

    let responses: Vec<InvitationResponse> = invitations
        .into_iter()
        .map(InvitationResponse::from)
        .collect();

    Ok(Json(responses))
}

/// Revoke a team invitation
///
/// **DELETE /v1/teams/{team_id}/invitations/{invitation_id}**
///
/// Revokes a pending invitation. Only owners and admins can revoke.
pub async fn revoke_invitation(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path((team_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Check permission: owner or admin only
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    if !membership.role.can_admin() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to revoke invitations".to_string(),
        ));
    }

    // Get invitation and validate it belongs to this team
    let invitation = state
        .repos
        .invitations
        .get_by_id(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get invitation: {}", e)))?
        .ok_or_else(|| Error::NotFound("Invitation not found".to_string()))?;

    if invitation.team_id != team_id {
        return Err(Error::NotFound("Invitation not found".to_string()));
    }

    // Validate invitation is pending
    if invitation.state() != InvitationState::Pending {
        return Err(Error::Conflict(
            "Only pending invitations can be revoked".to_string(),
        ));
    }

    state
        .repos
        .invitations
        .revoke(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to revoke invitation: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Resend a team invitation
///
/// **POST /v1/teams/{team_id}/invitations/{invitation_id}/resend**
///
/// Resends invitation email and extends expiration. Only owners and admins can resend.
pub async fn resend_invitation(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path((team_id, invitation_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<InvitationResponse>> {
    let user = &auth_context.0.user;

    // Check permission: owner or admin only
    let membership = state
        .repos
        .memberships
        .get_by_team_and_user(team_id, user.id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get membership: {}", e)))?
        .ok_or_else(|| {
            Error::Authorization("Access denied: Not a member of this team".to_string())
        })?;

    if !membership.role.can_admin() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to resend invitations".to_string(),
        ));
    }

    // Get invitation and validate it belongs to this team
    let invitation = state
        .repos
        .invitations
        .get_by_id(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get invitation: {}", e)))?
        .ok_or_else(|| Error::NotFound("Invitation not found".to_string()))?;

    if invitation.team_id != team_id {
        return Err(Error::NotFound("Invitation not found".to_string()));
    }

    // Validate invitation is pending
    if invitation.state() != InvitationState::Pending {
        return Err(Error::Conflict(
            "Only pending invitations can be resent".to_string(),
        ));
    }

    // Extend expiration
    let updated_invitation = state
        .repos
        .invitations
        .extend_expiration(invitation_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to extend invitation: {}", e)))?;

    // Resend email
    let team = state
        .repos
        .teams
        .get_by_id(team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get team: {}", e)))?
        .ok_or_else(|| Error::Internal("Team not found".to_string()))?;

    let inviter_name = user.name.clone().unwrap_or_else(|| user.email.clone());
    let role_display = invitation.role.to_string();

    state
        .email
        .send_team_invitation(
            &team.name,
            team_id,
            invitation_id,
            &invitation.email,
            &inviter_name,
            &role_display,
        )
        .await
        .map_err(|e| Error::Internal(format!("Failed to resend invitation email: {}", e)))?;

    Ok(Json(InvitationResponse::from(updated_invitation)))
}

/// Remove a team member
///
/// **DELETE /v1/teams/:team_id/members/:user_id**
///
/// Removes a member from the team. Only owners and admins can remove members.
/// Admins cannot remove owners.
///
/// All checks and mutations run inside a single transaction with `FOR UPDATE`
/// row locking to prevent concurrent remove/leave operations from violating
/// the "every team has ≥ 1 owner" invariant (INV-T2).
///
/// **Business Rules:**
/// - Only owner/admin can remove members (permission matrix)
/// - Admins cannot remove owners
/// - Cannot remove the last owner (INV-T2)
pub async fn remove_member(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode> {
    let user = &auth_context.0.user;

    // Cannot remove self — use POST /v1/teams/{id}/leave instead
    if member_user_id == user.id {
        return Err(Error::Validation(
            "Cannot remove yourself. Use the leave endpoint instead.".to_string(),
        ));
    }

    // Check if acting user has permission (owner or admin) — outside TX is fine
    // since the acting user's role doesn't change during the remove operation
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
    if !acting_membership.role.can_admin() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to remove members".to_string(),
        ));
    }

    // Begin transaction — lock membership rows and perform all mutations atomically
    let mut tx = state
        .repos
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

    // Lock all membership rows for this team (FOR UPDATE)
    count_members_for_team_tx(&mut tx, team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    // Get target member (rows are locked, membership state is current)
    let target_membership = get_membership_by_team_and_user_tx(&mut tx, team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get target membership: {}", e)))?
        .ok_or_else(|| Error::NotFound("Member not found in this team".to_string()))?;

    // Business rule: Admins cannot remove owners
    if !acting_membership.role.is_owner() && target_membership.role == MembershipRole::Owner {
        return Err(Error::Authorization(
            "Admins cannot remove team owners".to_string(),
        ));
    }

    // INV-U2: Cannot remove a Creator from their last team
    let target_user = state
        .repos
        .users
        .find(member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get user: {}", e)))?
        .ok_or_else(|| Error::NotFound("User not found".to_string()))?;

    if target_user.tier == UserTier::Creator {
        let target_membership_count = count_for_user_tx(&mut tx, member_user_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count user memberships: {}", e)))?;
        if target_membership_count <= 1 {
            return Err(Error::Conflict(
                "Cannot remove: creators must belong to at least one team (INV-U2)".to_string(),
            ));
        }
    }

    // Business rule: Cannot remove the last owner (INV-T2)
    if target_membership.role == MembershipRole::Owner {
        let owner_count = count_owners_for_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owners: {}", e)))?;

        if owner_count <= 1 {
            return Err(Error::Conflict(
                "Cannot remove the last owner from the team".to_string(),
            ));
        }
    }

    // Remove membership
    delete_membership_tx(&mut tx, team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to remove member: {}", e)))?;

    // Auto-delete team if no members remain
    let remaining_members = count_members_for_team_tx(&mut tx, team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    if remaining_members == 0 {
        delete_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to auto-delete empty team: {}", e)))?;
    }

    tx.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    Ok(StatusCode::NO_CONTENT)
}

/// Update a team member's role
///
/// **PATCH /v1/teams/:team_id/members/:user_id**
///
/// Updates a member's role in the team. Only owners can update roles.
/// Admins can update roles but cannot promote to owner.
///
/// All invariant checks (INV-T2, INV-T7) and the role mutation run inside a
/// single transaction with `FOR UPDATE` locking on the team's membership rows.
/// This prevents concurrent demotions from leaving a team with 0 owners, and
/// concurrent promotions from exceeding MAX_OWNED_TEAMS.
///
/// **Business Rules:**
/// - Only owner/admin can update roles (permission matrix)
/// - Admins cannot promote to owner
/// - Cannot demote the last owner (INV-T2)
/// - Promoting to owner must not exceed MAX_OWNED_TEAMS (INV-T7)
pub async fn update_member_role(
    auth_context: AuthUser,
    State(state): State<TeamsState>,
    Path((team_id, member_user_id)): Path<(Uuid, Uuid)>,
    Json(request): Json<UpdateMemberRoleRequest>,
) -> Result<Json<MembershipResponse>> {
    // Validate request
    request
        .validate()
        .map_err(|e| Error::Validation(format!("Validation failed: {}", e)))?;

    let user = &auth_context.0.user;

    // Cannot change own role
    if member_user_id == user.id {
        return Err(Error::Validation("Cannot change your own role".to_string()));
    }

    // Check if acting user has permission (owner or admin) — outside TX is fine
    // since the acting user's role is not modified by this operation
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
    if !acting_membership.role.can_admin() {
        return Err(Error::Authorization(
            "Access denied: Must be owner or admin to update member roles".to_string(),
        ));
    }

    // Business rule: Admins cannot promote to owner
    if acting_membership.role == MembershipRole::Admin && request.role == MembershipRole::Owner {
        return Err(Error::Authorization(
            "Admins cannot promote members to owner role".to_string(),
        ));
    }

    // Begin transaction — lock membership rows and perform all checks + mutation atomically
    let mut tx = state
        .repos
        .begin()
        .await
        .map_err(|e| Error::Internal(format!("Failed to begin transaction: {}", e)))?;

    // Lock all membership rows for this team (FOR UPDATE) to serialise with
    // concurrent role changes, leave, and remove operations
    count_members_for_team_tx(&mut tx, team_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to count team members: {}", e)))?;

    // Get target member (rows are locked, membership state is current)
    let target_membership = get_membership_by_team_and_user_tx(&mut tx, team_id, member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get target membership: {}", e)))?
        .ok_or_else(|| Error::NotFound("Member not found in this team".to_string()))?;

    // Business rule: Promoting to owner must not exceed MAX_OWNED_TEAMS (INV-T7)
    if request.role == MembershipRole::Owner && target_membership.role != MembershipRole::Owner {
        let owned_teams_count = count_owned_teams_tx(&mut tx, member_user_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owned teams: {}", e)))?;

        if owned_teams_count >= MAX_OWNED_TEAMS {
            return Err(Error::Conflict(format!(
                "User cannot own more than {} teams",
                MAX_OWNED_TEAMS
            )));
        }
    }

    // Business rule: Cannot demote the last owner (INV-T2)
    if target_membership.role == MembershipRole::Owner && request.role != MembershipRole::Owner {
        let owner_count = count_owners_for_team_tx(&mut tx, team_id)
            .await
            .map_err(|e| Error::Internal(format!("Failed to count owners: {}", e)))?;

        if owner_count <= 1 {
            return Err(Error::Conflict(
                "Cannot demote the last owner from the team".to_string(),
            ));
        }
    }

    // Update role in database
    let updated_membership = update_role_tx(&mut tx, team_id, member_user_id, request.role)
        .await
        .map_err(|e| Error::Internal(format!("Failed to update member role: {}", e)))?;

    tx.commit()
        .await
        .map_err(|e| Error::Internal(format!("Failed to commit transaction: {}", e)))?;

    // Fetch target user info for enriched response (outside TX — immutable user data)
    let target_user = state
        .repos
        .users
        .find(member_user_id)
        .await
        .map_err(|e| Error::Internal(format!("Failed to get user: {}", e)))?
        .ok_or_else(|| Error::Internal("User not found for membership".to_string()))?;

    Ok(Json(MembershipResponse {
        id: updated_membership.id,
        team_id: updated_membership.team_id,
        user_id: updated_membership.user_id,
        role: updated_membership.role,
        created_at: updated_membership.created_at,
        user_email: target_user.email,
        user_name: target_user.name,
        user_avatar_url: target_user.avatar_url,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invite_member_request_validation() {
        // Valid request
        let valid_request = InviteMemberRequest {
            email: "test@example.com".to_string(),
            role: InvitationRole::Member,
        };
        assert!(valid_request.validate().is_ok());

        // Invalid email
        let invalid_email = InviteMemberRequest {
            email: "not-an-email".to_string(),
            role: InvitationRole::Member,
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
        let team_id = Uuid::new_v4();
        let membership_response = MembershipResponse {
            id: Uuid::new_v4(),
            team_id,
            user_id: Uuid::new_v4(),
            role: MembershipRole::Admin,
            created_at: chrono::Utc::now(),
            user_email: "member@example.com".to_string(),
            user_name: Some("Test User".to_string()),
            user_avatar_url: None,
        };

        let json = serde_json::to_string(&membership_response).unwrap();
        assert!(json.contains("admin"));
        assert!(json.contains("member@example.com"));
        assert!(json.contains("Test User"));
        assert!(json.contains(&team_id.to_string()));
    }
}
