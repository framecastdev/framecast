# Section 8: Operations

**Note:** This section defines all API operations with pre/post conditions.

---

## 8.9 Webhook Operations Ã¢â€ Â MISSING

```
Operation: list_webhooks(team_id: UUID, user_id: UUID) Ã¢â€ â€™ Webhook[]
  Pre:  Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: Returns all webhooks WHERE team_id = team_id
        Ordered by created_at DESC
        Secret field is NOT returned in list

Operation: get_webhook(webhook_id: UUID, user_id: UUID) Ã¢â€ â€™ Webhook
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: Returns webhook
        Secret field is NOT returned (use rotate_webhook_secret to get new secret)

Operation: create_webhook(team_id: UUID, user_id: UUID, params: WebhookParams) Ã¢â€ â€™ {webhook: Webhook, secret: String}
  Pre:  Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
        Ã¢Ë†Â§ valid_https_url(params.url)
        Ã¢Ë†Â§ |params.events| > 0
        Ã¢Ë†Â§ Ã¢Ë†â‚¬ e Ã¢Ë†Ë† params.events : e Ã¢Ë†Ë† ValidWebhookEvents
  Post: Webhook created with:
          id = uuid()
          team_id = team_id
          created_by = user_id
          url = params.url
          events = params.events
          secret = generate_secret(32)
          is_active = true
        Ã¢Ë†Â§ Raw secret returned (only time it's visible)

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

Operation: update_webhook(webhook_id: UUID, user_id: UUID, updates: WebhookUpdates) Ã¢â€ â€™ Webhook
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
        Ã¢Ë†Â§ (updates.url IS NULL Ã¢Ë†Â¨ valid_https_url(updates.url))
        Ã¢Ë†Â§ (updates.events IS NULL Ã¢Ë†Â¨ (|updates.events| > 0 Ã¢Ë†Â§ Ã¢Ë†â‚¬ e Ã¢Ë†Ë† updates.events : e Ã¢Ë†Ë† ValidWebhookEvents))
  Post: Webhook updated with provided fields
        Ã¢Ë†Â§ w.updated_at = now()

  WebhookUpdates:
    url?: URL (HTTPS only, max 2048)
    events?: String[]
    is_active?: Boolean

Operation: delete_webhook(webhook_id: UUID, user_id: UUID) Ã¢â€ â€™ void
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: Webhook deleted (cascades to WebhookDelivery)

Operation: rotate_webhook_secret(webhook_id: UUID, user_id: UUID) Ã¢â€ â€™ {webhook: Webhook, secret: String}
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: w.secret = generate_secret(32)
        Ã¢Ë†Â§ w.updated_at = now()
        Ã¢Ë†Â§ New raw secret returned

Operation: test_webhook(webhook_id: UUID, user_id: UUID) Ã¢â€ â€™ WebhookDelivery
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id Ã¢Ë†Â§ w.is_active = true
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: Test delivery created and attempted immediately
        Event type = 'webhook.test'
        Returns delivery result (status, response_status, response_body)

Operation: list_webhook_deliveries(webhook_id: UUID, user_id: UUID, filters: DeliveryFilters?) Ã¢â€ â€™ Page<WebhookDelivery>
  Pre:  Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: Returns deliveries matching filters, ordered by created_at DESC

  DeliveryFilters:
    status?: {pending | retrying | delivered | failed}
    event_type?: String
    created_after?: Timestamp
    created_before?: Timestamp
    limit?: Integer (1-100, default 20)
    cursor?: String

Operation: retry_webhook_delivery(delivery_id: UUID, user_id: UUID) Ã¢â€ â€™ WebhookDelivery
  Pre:  Ã¢Ë†Æ’ d Ã¢Ë†Ë† WebhookDelivery : d.id = delivery_id Ã¢Ë†Â§ d.status = 'failed'
        Ã¢Ë†Â§ Ã¢Ë†Æ’ w Ã¢Ë†Ë† Webhook : w.id = d.webhook_id
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = w.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: d.status = 'pending'
        Ã¢Ë†Â§ d.attempts = 0
        Ã¢Ë†Â§ d.next_retry_at = now()
        Ã¢Ë†Â§ Delivery will be attempted immediately
```

---

## 8.10 API Key Operations Ã¢â€ Â MISSING

```
Operation: list_api_keys(user_id: UUID) Ã¢â€ â€™ ApiKey[]
  Pre:  Ã¢Ë†Æ’ u Ã¢Ë†Ë† User : u.id = user_id
  Post: Returns all API keys WHERE user_id = user_id
        Ordered by created_at DESC
        key_hash is NOT returned
        Only key_prefix is visible (e.g., "sk_live_abc...")

Operation: get_api_key(key_id: UUID, user_id: UUID) Ã¢â€ â€™ ApiKey
  Pre:  Ã¢Ë†Æ’ k Ã¢Ë†Ë† ApiKey : k.id = key_id Ã¢Ë†Â§ k.user_id = user_id
  Post: Returns API key details
        key_hash is NOT returned

Operation: create_api_key(user_id: UUID, params: ApiKeyParams) Ã¢â€ â€™ {api_key: ApiKey, raw_key: String}
  Pre:  Ã¢Ë†Æ’ u Ã¢Ë†Ë† User : u.id = user_id
        Ã¢Ë†Â§ (params.name IS NULL Ã¢Ë†Â¨ |params.name| Ã¢â€°Â¤ 100)
        Ã¢Ë†Â§ (params.owner IS NULL Ã¢Ë†Â¨ user_can_use_owner_urn(user_id, params.owner))
        Ã¢Ë†Â§ (params.scopes IS NULL Ã¢Ë†Â¨ (
            Ã¢Ë†â‚¬ s Ã¢Ë†Ë† params.scopes : s Ã¢Ë†Ë† AllowedScopes
            Ã¢Ë†Â§ (u.tier = 'creator' Ã¢Ë†Â¨ s Ã¢Ë†Ë† StarterAllowedScopes)
          ))
        Ã¢Ë†Â§ (params.expires_at IS NULL Ã¢Ë†Â¨ params.expires_at > now())
  Post: ApiKey created with:
          id = uuid()
          user_id = user_id
          owner = params.owner ?? 'framecast:user:' || user_id
          name = params.name ?? 'Default'
          key_prefix = 'sk_live_' + random(4)
          key_hash = sha256(raw_key)
          scopes = params.scopes ?? ['*']
          expires_at = params.expires_at
        Ã¢Ë†Â§ Raw key returned (ONLY TIME it's visible)
        Ã¢Ë†Â§ Raw key format: sk_live_XXXXXXXX_YYYYYYYYYYYYYYYYYYYYYYYYYYYY

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

Operation: update_api_key(key_id: UUID, user_id: UUID, updates: ApiKeyUpdates) Ã¢â€ â€™ ApiKey
  Pre:  Ã¢Ë†Æ’ k Ã¢Ë†Ë† ApiKey : k.id = key_id Ã¢Ë†Â§ k.user_id = user_id Ã¢Ë†Â§ k.revoked_at IS NULL
        Ã¢Ë†Â§ (updates.name IS NULL Ã¢Ë†Â¨ |updates.name| Ã¢â€°Â¤ 100)
  Post: API key updated with provided fields

  ApiKeyUpdates:
    name?: String (max 100)

  Note: scopes and owner cannot be modified after creation.
        To change scopes/owner, revoke and create new key.

Operation: revoke_api_key(key_id: UUID, user_id: UUID) Ã¢â€ â€™ void
  Pre:  Ã¢Ë†Æ’ k Ã¢Ë†Ë† ApiKey : k.id = key_id Ã¢Ë†Â§ k.user_id = user_id Ã¢Ë†Â§ k.revoked_at IS NULL
  Post: k.revoked_at = now()
        Ã¢Ë†Â§ Key immediately becomes invalid
        Ã¢Ë†Â§ Key record preserved for audit (30 days retention)
```

---

## 8.11 Project Archive Operations Ã¢â€ Â MISSING

```
Operation: archive_project(project_id: UUID, user_id: UUID) Ã¢â€ â€™ Project
  Pre:  Ã¢Ë†Æ’ p Ã¢Ë†Ë† Project : p.id = project_id Ã¢Ë†Â§ p.status Ã¢Ë†Ë† {draft, completed}
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = p.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: p.status = 'archived'
        Ã¢Ë†Â§ p.updated_at = now()

  Notes:
    - Cannot archive a project that is currently rendering
    - Archived projects are hidden from default list view
    - Jobs and assets associated with project are preserved

Operation: unarchive_project(project_id: UUID, user_id: UUID) Ã¢â€ â€™ Project
  Pre:  Ã¢Ë†Æ’ p Ã¢Ë†Ë† Project : p.id = project_id Ã¢Ë†Â§ p.status = 'archived'
        Ã¢Ë†Â§ Ã¢Ë†Æ’ m Ã¢Ë†Ë† Membership : m.team_id = p.team_id Ã¢Ë†Â§ m.user_id = user_id Ã¢Ë†Â§ m.role Ã¢Ë†Ë† {owner, admin}
  Post: p.status = 'draft'
        Ã¢Ë†Â§ p.updated_at = now()

  Notes:
    - Unarchived projects return to draft status
    - Previous completion status is not preserved
    - User can re-render after unarchiving
```

---

## 8.12 Estimation & Validation Operations

```
Operation: validate_spec(spec: JSONB, user_id: UUID) â†’ ValidationResponse
  Pre:  âˆƒ u âˆˆ User : u.id = user_id
        âˆ§ spec IS NOT NULL
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
    - Size limits (spec â‰¤ 100KB, scenes â‰¤ 50, etc.)
    - Field limits (prompt â‰¤ 2000 chars, duration 1-30s, etc.)
    - Reference integrity (timeline â†’ scenes, symbols, transitions)
    - Asset existence (for user assets, checks status = 'ready')
    - System asset validity (checks against catalog)

  Notes:
    - Does NOT consume credits
    - Does NOT check credit balance
    - Validates spec structure and references only
    - Asset ownership validation uses user_id context

Operation: estimate_spec(spec: JSONB, user_id: UUID, owner?: URN) â†’ EstimateResponse
  Pre:  âˆƒ u âˆˆ User : u.id = user_id
        âˆ§ validate_spec(spec, user_id).valid = true
        âˆ§ (owner IS NULL âˆ¨ user_can_use_owner_urn(user_id, owner))
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
    - Estimate is best-effort, actual may vary Â±10%
    - If owner provided, validates credit availability
```

---

## 8.13 Endpoint Mapping Table

| Operation | HTTP Method | Endpoint |
|-----------|-------------|----------|
| **User** | | |
| signup | POST | /v1/auth/signup |
| upgrade | POST | /v1/account/upgrade |
| update_profile | PATCH | /v1/account |
| delete_user | DELETE | /v1/account |
| **Team** | | |
| list_teams | GET | /v1/teams |
| get_team | GET | /v1/teams/:id |
| create_team | POST | /v1/teams |
| update_team | PATCH | /v1/teams/:id |
| delete_team | DELETE | /v1/teams/:id |
| list_members | GET | /v1/teams/:id/members |
| update_member_role | PATCH | /v1/teams/:id/members/:user_id |
| remove_member | DELETE | /v1/teams/:id/members/:user_id |
| leave_team | POST | /v1/teams/:id/leave |
| **Invitation** | | |
| list_invitations | GET | /v1/teams/:id/invitations |
| create_invitation | POST | /v1/teams/:id/invitations |
| revoke_invitation | DELETE | /v1/teams/:id/invitations/:id |
| resend_invitation | POST | /v1/teams/:id/invitations/:id/resend |
| accept_invitation | POST | /v1/invitations/accept |
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
| estimate_spec | POST | /v1/spec/estimate |
| validate_spec | POST | /v1/spec/validate |
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

---

**Document Version: 0.0.1-SNAPSHOT
**Last Updated**: 2025-01-30
