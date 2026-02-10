# Section 4: Entity Definitions

## 4.1 User

```
Entity: User
Description: An authenticated individual in the system

Note: Authentication (passwords, OAuth, sessions) is handled by Supabase Auth.
      User.id corresponds to Supabase Auth user ID.
      This entity stores application-level user data only.

Attributes:
  id                      : UUID PK (matches Supabase Auth user ID)
  email                   : String! (unique, max 255, valid email format, synced from Supabase Auth)
  name                    : String? (max 100)
  avatar_url              : URL?
  tier                    : {starter | creator} DEFAULT starter
  credits                 : Integer DEFAULT 0
  ephemeral_storage_bytes : BigInt DEFAULT 0
  upgraded_at             : Timestamp? (set when tier becomes creator)
  created_at              : Timestamp DEFAULT now()
  updated_at              : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(email)
  - INDEX(tier)

Triggers:
  - ON UPDATE: SET updated_at = now()
```

## 4.2 Team

```
Entity: Team
Description: A workspace that owns projects and assets. Composed of 1+ users.
             Teams vanish when last member leaves.

Attributes:
  id                      : UUID PK
  name                    : String! (max 100, min 1)
  slug                    : String! (unique, max 50, lowercase alphanumeric + hyphen)
  credits                 : Integer DEFAULT 0
  ephemeral_storage_bytes : BigInt DEFAULT 0
  settings                : JSONB DEFAULT {}
  created_at              : Timestamp DEFAULT now()
  updated_at              : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(slug)

Triggers:
  - ON UPDATE: SET updated_at = now()

Slug Generation (application layer, not DB trigger):
  - IF slug IS NULL THEN slug = slugify(name) + '-' + random_hex(8)
  - slugify: lowercase, ASCII transliteration, replace non-alphanumeric with hyphen
  - random_hex: 8 hex characters from UUID v4 (e.g., "my-team-a1b2c3d4")
  - Validated: ^[a-z0-9]([a-z0-9-]*[a-z0-9])?$, no consecutive hyphens, max 50 chars
```

## 4.3 Membership

```
Entity: Membership
Description: Association between User and Team with role

Attributes:
  id            : UUID PK
  team_id       : UUID FK Ã¢â€ â€™ Team (ON DELETE CASCADE)
  user_id       : UUID FK Ã¢â€ â€™ User (ON DELETE CASCADE)
  role          : {owner | admin | member | viewer} DEFAULT member
  created_at    : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(team_id, user_id)
  - INDEX(user_id)

Constraints:
  - Ã¢Ë†â‚¬ team Ã¢Ë†Ë† Team : |{m Ã¢Ë†Ë† Membership : m.team_id = team.id}| Ã¢â€°Â¥ 1
    (every team has at least one member)
  - Ã¢Ë†â‚¬ team Ã¢Ë†Ë† Team : |{m Ã¢Ë†Ë† Membership : m.team_id = team.id Ã¢Ë†Â§ m.role = 'owner'}| Ã¢â€°Â¥ 1
    (every team has at least one owner)
```

## 4.4 Invitation

```
Entity: Invitation
Description: Pending invitation to join a team

Attributes:
  id            : UUID PK
  team_id       : UUID FK Ã¢â€ â€™ Team (ON DELETE CASCADE)
  invited_by    : UUID FK Ã¢â€ â€™ User
  email         : String (valid email format)
  role          : {admin | member | viewer} DEFAULT member
  token         : String! (unique, 32 bytes, URL-safe base64)
  expires_at    : Timestamp DEFAULT now() + INTERVAL '7 days'
  accepted_at   : Timestamp?
  declined_at   : Timestamp?
  revoked_at    : Timestamp?                                    // Ã¢â€ Â NEW in v0.4.0
  created_at    : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(token)
  - INDEX(email)
  - INDEX(team_id, email)

Constraints:
  - role Ã¢â€°Â  'owner' (owners cannot be invited, only original creator)
  - Ã¢Ë†â€ž m Ã¢Ë†Ë† Membership : m.team_id = team_id Ã¢Ë†Â§ m.user_id = (SELECT id FROM User WHERE email = Invitation.email)
    (cannot invite existing team member)
  - invited_by Ã¢â€°Â  (SELECT id FROM User WHERE email = Invitation.email)
    (cannot invite self)

Derived:
  state Ã¢â€°Â¡
    IF accepted_at IS NOT NULL THEN 'accepted'
    ELSE IF declined_at IS NOT NULL THEN 'declined'
    ELSE IF revoked_at IS NOT NULL THEN 'revoked'
    ELSE IF expires_at < now() THEN 'expired'
    ELSE 'pending'

  is_actionable Ã¢â€°Â¡ (accepted_at IS NULL Ã¢Ë†Â§ declined_at IS NULL Ã¢Ë†Â§ revoked_at IS NULL Ã¢Ë†Â§ expires_at > now())
```

## 4.5 ApiKey

```
Entity: ApiKey
Description: Authentication credential for API access

Attributes:
  id            : UUID PK
  user_id       : UUID FK Ã¢â€ â€™ User (ON DELETE CASCADE)
  owner         : URN (scope of the key)
  name          : String (max 100) DEFAULT 'Default'
  key_prefix    : String (8 chars, e.g., "sk_live_")
  key_hash      : String! (unique, format: "hex(32-byte salt):hex(SHA-256(key + salt))")
  key_hash_prefix : String! (first 16 chars of SHA-256 hex, for O(1) lookup)
  scopes        : JSONB DEFAULT ["*"]
  last_used_at  : Timestamp?
  expires_at    : Timestamp?
  revoked_at    : Timestamp?
  created_at    : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(key_hash)
  - INDEX(user_id)
  - INDEX(owner)

Constraints:
  - (user.tier = 'starter') Ã¢â€ â€™ (owner = 'framecast:user:' + user.id)
  - (owner STARTS WITH 'framecast:team:' Ã¢Ë†Â¨ owner MATCHES 'framecast:tm_[^:]+:usr_') Ã¢â€ â€™ (user.tier = 'creator')

Derived:
  is_valid Ã¢â€°Â¡ (revoked_at IS NULL) Ã¢Ë†Â§ (expires_at IS NULL Ã¢Ë†Â¨ expires_at > now())
```

## 4.6 Project

```
Entity: Project
Description: A storyboard project containing spec and rendering generations

Attributes:
  id            : UUID PK
  team_id       : UUID FK Ã¢â€ â€™ Team (ON DELETE CASCADE)
  created_by    : UUID FK Ã¢â€ â€™ User
  name          : String (max 200)
  status        : {draft | rendering | completed | archived} DEFAULT draft
  spec          : JSONB (see Spec Schema in Appendix A)
  created_at    : Timestamp DEFAULT now()
  updated_at    : Timestamp DEFAULT now()

Indexes:
  - INDEX(team_id)
  - INDEX(created_by)
  - INDEX(status)
  - INDEX(team_id, updated_at DESC)

Triggers:
  - ON UPDATE: SET updated_at = now()
```

## 4.7 Generation

```
Entity: Generation
Description: A video generation, either ephemeral or project-based

Attributes:
  id              : UUID PK
  owner           : URN (determines visibility and storage quota)
  triggered_by    : UUID FK Ã¢â€ â€™ User (who created the generation)
  project_id      : UUID? FK Ã¢â€ â€™ Project (ON DELETE SET NULL)
                    -- NULL for ephemeral generations
                    -- Set for project-based generations
  status          : {queued | processing | completed | failed | canceled}
                    DEFAULT queued
  spec_snapshot   : JSONB (immutable copy of spec at generation creation)
  options         : JSONB DEFAULT {}
  progress        : JSONB DEFAULT {}
  output          : JSONB? (set on completion)
  output_size_bytes : BigInt? (set on completion)
  error           : JSONB? (set on failure)
  credits_charged : Integer DEFAULT 0
  failure_type    : {system | validation | timeout | canceled}?
                    -- Set when status becomes 'failed' or 'canceled'
                    -- system: Infrastructure/service error (refundable)
                    -- validation: Spec issue detected during generation
                    -- timeout: Generation exceeded max processing time
                    -- canceled: User-initiated cancellation
  credits_refunded : Integer DEFAULT 0
                    -- Credits returned to owner on failure/cancel
  idempotency_key : String? (unique per user, for duplicate prevention)
  started_at      : Timestamp?
  completed_at    : Timestamp?
  created_at      : Timestamp DEFAULT now()
  updated_at      : Timestamp DEFAULT now()

Indexes:
  - INDEX(owner)
  - INDEX(triggered_by)
  - INDEX(project_id)
  - INDEX(status)
  - INDEX(created_at DESC)
  - UNIQUE(triggered_by, idempotency_key) WHERE idempotency_key IS NOT NULL

Constraints:
  - (project_id IS NOT NULL) Ã¢â€ â€™ (owner STARTS WITH 'framecast:team:')
    (project generations are always team-owned)
  - status Ã¢Ë†Ë† {completed, failed, canceled} Ã¢â€ â€™ completed_at IS NOT NULL
    (terminal generations have completion timestamp)

Triggers:
  - ON UPDATE: SET updated_at = now()

Derived:
  is_ephemeral Ã¢â€°Â¡ (project_id IS NULL)
  is_terminal Ã¢â€°Â¡ status Ã¢Ë†Ë† {completed, failed, canceled}
  net_credits Ã¢â€°Â¡ credits_charged - credits_refunded
```

## 4.8 GenerationEvent

```
Entity: GenerationEvent
Description: Progress events emitted during generation execution (for SSE)

Attributes:
  id            : UUID PK
  generation_id : UUID FK Ã¢â€ â€™ Generation (ON DELETE CASCADE)
  sequence      : BigInt (monotonically increasing per generation)         // Ã¢â€ Â NEW in v0.4.0
  event_type    : {queued | started | progress | scene_complete | completed | failed | canceled}
  payload       : JSONB
  created_at    : Timestamp DEFAULT now()

Indexes:
  - INDEX(generation_id, created_at ASC)
  - INDEX(generation_id, sequence ASC)                                      // Ã¢â€ Â NEW in v0.4.0

Retention:
  - DELETE WHERE created_at < now() - INTERVAL '7 days'

SSE Protocol:                                                        // Ã¢â€ Â NEW in v0.4.0
  Event Format:
    id: {generation_id}:{sequence}
    event: {event_type}
    data: {payload as JSON}

  Reconnection:
    - Client sends Last-Event-ID header on reconnect
    - Server parses generation_id and sequence from Last-Event-ID
    - Server replays events WHERE generation_id = :generation_id AND sequence > :sequence
    - If sequence not found (expired), return HTTP 410 Gone
    - Client should then GET /v1/generations/:id to fetch current state
```

## 4.9 AssetFile

```
Entity: AssetFile
Description: Uploaded file (reference images, etc.). Managed explicitly by user.

Attributes:
  id            : UUID PK
  owner         : URN (determines visibility and storage quota)
  uploaded_by   : UUID FK Ã¢â€ â€™ User
  project_id    : UUID? FK Ã¢â€ â€™ Project (ON DELETE CASCADE)
                  -- NULL = not tied to specific project
                  -- Set = project-level asset
  filename      : String (max 255)
  s3_key        : String! (unique)
  content_type  : String (MIME type)
  size_bytes    : BigInt
  status        : {pending | ready | failed} DEFAULT pending        // Ã¢â€ Â NEW in v0.4.0
  metadata      : JSONB DEFAULT {}
  created_at    : Timestamp DEFAULT now()
  updated_at    : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(s3_key)
  - INDEX(owner)
  - INDEX(project_id)
  - INDEX(uploaded_by)                                               // Ã¢â€ Â NEW in v0.4.0

Triggers:
  - ON UPDATE: SET updated_at = now()

Constraints:
  - size_bytes > 0
  - size_bytes Ã¢â€°Â¤ 50 * 1024 * 1024 (50MB max)
  - content_type Ã¢Ë†Ë† {
      'image/jpeg', 'image/png', 'image/webp',           // images
      'audio/mpeg', 'audio/wav', 'audio/ogg',            // audio
      'video/mp4'                                         // video
    }
```

## 4.10 Webhook

```
Entity: Webhook
Description: HTTP callback registration for events

Attributes:
  id                : UUID PK
  team_id           : UUID FK Ã¢â€ â€™ Team (ON DELETE CASCADE)
  created_by        : UUID FK Ã¢â€ â€™ User
  url               : URL (max 2048)
  events            : String[] (e.g., ['generation.completed', 'generation.failed'])
  secret            : String (32 bytes, for HMAC signing)
  is_active         : Boolean DEFAULT true
  last_triggered_at : Timestamp?
  created_at        : Timestamp DEFAULT now()
  updated_at        : Timestamp DEFAULT now()

Indexes:
  - INDEX(team_id)
  - INDEX(team_id, is_active)

Constraints:
  - |events| > 0
  - url scheme Ã¢Ë†Ë† {'https'} (no plain HTTP)
  - Ã¢Ë†â‚¬ e Ã¢Ë†Ë† events : e Ã¢Ë†Ë† ValidWebhookEvents                           // Ã¢â€ Â NEW in v0.4.0
```

## 4.11 WebhookDelivery

```
Entity: WebhookDelivery
Description: Record of webhook delivery attempts

Attributes:
  id              : UUID PK
  webhook_id      : UUID FK Ã¢â€ â€™ Webhook (ON DELETE CASCADE)
  generation_id   : UUID? FK Ã¢â€ â€™ Generation (ON DELETE SET NULL)
  event_type      : String
  status          : {pending | retrying | delivered | failed} DEFAULT pending
  payload         : JSONB
  response_status : Integer?
  response_body   : String? (max 10KB, truncated)
  attempts        : Integer DEFAULT 0
  max_attempts    : Integer DEFAULT 5
  next_retry_at   : Timestamp?
  delivered_at    : Timestamp?
  created_at      : Timestamp DEFAULT now()

Indexes:
  - INDEX(webhook_id)
  - INDEX(status, next_retry_at) WHERE status = 'retrying'
  - INDEX(created_at)

Retention:
  - DELETE WHERE created_at < now() - INTERVAL '30 days'

Backoff Schedule:
  - Attempt 1: Immediate
  - Attempt 2: 1 minute
  - Attempt 3: 5 minutes
  - Attempt 4: 30 minutes
  - Attempt 5: 2 hours

Permanent Failure (no retry):
  - 4xx responses
```

## 4.12 Usage

```
Entity: Usage
Description: Aggregated usage metrics for billing

Attributes:
  id              : UUID PK
  owner           : URN (user or team)
  period          : String (format: 'YYYY-MM')
  renders_count   : Integer DEFAULT 0
  render_seconds  : Integer DEFAULT 0
  credits_used    : Integer DEFAULT 0
  credits_refunded: Integer DEFAULT 0                                   // Ã¢â€ Â NEW in v0.4.2
  api_calls       : Integer DEFAULT 0
  updated_at      : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(owner, period)
  - INDEX(period)

Derived:
  net_credits Ã¢â€°Â¡ credits_used - credits_refunded
```

## 4.13 SystemAsset Ã¢â€ Â NEW in v0.4.0

```
Entity: SystemAsset
Description: Pre-loaded assets available to all users. Read-only, managed by system.

Attributes:
  id              : String PK (format: asset_{category}_{name})
  category        : {sfx | ambient | music | transition}
  name            : String (display name)
  description     : String (max 500)
  duration_seconds: Decimal? (for audio assets)
  s3_key          : String! (unique)
  content_type    : String (MIME type)
  size_bytes      : BigInt
  tags            : String[] DEFAULT []
  created_at      : Timestamp DEFAULT now()

Indexes:
  - INDEX(category)
  - INDEX(tags) USING GIN

Constraints:
  - id MATCHES '^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$'
```

## 4.14 Conversation

```
Entity: Conversation
Description: A chat thread between a user and the LLM that produces artifacts

Attributes:
  id              : UUID PK
  user_id         : UUID FK → User (ON DELETE CASCADE)
  title           : String? (max 200)
  model           : String! (max 100, e.g. "claude-sonnet-4-20250514")
  system_prompt   : Text? (max 10,000)
  status          : {active | archived} DEFAULT active
  message_count   : Integer DEFAULT 0
  last_message_at : Timestamp?
  created_at      : Timestamp DEFAULT now()
  updated_at      : Timestamp DEFAULT now()

Indexes:
  - INDEX(user_id, status)
  - INDEX(user_id, last_message_at DESC)

Triggers:
  - ON UPDATE: SET updated_at = now()

Constraints:
  - system_prompt IS NULL OR LENGTH(system_prompt) <= 10000
  - message_count >= 0
```

## 4.15 Message

```
Entity: Message
Description: A single turn in a conversation (user or assistant)

Attributes:
  id              : UUID PK
  conversation_id : UUID FK → Conversation (ON DELETE CASCADE)
  role            : {user | assistant}
  content         : Text! (non-empty)
  artifacts       : JSONB? (array of ArtifactRef for assistant messages)
  model           : String? (max 100, assistant only)
  input_tokens    : Integer? (assistant only)
  output_tokens   : Integer? (assistant only)
  sequence        : Integer (monotonically increasing per conversation)
  created_at      : Timestamp DEFAULT now()

Indexes:
  - INDEX(conversation_id, sequence ASC)
  - UNIQUE(conversation_id, sequence)

Constraints:
  - sequence >= 1
  - LENGTH(TRIM(content)) > 0
```

## 4.16 Artifact

```
Entity: Artifact
Description: A creative output — storyboard spec, uploaded media, or generation output.
             Replaces AssetFile as the unified entity for all creative content.

Attributes:
  id              : UUID PK
  owner           : URN (determines visibility and storage quota)
  created_by      : UUID FK → User
  project_id      : UUID? FK → Project (ON DELETE CASCADE)
  kind            : {storyboard | image | audio | video | character}
  status          : {pending | ready | failed} DEFAULT pending
  source          : {upload | conversation | generation} DEFAULT upload
  filename        : String? (max 255, required for media)
  s3_key          : String? (unique, required for media)
  content_type    : String? (MIME type, required for media)
  size_bytes      : BigInt? (required for media, max 50MB)
  spec            : JSONB? (required for storyboard)
  conversation_id : UUID? FK → Conversation (ON DELETE SET NULL)
  source_generation_id : UUID? FK → Generation (ON DELETE SET NULL)
  metadata        : JSONB DEFAULT {}
  created_at      : Timestamp DEFAULT now()
  updated_at      : Timestamp DEFAULT now()

Indexes:
  - UNIQUE(s3_key) WHERE s3_key IS NOT NULL
  - INDEX(owner)
  - INDEX(project_id)
  - INDEX(created_by)
  - INDEX(kind)

Triggers:
  - ON UPDATE: SET updated_at = now()

Constraints:
  - Media kinds (image/audio/video) require filename, s3_key, content_type, size_bytes
  - Storyboard kind requires spec
  - source=conversation requires conversation_id
  - source=generation requires source_generation_id
  - project_id IS NOT NULL → owner STARTS WITH 'framecast:team:'
  - size_bytes > 0 AND size_bytes ≤ 50MB (when set)
  - content_type ∈ allowed list per kind
```
