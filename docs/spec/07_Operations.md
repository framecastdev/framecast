# Section 8: Operations

**Note:** This section defines all API operations with pre/post conditions.

---

## 8.1 User Operations

```
Operation: get_profile(user_id: UUID) → User
  Pre:  ∃ u ∈ User : u.id = user_id
  Post: Returns user profile
        Fields returned: id, email, name, avatar_url, tier, credits,
          ephemeral_storage_bytes, upgraded_at, created_at, updated_at

  Notes:
    - Available to both Starter and Creator tiers
    - Authenticated via JWT (Supabase Auth) or API key

Operation: signup(email: String, password: String) → User
  Pre:  ∄ u ∈ User : u.email = email
        ∧ valid_email(email)
        ∧ |email| ≤ 255
  Post: User created with:
          id = supabase_auth_user_id()
          email = email
          tier = 'starter'
          credits = 0
          ephemeral_storage_bytes = 0
          created_at = now()
          updated_at = now()

  Notes:
    - Authentication handled by Supabase Auth
    - Application creates User record on first authenticated request
    - New users start as Starter tier with no team memberships (INV-U3)

Operation: update_profile(user_id: UUID, updates: ProfileUpdates) → User
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ (updates.name IS NULL ∨ |updates.name| ≤ 100)
        ∧ (updates.avatar_url IS NULL ∨ valid_url(updates.avatar_url))
  Post: User updated with provided fields
        ∧ u.updated_at = now()

  ProfileUpdates:
    name?: String (max 100)
    avatar_url?: URL

  Notes:
    - Cannot update email (managed by Supabase Auth)
    - Cannot update tier (use upgrade operation)
    - Cannot update credits directly

Operation: upgrade(user_id: UUID) → {user: User, team: Team, membership: Membership}
  Pre:  ∃ u ∈ User : u.id = user_id ∧ u.tier = 'starter'
  Post: BEGIN TRANSACTION
          u.tier = 'creator'
          ∧ u.upgraded_at = now()
          ∧ u.updated_at = now()
          ∧ Team created with:
              id = uuid()
              name = 'My Team'
              slug = slugify('My Team') + '-' + random_hex(8)
              credits = 0
          ∧ Membership created with:
              id = uuid()
              team_id = new_team.id
              user_id = user_id
              role = 'owner'
          ∧ Project created with:
              id = uuid()
              team_id = new_team.id
              created_by = user_id
              name = 'Welcome to Framecast'
              status = 'draft'
              spec = WELCOME_SPEC
        COMMIT

  Notes:
    - Idempotent for creator users (returns current state)
    - Atomic: user upgrade, team creation, membership, and welcome project
    - Enforces INV-U1 (creator ↔ upgraded_at IS NOT NULL)
    - Enforces INV-U2 (creator belongs to ≥1 team)

Operation: delete_user(user_id: UUID) → void
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ ∀ m ∈ Membership WHERE m.user_id = user_id :
            (m.role ≠ 'owner'
             ∨ |{m2 ∈ Membership : m2.team_id = m.team_id ∧ m2.role = 'owner' ∧ m2.user_id ≠ user_id}| ≥ 1)
  Post: User deleted (cascades to Membership, ApiKey)
        ∧ Supabase Auth account deleted
        ∧ Teams where user was sole member are deleted (cascade)
        ∧ Jobs and assets owned by user URN preserved (orphaned)

  Notes:
    - Cannot delete if user is sole owner of any team with other members (INV-T2)
    - Must transfer ownership or remove other members first
    - Ephemeral jobs/assets under personal URN become inaccessible
```

---

## 8.2 Team Operations

```
Operation: list_teams(user_id: UUID) → Team[]
  Pre:  ∃ u ∈ User : u.id = user_id ∧ u.tier = 'creator'
  Post: Returns all teams WHERE ∃ m ∈ Membership :
          m.team_id = team.id ∧ m.user_id = user_id
        Each team includes user's role (from membership)
        Ordered by team.name ASC

  Notes:
    - Creator-only operation (INV-U3: starters have no memberships)
    - Returns only teams user is a member of

Operation: get_team(team_id: UUID, user_id: UUID) → Team
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
  Post: Returns team with:
          id, name, slug, credits, ephemeral_storage_bytes, settings,
          created_at, updated_at
        ∧ Includes user's membership role

  Notes:
    - Any team member can view team details (all roles)

Operation: create_team(user_id: UUID, params: CreateTeamParams) → {team: Team, membership: Membership}
  Pre:  ∃ u ∈ User : u.id = user_id ∧ u.tier = 'creator'
        ∧ |params.name| ≥ 1 ∧ |params.name| ≤ 100
        ∧ (params.slug IS NULL ∨ valid_slug(params.slug))
        ∧ (params.slug IS NULL ∨ ∄ t ∈ Team : t.slug = params.slug)
        ∧ |{m ∈ Membership : m.user_id = user_id ∧ m.role = 'owner'}| < 10
        ∧ |{m ∈ Membership : m.user_id = user_id}| < 50
  Post: BEGIN TRANSACTION
          Team created with:
            id = uuid()
            name = params.name
            slug = params.slug ?? slugify(params.name) + '-' + random_hex(8)
            credits = 0
            ephemeral_storage_bytes = 0
            settings = {}
          ∧ Membership created with:
            id = uuid()
            team_id = new_team.id
            user_id = user_id
            role = 'owner'
        COMMIT

  CreateTeamParams:
    name: String! (1-100 chars)
    slug?: String (valid slug format, max 50)

  Notes:
    - Creator-only operation
    - Enforces INV-T7 (max 10 owned teams per user)
    - Enforces INV-T8 (max 50 team memberships per user)
    - Slug auto-generated if not provided (INV-T4 format)
    - Slug uniqueness enforced (INV-T3)

Operation: update_team(team_id: UUID, user_id: UUID, updates: TeamUpdates) → Team
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
        ∧ (updates.name IS NULL ∨ (|updates.name| ≥ 1 ∧ |updates.name| ≤ 100))
        ∧ (updates.settings IS NULL ∨ valid_json(updates.settings))
  Post: Team updated with provided fields
        ∧ t.updated_at = now()

  TeamUpdates:
    name?: String (1-100 chars)
    settings?: JSONB

  Notes:
    - Owner or admin role required
    - Slug cannot be changed after creation
    - Settings is a free-form JSONB object

Operation: delete_team(team_id: UUID, user_id: UUID) → void
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role = 'owner'
        ∧ |{m ∈ Membership : m.team_id = team_id}| = 1
        ∧ ∄ j ∈ Job : j.owner STARTS WITH 'framecast:team:' || team_id
            ∧ j.status ∈ {'queued', 'processing'}
  Post: Team deleted (cascades to Membership, Project, Webhook, Invitation)
        ∧ Associated S3 storage scheduled for cleanup

  Notes:
    - Owner-only operation
    - Team must have no other members (sole member check)
    - No active jobs can exist for the team
    - Cascades to all team-owned resources
```

---

## 8.3 Membership Operations

```
Operation: list_members(team_id: UUID, user_id: UUID) → Membership[]
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
  Post: Returns all memberships WHERE team_id = team_id
        Each membership includes user details (id, email, name, avatar_url)
        Ordered by role priority (owner > admin > member > viewer), then name ASC

  Notes:
    - Any team member can view the member list (all roles)

Operation: update_member_role(team_id: UUID, user_id: UUID, target_user_id: UUID, new_role: Role) → Membership
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ actor ∈ Membership : actor.team_id = team_id ∧ actor.user_id = user_id
            ∧ actor.role ∈ {owner, admin}
        ∧ ∃ target ∈ Membership : target.team_id = team_id ∧ target.user_id = target_user_id
        ∧ user_id ≠ target_user_id
        ∧ new_role ∈ {owner, admin, member, viewer}
        ∧ (actor.role = 'owner' ∨ (actor.role = 'admin' ∧ target.role ≠ 'owner' ∧ new_role ≠ 'owner'))
        ∧ (target.role = 'owner' → new_role = 'owner'
           ∨ |{m ∈ Membership : m.team_id = team_id ∧ m.role = 'owner' ∧ m.user_id ≠ target_user_id}| ≥ 1)
  Post: target.role = new_role

  Notes:
    - Owner or admin required
    - Admins cannot modify owners or promote to owner
    - Cannot change own role (use leave_team instead)
    - Enforces INV-T2: cannot demote last owner unless another owner exists
    - Owners can promote any member to any role including owner

Operation: remove_member(team_id: UUID, user_id: UUID, target_user_id: UUID) → void
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ actor ∈ Membership : actor.team_id = team_id ∧ actor.user_id = user_id
            ∧ actor.role ∈ {owner, admin}
        ∧ ∃ target ∈ Membership : target.team_id = team_id ∧ target.user_id = target_user_id
        ∧ user_id ≠ target_user_id
        ∧ (actor.role = 'owner' ∨ target.role ≠ 'owner')
        ∧ (target.role ≠ 'owner'
           ∨ |{m ∈ Membership : m.team_id = team_id ∧ m.role = 'owner' ∧ m.user_id ≠ target_user_id}| ≥ 1)
  Post: target membership deleted
        ∧ If target was last member: team is deleted (cascade)

  Notes:
    - Owner or admin required
    - Admins cannot remove owners
    - Cannot remove self (use leave_team instead)
    - Enforces INV-T2: cannot remove last owner

Operation: leave_team(team_id: UUID, user_id: UUID) → void
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
        ∧ (m.role ≠ 'owner'
           ∨ |{m2 ∈ Membership : m2.team_id = team_id ∧ m2.role = 'owner' ∧ m2.user_id ≠ user_id}| ≥ 1
           ∨ |{m2 ∈ Membership : m2.team_id = team_id}| = 1)
  Post: BEGIN TRANSACTION
          Membership deleted
          ∧ IF |{m ∈ Membership : m.team_id = team_id}| = 0 THEN
              Team deleted (cascade to Project, Webhook, Invitation)
        COMMIT

  Notes:
    - Any member can leave
    - Last owner cannot leave if other members exist (INV-T2)
    - Last owner CAN leave if they are the sole member → team is deleted
    - Atomic: membership removal and optional team deletion
```

---

## 8.4 Invitation Operations

```
Operation: list_invitations(team_id: UUID, user_id: UUID) → Invitation[]
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
  Post: Returns all invitations WHERE team_id = team_id
        Each invitation includes derived state (pending, accepted, declined, expired, revoked)
        Ordered by created_at DESC

  Notes:
    - Owner or admin required
    - Returns all invitations including non-actionable ones (for audit)

Operation: create_invitation(team_id: UUID, user_id: UUID, params: InvitationParams) → Invitation
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
        ∧ valid_email(params.email) ∧ |params.email| ≤ 255
        ∧ params.role ∈ {admin, member, viewer}
        ∧ user_id ≠ (SELECT id FROM User WHERE email = params.email)
        ∧ ∄ m ∈ Membership : m.team_id = team_id
            ∧ m.user_id = (SELECT id FROM User WHERE email = params.email)
        ∧ |{i ∈ Invitation : i.team_id = team_id ∧ i.is_actionable}| < 50
  Post: Invitation created with:
          id = uuid()
          team_id = team_id
          invited_by = user_id
          email = params.email
          role = params.role ?? 'member'
          token = generate_token(32)
          expires_at = now() + INTERVAL '7 days'
        ∧ Invitation email sent to params.email

  InvitationParams:
    email: String! (valid email, max 255)
    role?: {admin | member | viewer} (default: member)

  Notes:
    - Owner or admin required
    - Cannot invite self (INV-I7)
    - Cannot invite existing team member (INV-I8)
    - Cannot invite with role 'owner' (INV-I2)
    - Enforces CARD-4 (max 50 pending invitations per team)
    - If an actionable invitation already exists for same email+team,
      the existing invitation is revoked and a new one is created
    - Email delivery is best-effort; invitation is created regardless

Operation: revoke_invitation(team_id: UUID, user_id: UUID, invitation_id: UUID) → void
  Pre:  ∃ i ∈ Invitation : i.id = invitation_id ∧ i.team_id = team_id ∧ i.is_actionable
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
  Post: i.revoked_at = now()

  Notes:
    - Owner or admin required
    - Can only revoke actionable (pending) invitations
    - Revoked invitations cannot be accepted (INV-I4)

Operation: resend_invitation(team_id: UUID, user_id: UUID, invitation_id: UUID) → Invitation
  Pre:  ∃ i ∈ Invitation : i.id = invitation_id ∧ i.team_id = team_id ∧ i.is_actionable
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
  Post: i.expires_at = now() + INTERVAL '7 days'
        ∧ Invitation email re-sent to i.email

  Notes:
    - Owner or admin required
    - Can only resend actionable (pending) invitations
    - Extends expiration by 7 days from now
    - Does NOT change invitation state (remains pending)

Operation: accept_invitation(invitation_id: UUID, user_id: UUID) → Membership
  Pre:  ∃ i ∈ Invitation : i.id = invitation_id ∧ i.is_actionable
        ∧ ∃ u ∈ User : u.id = user_id ∧ u.email = i.email
        ∧ ∄ m ∈ Membership : m.team_id = i.team_id ∧ m.user_id = user_id
        ∧ |{m ∈ Membership : m.user_id = user_id}| < 50
  Post: BEGIN TRANSACTION
          -- Auto-upgrade Starter → Creator if needed
          IF u.tier = 'starter' THEN
            u.tier = 'creator'
            ∧ u.upgraded_at = now()
            ∧ Personal team created (see T4 in User Model)
            ∧ Welcome project created in personal team

          -- Join invited team
          Membership created with:
            id = uuid()
            team_id = i.team_id
            user_id = user_id
            role = i.role

          -- Mark invitation
          i.accepted_at = now()
        COMMIT

  Notes:
    - Only the invited user (matching email) can accept
    - Auto-upgrades Starter to Creator (INV-U2: creator must have ≥1 team)
    - Enforces INV-T8 (max 50 team memberships)
    - Enforces INV-I3 (accepted cannot be revoked)
    - Token-based acceptance also supported for new users (see T3 in User Model)

Operation: decline_invitation(invitation_id: UUID, user_id: UUID) → void
  Pre:  ∃ i ∈ Invitation : i.id = invitation_id ∧ i.is_actionable
        ∧ ∃ u ∈ User : u.id = user_id ∧ u.email = i.email
  Post: i.declined_at = now()

  Notes:
    - Only the invited user (matching email) can decline
    - Declined invitations cannot be accepted
    - Team admin can send a new invitation after decline
```

---

## 8.5 Job Operations

```
Operation: list_jobs(user_id: UUID, filters: JobFilters?) → Page<Job>
  Pre:  ∃ u ∈ User : u.id = user_id
  Post: Returns jobs accessible to user:
          IF u.tier = 'starter' THEN
            WHERE owner = 'framecast:user:' || user_id
          ELSE
            WHERE owner ∈ user_accessible_urns(user_id)
        Ordered by created_at DESC

  JobFilters:
    status?: {queued | processing | completed | failed | canceled}
    owner?: URN
    project_id?: UUID
    created_after?: Timestamp
    created_before?: Timestamp
    limit?: Integer (1-100, default 20)
    cursor?: String

  Notes:
    - Starters see only personal jobs
    - Creators see jobs for all accessible URNs (personal + team + membership)
    - Cursor-based pagination

Operation: get_job(job_id: UUID, user_id: UUID) → Job
  Pre:  ∃ j ∈ Job : j.id = job_id
        ∧ user_can_access_owner(user_id, j.owner)
  Post: Returns job with all fields
        ∧ Output URLs are presigned (1 hour expiry) if status = 'completed'

  Notes:
    - Access determined by owner URN
    - Starters can only access own jobs (owner = framecast:user:{user_id})
    - Creators can access team/membership URN jobs where they have membership

Operation: create_ephemeral_job(user_id: UUID, params: EphemeralJobParams) → Job
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ validate_spec(params.spec, user_id).valid = true
        ∧ (params.owner IS NULL ∨ user_can_use_owner_urn(user_id, params.owner))
        ∧ credit_source(params.owner ?? 'framecast:user:' || user_id).credits ≥ estimated_credits(params.spec)
        ∧ concurrent_job_count(params.owner ?? 'framecast:user:' || user_id) < concurrent_limit(u)
        ∧ (params.idempotency_key IS NULL ∨ ∄ j ∈ Job :
            j.triggered_by = user_id ∧ j.idempotency_key = params.idempotency_key)
  Post: BEGIN TRANSACTION
          Job created with:
            id = uuid()
            owner = params.owner ?? 'framecast:user:' || user_id
            triggered_by = user_id
            project_id = NULL
            status = 'queued'
            spec_snapshot = params.spec
            options = params.options ?? {}
            credits_charged = estimated_credits(params.spec)
            idempotency_key = params.idempotency_key
          ∧ Credits debited from credit_source(job.owner)
          ∧ JobEvent created (type = 'queued')
          ∧ Job enqueued to Inngest for processing
        COMMIT

  EphemeralJobParams:
    spec: JSONB! (valid spec)
    owner?: URN
    options?: JSONB
    idempotency_key?: String

  Notes:
    - Available to both Starter and Creator tiers
    - Starter: max 1 concurrent job (CARD-6), owner must be personal URN
    - Creator: max 5 concurrent jobs per team (CARD-5)
    - Credits reserved upfront, refunded on failure per refund policy (§12.7)
    - Idempotency key prevents duplicate submissions

Operation: create_project_job(project_id: UUID, user_id: UUID, params: ProjectJobParams?) → Job
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status ∈ {draft, completed}
        ∧ p.spec IS NOT NULL
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin, member}
        ∧ validate_spec(p.spec, user_id).valid = true
        ∧ ∄ j ∈ Job : j.project_id = project_id ∧ j.status ∈ {'queued', 'processing'}
        ∧ credit_source('framecast:team:' || p.team_id).credits ≥ estimated_credits(p.spec)
        ∧ concurrent_job_count('framecast:team:' || p.team_id) < 5
  Post: BEGIN TRANSACTION
          Job created with:
            id = uuid()
            owner = 'framecast:team:' || p.team_id
            triggered_by = user_id
            project_id = project_id
            status = 'queued'
            spec_snapshot = p.spec
            options = params.options ?? {}
            credits_charged = estimated_credits(p.spec)
          ∧ p.status = 'rendering'
          ∧ Credits debited from Team.credits
          ∧ JobEvent created (type = 'queued')
          ∧ Job enqueued to Inngest for processing
        COMMIT

  ProjectJobParams:
    options?: JSONB

  Notes:
    - Creator-only (requires team membership)
    - Owner, admin, or member role required (viewer cannot trigger)
    - Enforces INV-J12 (max 1 active job per project)
    - Enforces CARD-5 (max 5 concurrent jobs per team)
    - Project status transitions to 'rendering' atomically
    - Project spec is snapshot into job (immutable copy)

Operation: get_job_events(job_id: UUID, user_id: UUID, last_event_id?: String) → SSE<JobEvent>
  Pre:  ∃ j ∈ Job : j.id = job_id
        ∧ user_can_access_owner(user_id, j.owner)
  Post: Returns Server-Sent Events stream of job events
        IF last_event_id IS NOT NULL THEN
          Parse job_id and sequence from last_event_id
          Replay events WHERE job_id = j.id AND sequence > parsed_sequence
          IF parsed_sequence not found (expired) THEN return HTTP 410 Gone
        ELSE
          Stream all events from current position

  SSE Event Format:
    id: {job_id}:{sequence}
    event: {event_type}
    data: {payload as JSON}

  Notes:
    - Supports reconnection via Last-Event-ID header
    - Events retained for 7 days (see §4.8 Retention)
    - Stream closes when job reaches terminal state
    - Access determined by owner URN

Operation: cancel_job(job_id: UUID, user_id: UUID) → Job
  Pre:  ∃ j ∈ Job : j.id = job_id ∧ j.status ∈ {'queued', 'processing'}
        ∧ (j.triggered_by = user_id
           ∨ (∃ m ∈ Membership : m.team_id = extract_team_from_urn(j.owner)
              ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}))
  Post: BEGIN TRANSACTION
          j.status = 'canceled'
          ∧ j.failure_type = 'canceled'
          ∧ j.completed_at = now()
          ∧ j.updated_at = now()
          ∧ Credits partially refunded per refund policy (§12.7)
          ∧ JobEvent created (type = 'canceled')
          ∧ IF j.project_id IS NOT NULL THEN
              Project.status = 'draft'
        COMMIT

  Notes:
    - Job creator can cancel their own jobs
    - Team owner/admin can cancel any team job
    - Members can cancel only their own jobs
    - Enforces Job state machine: queued → canceled, processing → canceled
    - Partial credit refund based on progress (see §12.7)

Operation: delete_job(job_id: UUID, user_id: UUID) → void
  Pre:  ∃ j ∈ Job : j.id = job_id
        ∧ j.is_ephemeral = true (project_id IS NULL)
        ∧ j.status ∈ {completed, failed, canceled}
        ∧ user_can_access_owner(user_id, j.owner)
  Post: Job deleted (cascades to JobEvent)
        ∧ Associated S3 output files scheduled for deletion

  Notes:
    - Only ephemeral (non-project) jobs can be deleted
    - Job must be in terminal state
    - Starters can delete own ephemeral jobs
    - Creators can delete accessible ephemeral jobs

Operation: clone_job(job_id: UUID, user_id: UUID, params: CloneJobParams?) → Job
  Pre:  ∃ j ∈ Job : j.id = job_id
        ∧ user_can_access_owner(user_id, j.owner)
        ∧ (params.owner IS NULL ∨ user_can_use_owner_urn(user_id, params.owner))
        ∧ credit_source(params.owner ?? j.owner).credits ≥ estimated_credits(j.spec_snapshot)
        ∧ concurrent_job_count(params.owner ?? j.owner) < concurrent_limit(user)
  Post: New ephemeral Job created with:
          id = uuid()
          owner = params.owner ?? j.owner
          triggered_by = user_id
          project_id = NULL
          status = 'queued'
          spec_snapshot = j.spec_snapshot
          options = j.options
          credits_charged = estimated_credits(j.spec_snapshot)
        ∧ Credits debited from credit_source(new_job.owner)
        ∧ JobEvent created (type = 'queued')
        ∧ Job enqueued to Inngest for processing

  CloneJobParams:
    owner?: URN

  Notes:
    - Creates a new ephemeral job from an existing job's spec
    - Starters can clone their own jobs
    - Creators can clone any accessible job
    - Cloned job is always ephemeral (no project association)
    - Owner can be overridden to bill a different entity
```

---

## 8.6 Project Operations

```
Operation: list_projects(team_id: UUID, user_id: UUID, filters: ProjectFilters?) → Page<Project>
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
  Post: Returns projects WHERE team_id = team_id
        Default: excludes archived projects (unless filters.include_archived = true)
        Ordered by updated_at DESC

  ProjectFilters:
    status?: {draft | rendering | completed | archived}
    include_archived?: Boolean (default: false)
    limit?: Integer (1-100, default 20)
    cursor?: String

  Notes:
    - Any team member can list projects (all roles)
    - Archived projects hidden by default

Operation: get_project(project_id: UUID, user_id: UUID) → Project
  Pre:  ∃ p ∈ Project : p.id = project_id
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
  Post: Returns project with all fields including spec
        ∧ Includes latest job status if one exists

  Notes:
    - Any team member can view project details (all roles)

Operation: create_project(team_id: UUID, user_id: UUID, params: CreateProjectParams) → Project
  Pre:  ∃ t ∈ Team : t.id = team_id
        ∧ ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin, member}
        ∧ |params.name| ≥ 1 ∧ |params.name| ≤ 200
        ∧ (params.spec IS NULL ∨ valid_json(params.spec))
  Post: Project created with:
          id = uuid()
          team_id = team_id
          created_by = user_id
          name = params.name
          status = 'draft'
          spec = params.spec ?? NULL

  CreateProjectParams:
    name: String! (1-200 chars)
    spec?: JSONB

  Notes:
    - Owner, admin, or member role required (viewer cannot create)
    - Spec is optional at creation time (can be added later via update_spec)
    - Spec is NOT validated on creation (use validate_spec separately)

Operation: update_project(project_id: UUID, user_id: UUID, updates: ProjectUpdates) → Project
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status ≠ 'rendering'
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin, member}
        ∧ (updates.name IS NULL ∨ (|updates.name| ≥ 1 ∧ |updates.name| ≤ 200))
  Post: Project updated with provided fields
        ∧ p.updated_at = now()

  ProjectUpdates:
    name?: String (1-200 chars)

  Notes:
    - Owner, admin, or member role required (viewer cannot edit)
    - Cannot update while rendering (status = 'rendering')
    - Use update_spec for spec changes

Operation: update_spec(project_id: UUID, user_id: UUID, spec: JSONB) → Project
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status ∈ {draft, completed}
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin, member}
        ∧ valid_json(spec)
  Post: p.spec = spec
        ∧ p.updated_at = now()

  Notes:
    - Owner, admin, or member role required
    - Cannot update spec while rendering
    - Full replacement (PUT semantics), not partial update
    - Spec is NOT validated on save (use validate_spec separately)
    - Updating spec on a completed project does NOT change status to draft

Operation: delete_project(project_id: UUID, user_id: UUID) → void
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status ≠ 'rendering'
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
            ∧ m.role ∈ {owner, admin}
        ∧ ∄ j ∈ Job : j.project_id = project_id ∧ j.status ∈ {'queued', 'processing'}
  Post: Project deleted (cascades to AssetFile)
        ∧ Associated jobs have project_id set to NULL (ON DELETE SET NULL)

  Notes:
    - Owner or admin role required
    - Cannot delete while rendering
    - No active jobs can reference the project
    - Jobs are preserved (project_id set to NULL) for billing audit
    - Project-scoped assets are deleted (cascade)
```

---

## 8.7 Asset Operations

```
Operation: list_assets(user_id: UUID, filters: AssetFilters?) → Page<AssetFile>
  Pre:  ∃ u ∈ User : u.id = user_id
  Post: Returns assets accessible to user:
          IF u.tier = 'starter' THEN
            WHERE owner = 'framecast:user:' || user_id
          ELSE
            WHERE owner ∈ user_accessible_urns(user_id)
        ∧ (filters applied)
        Ordered by created_at DESC

  AssetFilters:
    owner?: URN
    project_id?: UUID
    content_type?: String (prefix match, e.g., 'image/')
    status?: {pending | ready | failed}
    limit?: Integer (1-100, default 20)
    cursor?: String

  Notes:
    - Starters see only personal assets
    - Creators see assets for all accessible URNs
    - Cursor-based pagination

Operation: get_asset(asset_id: UUID, user_id: UUID) → AssetFile
  Pre:  ∃ a ∈ AssetFile : a.id = asset_id
        ∧ user_can_access_owner(user_id, a.owner)
  Post: Returns asset with all fields
        ∧ Includes presigned download URL (1 hour expiry) if status = 'ready'

  Notes:
    - Access determined by owner URN

Operation: create_upload_url(user_id: UUID, params: UploadParams) → {asset: AssetFile, upload_url: String}
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ |params.filename| ≤ 255
        ∧ params.content_type ∈ {
            'image/jpeg', 'image/png', 'image/webp',
            'audio/mpeg', 'audio/wav', 'audio/ogg',
            'video/mp4'
          }
        ∧ params.size_bytes > 0 ∧ params.size_bytes ≤ 50 * 1024 * 1024
        ∧ (params.owner IS NULL ∨ user_can_use_owner_urn(user_id, params.owner))
        ∧ (params.project_id IS NULL ∨ (
            ∃ p ∈ Project : p.id = params.project_id
            ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id
                ∧ m.role ∈ {owner, admin, member}
          ))
  Post: AssetFile created with:
          id = uuid()
          owner = params.owner ?? 'framecast:user:' || user_id
          uploaded_by = user_id
          project_id = params.project_id
          filename = params.filename
          s3_key = generate_s3_key(owner, id, filename)
          content_type = params.content_type
          size_bytes = params.size_bytes
          status = 'pending'
        ∧ Presigned S3 upload URL returned (15 minute expiry)

  UploadParams:
    filename: String! (max 255)
    content_type: String! (valid MIME type)
    size_bytes: Integer! (1 to 50MB)
    owner?: URN
    project_id?: UUID

  Notes:
    - Available to both tiers
    - Asset starts in 'pending' status
    - Client must upload file to presigned URL, then call confirm_upload
    - Upload URL expires in 15 minutes
    - If project_id is set, owner must match project's team (INV-X3)

Operation: confirm_upload(asset_id: UUID, user_id: UUID) → AssetFile
  Pre:  ∃ a ∈ AssetFile : a.id = asset_id ∧ a.status = 'pending'
        ∧ a.uploaded_by = user_id
        ∧ S3 object exists at a.s3_key
        ∧ S3 object size matches a.size_bytes (±5%)
        ∧ S3 object content type matches a.content_type
  Post: a.status = 'ready'
        ∧ a.updated_at = now()
        ∧ Storage quota updated for owner

  Notes:
    - Only the uploader can confirm
    - Validates that the S3 upload actually completed
    - If validation fails, status transitions to 'failed'
    - Storage quota for owner is incremented by size_bytes

Operation: delete_asset(asset_id: UUID, user_id: UUID) → void
  Pre:  ∃ a ∈ AssetFile : a.id = asset_id
        ∧ user_can_access_owner(user_id, a.owner)
        ∧ (a.uploaded_by = user_id
           ∨ ∃ m ∈ Membership : m.team_id = extract_team_from_urn(a.owner)
               ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin})
  Post: AssetFile deleted
        ∧ S3 object deleted
        ∧ Storage quota decremented for owner

  Notes:
    - Uploaders can delete their own assets
    - Team owner/admin can delete any team asset
    - Members can delete only their own assets within team
    - Viewers cannot delete assets
```

---

## 8.8 System Asset Operations

```
Operation: list_system_assets(user_id: UUID, filters: SystemAssetFilters?) → SystemAsset[]
  Pre:  ∃ u ∈ User : u.id = user_id
  Post: Returns system assets matching filters
        Ordered by category ASC, name ASC

  SystemAssetFilters:
    category?: {sfx | ambient | music | transition}
    tags?: String[] (match any)
    search?: String (name/description substring match)

  Notes:
    - Available to both Starter and Creator tiers
    - System assets are read-only, managed by the system
    - No pagination needed (catalog is small)

Operation: get_system_asset(asset_id: String, user_id: UUID) → SystemAsset
  Pre:  ∃ a ∈ SystemAsset : a.id = asset_id
        ∧ ∃ u ∈ User : u.id = user_id
  Post: Returns system asset with all fields
        ∧ Includes presigned preview URL (24 hour expiry)

  Notes:
    - Available to both tiers
    - Asset ID format: asset_{category}_{name} (e.g., asset_sfx_whoosh_1)
    - Preview URL allows listening/viewing before use in spec
```

---

## 8.9 Webhook Operations

```
Operation: list_webhooks(team_id: UUID, user_id: UUID) → Webhook[]
  Pre:  ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: Returns all webhooks WHERE team_id = team_id
        Ordered by created_at DESC
        Secret field is NOT returned in list

Operation: get_webhook(webhook_id: UUID, user_id: UUID) → Webhook
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: Returns webhook
        Secret field is NOT returned (use rotate_webhook_secret to get new secret)

Operation: create_webhook(team_id: UUID, user_id: UUID, params: WebhookParams) → {webhook: Webhook, secret: String}
  Pre:  ∃ m ∈ Membership : m.team_id = team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
        ∧ valid_https_url(params.url)
        ∧ |params.events| > 0
        ∧ ∀ e ∈ params.events : e ∈ ValidWebhookEvents
  Post: Webhook created with:
          id = uuid()
          team_id = team_id
          created_by = user_id
          url = params.url
          events = params.events
          secret = generate_secret(32)
          is_active = true
        ∧ Raw secret returned (only time it's visible)

  WebhookParams:
    url: URL (HTTPS only, max 2048)
    events: String[] (at least one valid event)

  ValidWebhookEvents:
    - job.queued
    - job.started
    - job.progress
    - job.completed
    - job.failed
    - job.canceled

Operation: update_webhook(webhook_id: UUID, user_id: UUID, updates: WebhookUpdates) → Webhook
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
        ∧ (updates.url IS NULL ∨ valid_https_url(updates.url))
        ∧ (updates.events IS NULL ∨ (|updates.events| > 0 ∧ ∀ e ∈ updates.events : e ∈ ValidWebhookEvents))
  Post: Webhook updated with provided fields
        ∧ w.updated_at = now()

  WebhookUpdates:
    url?: URL (HTTPS only, max 2048)
    events?: String[]
    is_active?: Boolean

Operation: delete_webhook(webhook_id: UUID, user_id: UUID) → void
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: Webhook deleted (cascades to WebhookDelivery)

Operation: rotate_webhook_secret(webhook_id: UUID, user_id: UUID) → {webhook: Webhook, secret: String}
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: w.secret = generate_secret(32)
        ∧ w.updated_at = now()
        ∧ New raw secret returned

Operation: test_webhook(webhook_id: UUID, user_id: UUID) → WebhookDelivery
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id ∧ w.is_active = true
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: Test delivery created and attempted immediately
        Event type = 'webhook.test'
        Returns delivery result (status, response_status, response_body)

Operation: list_webhook_deliveries(webhook_id: UUID, user_id: UUID, filters: DeliveryFilters?) → Page<WebhookDelivery>
  Pre:  ∃ w ∈ Webhook : w.id = webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: Returns deliveries matching filters, ordered by created_at DESC

  DeliveryFilters:
    status?: {pending | retrying | delivered | failed}
    event_type?: String
    created_after?: Timestamp
    created_before?: Timestamp
    limit?: Integer (1-100, default 20)
    cursor?: String

Operation: retry_webhook_delivery(delivery_id: UUID, user_id: UUID) → WebhookDelivery
  Pre:  ∃ d ∈ WebhookDelivery : d.id = delivery_id ∧ d.status = 'failed'
        ∧ ∃ w ∈ Webhook : w.id = d.webhook_id
        ∧ ∃ m ∈ Membership : m.team_id = w.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: d.status = 'pending'
        ∧ d.attempts = 0
        ∧ d.next_retry_at = now()
        ∧ Delivery will be attempted immediately
```

---

## 8.10 API Key Operations

```
Operation: list_api_keys(user_id: UUID) → ApiKey[]
  Pre:  ∃ u ∈ User : u.id = user_id
  Post: Returns all API keys WHERE user_id = user_id
        Ordered by created_at DESC
        key_hash is NOT returned
        Only key_prefix is visible (e.g., "sk_live_abc...")

Operation: get_api_key(key_id: UUID, user_id: UUID) → ApiKey
  Pre:  ∃ k ∈ ApiKey : k.id = key_id ∧ k.user_id = user_id
  Post: Returns API key details
        key_hash is NOT returned

Operation: create_api_key(user_id: UUID, params: ApiKeyParams) → {api_key: ApiKey, raw_key: String}
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ (params.name IS NULL ∨ |params.name| ≤ 100)
        ∧ (params.owner IS NULL ∨ user_can_use_owner_urn(user_id, params.owner))
        ∧ (params.scopes IS NULL ∨ (
            ∀ s ∈ params.scopes : s ∈ AllowedScopes
            ∧ (u.tier = 'creator' ∨ s ∈ StarterAllowedScopes)
          ))
        ∧ (params.expires_at IS NULL ∨ params.expires_at > now())
  Post: ApiKey created with:
          id = uuid()
          user_id = user_id
          owner = params.owner ?? 'framecast:user:' || user_id
          name = params.name ?? 'Default'
          key_prefix = 'sk_live_' + random(4)
          key_hash = sha256(raw_key)
          scopes = params.scopes ?? ['*']
          expires_at = params.expires_at
        ∧ Raw key returned (ONLY TIME it's visible)
        ∧ Raw key format: sk_live_XXXXXXXX_YYYYYYYYYYYYYYYYYYYYYYYYYYYY

  ApiKeyParams:
    name?: String (max 100)
    owner?: URN
    scopes?: String[]
    expires_at?: Timestamp

  AllowedScopes:
    - generate
    - jobs:read
    - jobs:write
    - assets:read
    - assets:write
    - projects:read
    - projects:write
    - team:read
    - team:admin
    - * (wildcard)

  StarterAllowedScopes:
    - generate
    - jobs:read
    - jobs:write
    - assets:read
    - assets:write

Operation: update_api_key(key_id: UUID, user_id: UUID, updates: ApiKeyUpdates) → ApiKey
  Pre:  ∃ k ∈ ApiKey : k.id = key_id ∧ k.user_id = user_id ∧ k.revoked_at IS NULL
        ∧ (updates.name IS NULL ∨ |updates.name| ≤ 100)
  Post: API key updated with provided fields

  ApiKeyUpdates:
    name?: String (max 100)

  Note: scopes and owner cannot be modified after creation.
        To change scopes/owner, revoke and create new key.

Operation: revoke_api_key(key_id: UUID, user_id: UUID) → void
  Pre:  ∃ k ∈ ApiKey : k.id = key_id ∧ k.user_id = user_id ∧ k.revoked_at IS NULL
  Post: k.revoked_at = now()
        ∧ Key immediately becomes invalid
        ∧ Key record preserved for audit (30 days retention)
```

---

## 8.11 Project Archive Operations

```
Operation: archive_project(project_id: UUID, user_id: UUID) → Project
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status ∈ {draft, completed}
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: p.status = 'archived'
        ∧ p.updated_at = now()

  Notes:
    - Cannot archive a project that is currently rendering
    - Archived projects are hidden from default list view
    - Jobs and assets associated with project are preserved

Operation: unarchive_project(project_id: UUID, user_id: UUID) → Project
  Pre:  ∃ p ∈ Project : p.id = project_id ∧ p.status = 'archived'
        ∧ ∃ m ∈ Membership : m.team_id = p.team_id ∧ m.user_id = user_id ∧ m.role ∈ {owner, admin}
  Post: p.status = 'draft'
        ∧ p.updated_at = now()

  Notes:
    - Unarchived projects return to draft status
    - Previous completion status is not preserved
    - User can re-render after unarchiving
```

---

## 8.12 Estimation & Validation Operations

```
Operation: validate_spec(spec: JSONB, user_id: UUID) → ValidationResponse
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ spec IS NOT NULL
  Post: Returns validation result with errors and warnings

  ValidationResponse:
    valid: Boolean
    errors: Array<{
      path: String        // JSONPath to problematic field
      message: String     // Human-readable error
      value?: Any         // The value that failed
      valid_values?: Any[]  // Acceptable values if applicable
      limit?: Number      // Limit exceeded if applicable
    }>
    warnings: Array<{
      path: String
      message: String
    }>

  Validation Rules Applied:
    - Size limits (spec ≤ 100KB, scenes ≤ 50, etc.)
    - Field limits (prompt ≤ 2000 chars, duration 1-30s, etc.)
    - Reference integrity (timeline → scenes, symbols, transitions)
    - Asset existence (for user assets, checks status = 'ready')
    - System asset validity (checks against catalog)

  Notes:
    - Does NOT consume credits
    - Does NOT check credit balance
    - Validates spec structure and references only
    - Asset ownership validation uses user_id context

Operation: estimate_spec(spec: JSONB, user_id: UUID, owner?: URN) → EstimateResponse
  Pre:  ∃ u ∈ User : u.id = user_id
        ∧ validate_spec(spec, user_id).valid = true
        ∧ (owner IS NULL ∨ user_can_use_owner_urn(user_id, owner))
  Post: Returns credit estimate and generation time forecast

  EstimateResponse:
    estimated_duration_seconds: Number    // Total video duration
    estimated_credits: Number             // Credits that will be charged
    estimated_generation_time_seconds: Number  // Wall clock time
    scenes: Array<{
      id: String
      duration: Number
      credits: Number
    }>
    warnings?: Array<{
      message: String
    }>

  Notes:
    - Does NOT consume credits
    - Does NOT reserve credits
    - Estimate is best-effort, actual may vary ±10%
    - If owner provided, validates credit availability
```

---

## 8.13 Endpoint Mapping Table

| Operation | HTTP Method | Endpoint |
|-----------|-------------|----------|
| **User** | | |
| get_profile | GET | /v1/account |
| signup | POST | /v1/auth/signup |
| update_profile | PATCH | /v1/account |
| upgrade | POST | /v1/account/upgrade |
| delete_user | DELETE | /v1/account |
| **Team** | | |
| list_teams | GET | /v1/teams |
| get_team | GET | /v1/teams/:id |
| create_team | POST | /v1/teams |
| update_team | PATCH | /v1/teams/:id |
| delete_team | DELETE | /v1/teams/:id |
| **Membership** | | |
| list_members | GET | /v1/teams/:id/members |
| update_member_role | PATCH | /v1/teams/:id/members/:user_id |
| remove_member | DELETE | /v1/teams/:id/members/:user_id |
| leave_team | POST | /v1/teams/:id/leave |
| **Invitation** | | |
| list_invitations | GET | /v1/teams/:id/invitations |
| create_invitation | POST | /v1/teams/:id/invitations |
| revoke_invitation | DELETE | /v1/teams/:id/invitations/:id |
| resend_invitation | POST | /v1/teams/:id/invitations/:id/resend |
| accept_invitation | POST | /v1/invitations/:id/accept |
| decline_invitation | POST | /v1/invitations/:id/decline |
| **Project** | | |
| list_projects | GET | /v1/teams/:id/projects |
| get_project | GET | /v1/projects/:id |
| create_project | POST | /v1/teams/:id/projects |
| update_project | PATCH | /v1/projects/:id |
| update_spec | PUT | /v1/projects/:id/spec |
| delete_project | DELETE | /v1/projects/:id |
| archive_project | POST | /v1/projects/:id/archive |
| unarchive_project | POST | /v1/projects/:id/unarchive |
| **Job** | | |
| list_jobs | GET | /v1/jobs |
| get_job | GET | /v1/jobs/:id |
| create_ephemeral_job | POST | /v1/generate |
| create_project_job | POST | /v1/projects/:id/render |
| get_job_events | GET | /v1/jobs/:id/events |
| cancel_job | POST | /v1/jobs/:id/cancel |
| delete_job | DELETE | /v1/jobs/:id |
| clone_job | POST | /v1/jobs/:id/clone |
| **Estimation** | | |
| validate_spec | POST | /v1/spec/validate |
| estimate_spec | POST | /v1/spec/estimate |
| **Asset** | | |
| list_assets | GET | /v1/assets |
| get_asset | GET | /v1/assets/:id |
| create_upload_url | POST | /v1/assets/upload-url |
| confirm_upload | POST | /v1/assets/:id/confirm |
| delete_asset | DELETE | /v1/assets/:id |
| **System Asset** | | |
| list_system_assets | GET | /v1/system-assets |
| get_system_asset | GET | /v1/system-assets/:id |
| **Webhook** | | |
| list_webhooks | GET | /v1/teams/:id/webhooks |
| get_webhook | GET | /v1/webhooks/:id |
| create_webhook | POST | /v1/teams/:id/webhooks |
| update_webhook | PATCH | /v1/webhooks/:id |
| delete_webhook | DELETE | /v1/webhooks/:id |
| rotate_webhook_secret | POST | /v1/webhooks/:id/rotate-secret |
| test_webhook | POST | /v1/webhooks/:id/test |
| list_webhook_deliveries | GET | /v1/webhooks/:id/deliveries |
| retry_webhook_delivery | POST | /v1/webhook-deliveries/:id/retry |
| **API Key** | | |
| list_api_keys | GET | /v1/auth/keys |
| get_api_key | GET | /v1/auth/keys/:id |
| create_api_key | POST | /v1/auth/keys |
| update_api_key | PATCH | /v1/auth/keys/:id |
| revoke_api_key | DELETE | /v1/auth/keys/:id |
| **Auth** | | |
| whoami | GET | /v1/auth/whoami |

---

## 8.14 Auth Operations

```
Operation: whoami() → WhoamiResponse
  Pre:  Valid JWT token or valid API key in Authorization header
  Post: Returns authentication context including user profile and auth method
        If authenticated via API key, includes key metadata (scopes, owner)

  WhoamiResponse:
    auth_method: "jwt" | "api_key"
    user: User (same fields as get_profile)
    api_key?: {id, owner, name, key_prefix, scopes, expires_at}
              (only present when auth_method = "api_key")

  Notes:
    - Supports both JWT (Supabase Auth) and API key authentication
    - Available to both Starter and Creator tiers
    - Primary use case: auth verification and debugging
    - API key metadata is a subset (excludes user_id, revoked_at, last_used_at, created_at)
```

---

**Document Version: 0.0.1-SNAPSHOT
**Last Updated**: 2025-01-30
