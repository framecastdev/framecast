# 9. API Permissions

## 9.1 Permission Matrix by User Tier

| Endpoint | Starter | Creator |
|----------|---------|---------|
| `POST /v1/generations` | ✓ | ✓ |
| `GET /v1/generations` | ✓ (own generations) | ✓ (accessible via owner URN) |
| `GET /v1/generations/:id` | ✓ (own generations) | ✓ (accessible via owner URN) |
| `GET /v1/generations/:id/events` | ✓ (own generations) | ✓ (accessible via owner URN) |
| `POST /v1/generations/:id/cancel` | ✓ (own generations) | ✓ (accessible via owner URN) |
| `POST /v1/generations/:id/clone` | ✓ (own generations) | ✓ (accessible generations) |
| `DELETE /v1/generations/:id` | ✓ (own ephemeral) | ✓ (accessible ephemeral) |
| `GET /v1/account` | ✓ | ✓ |
| `PATCH /v1/account` | ✓ | ✓ |
| `* /v1/auth/keys` | ✓ | ✓ |
| `GET /v1/teams` | ✗ | ✓ |
| `GET /v1/teams/:id` | ✗ | ✓ |
| `PATCH /v1/teams/:id` | ✗ | ✓ |
| `POST /v1/teams` | ✗ | ✓ |
| `GET /v1/teams/:id/members` | ✗ | ✓ |
| `* /v1/teams/:id/members/*` | ✗ | ✓ |
| `GET /v1/teams/:id/invitations` | ✗ | ✓ |
| `POST /v1/teams/:id/invitations` | ✗ | ✓ |
| `DELETE /v1/teams/:id/invitations/:id` | ✗ | ✓ |
| `POST /v1/teams/:id/invitations/:id/resend` | ✗ | ✓ |
| `* /v1/projects/*` | ✗ | ✓ |
| `* /v1/webhooks/*` | ✗ | ✓ |
| `GET /v1/assets` | ✓ (own assets) | ✓ (accessible via owner URN) |
| `GET /v1/assets/:id` | ✓ (own assets) | ✓ (accessible via owner URN) |
| `POST /v1/assets/upload-url` | ✓ | ✓ |
| `POST /v1/assets/:id/confirm` | ✓ | ✓ |
| `DELETE /v1/assets/:id` | ✓ (own assets) | ✓ (accessible via owner URN) |
| `GET /v1/system-assets` | ✓ | ✓ |
| `GET /v1/system-assets/:id` | ✓ | ✓ |
| `POST /v1/spec/validate` | ✓ | ✓ |
| `POST /v1/spec/estimate` | ✓ | ✓ |
| `GET /v1/conversations` | ✓ (own) | ✓ (own) |
| `GET /v1/conversations/:id` | ✓ (own) | ✓ (own) |
| `POST /v1/conversations` | ✓ | ✓ |
| `PATCH /v1/conversations/:id` | ✓ (own) | ✓ (own) |
| `DELETE /v1/conversations/:id` | ✓ (own) | ✓ (own) |
| `POST /v1/conversations/:id/messages` | ✓ (own) | ✓ (own) |
| `GET /v1/conversations/:id/messages` | ✓ (own) | ✓ (own) |
| `GET /v1/artifacts` | ✓ (own artifacts) | ✓ (accessible via owner URN) |
| `GET /v1/artifacts/:id` | ✓ (own artifacts) | ✓ (accessible via owner URN) |
| `POST /v1/artifacts/storyboards` | ✓ | ✓ |
| `POST /v1/artifacts/characters` | ✓ | ✓ |
| `POST /v1/artifacts/upload-url` | ✓ | ✓ |
| `POST /v1/artifacts/:id/confirm` | ✓ | ✓ |
| `DELETE /v1/artifacts/:id` | ✓ (own artifacts) | ✓ (accessible via owner URN) |
| `GET /v1/status` | ✓ | ✓ |

**v4.1 Additions:**

- `POST /v1/teams`: Creators can create new teams
- `POST /v1/generations/:id/clone`: Starters can clone own generations; Creators can clone accessible generations

---

## 9.2 Permission Matrix by Membership Role

| Operation | Owner | Admin | Member | Viewer |
|-----------|-------|-------|--------|--------|
| View team | ✓ | ✓ | ✓ | ✓ |
| Edit team settings | ✓ | ✓ | ✗ | ✗ |
| Delete team | ✓ (if sole member) | ✗ | ✗ | ✗ |
| View members | ✓ | ✓ | ✓ | ✓ |
| Invite members | ✓ | ✓ | ✗ | ✗ |
| Remove members | ✓ | ✓ (not owner) | ✗ | ✗ |
| Change member roles | ✓ | ✓ (not to owner) | ✗ | ✗ |
| View invitations | ✓ | ✓ | ✗ | ✗ |
| Revoke invitations | ✓ | ✓ | ✗ | ✗ |
| Resend invitations | ✓ | ✓ | ✗ | ✗ |
| View projects | ✓ | ✓ | ✓ | ✓ |
| Create projects | ✓ | ✓ | ✓ | ✗ |
| Edit projects | ✓ | ✓ | ✓ | ✗ |
| Delete projects | ✓ | ✓ | ✗ | ✗ |
| Archive projects | ✓ | ✓ | ✗ | ✗ |
| Unarchive projects | ✓ | ✓ | ✗ | ✗ |
| Trigger render | ✓ | ✓ | ✓ | ✗ |
| Cancel generations | ✓ | ✓ | ✓ (own) | ✗ |
| Clone generations | ✓ | ✓ | ✓ | ✗ |
| View assets | ✓ | ✓ | ✓ | ✓ |
| Upload assets | ✓ | ✓ | ✓ | ✗ |
| Delete assets | ✓ | ✓ | ✓ (own) | ✗ |
| Manage webhooks | ✓ | ✓ | ✗ | ✗ |
| View artifacts | ✓ | ✓ | ✓ | ✓ |
| Create artifacts | ✓ | ✓ | ✓ | ✗ |
| Delete artifacts | ✓ | ✓ | ✓ (own) | ✗ |
| Manage API keys | ✓ | ✓ | ✓ (own) | ✗ |

---

## 9.3 API Key Scopes

```
Scope: generate
  Allows: POST /v1/generations

Scope: generations:read
  Allows: GET /v1/generations, GET /v1/generations/:id, GET /v1/generations/:id/events

Scope: generations:write
  Allows: POST /v1/generations/:id/cancel, POST /v1/generations/:id/clone, DELETE /v1/generations/:id

Scope: assets:read
  Allows: GET /v1/assets, GET /v1/assets/:id

Scope: assets:write
  Allows: POST /v1/assets/upload-url, POST /v1/assets/:id/confirm, DELETE /v1/assets/:id

Scope: projects:read
  Allows: GET /v1/projects/*, GET /v1/teams/:id/projects/*

Scope: projects:write
  Allows: POST/PUT/PATCH/DELETE /v1/projects/*

Scope: team:read
  Allows: GET /v1/teams, GET /v1/teams/:id, GET /v1/teams/:id/members

Scope: team:admin
  Allows: All team management operations including POST /v1/teams
  (members, invitations, settings, and team creation)

Scope: conversations:read
  Allows: GET /v1/conversations, GET /v1/conversations/:id, GET /v1/conversations/:id/messages

Scope: conversations:write
  Allows: POST /v1/conversations, PATCH /v1/conversations/:id,
          DELETE /v1/conversations/:id, POST /v1/conversations/:id/messages

Scope: artifacts:read
  Allows: GET /v1/artifacts, GET /v1/artifacts/:id

Scope: artifacts:write
  Allows: POST /v1/artifacts/storyboards, POST /v1/artifacts/characters,
          POST /v1/artifacts/upload-url, POST /v1/artifacts/:id/confirm,
          DELETE /v1/artifacts/:id

Scope: webhooks:read
  Allows: GET /v1/teams/:id/webhooks, GET /v1/webhooks/:id, GET /v1/webhooks/:id/deliveries

Scope: webhooks:write
  Allows: POST /v1/teams/:id/webhooks, PATCH /v1/webhooks/:id, DELETE /v1/webhooks/:id,
          POST /v1/webhooks/:id/rotate-secret, POST /v1/webhooks/:id/test,
          POST /v1/webhook-deliveries/:id/retry

Scope: * (wildcard)
  Allows: All operations user's role permits
```

**v4.1 Updates:**

- `generations:write` now includes `POST /v1/generations/:id/clone` in addition to existing operations
- `team:admin` scope expanded to include `POST /v1/teams` (team creation)

---

## Scope Restrictions by Tier

| Tier | Allowed Scopes |
|------|----------------|
| Starter | `generate`, `generations:read`, `generations:write`, `assets:read`, `assets:write`, `artifacts:read`, `artifacts:write`, `conversations:read`, `conversations:write` |
| Creator | All scopes |

**Notes:**

- Starters cannot create API keys with `team:read` or `team:admin` scopes
- Creators can create API keys with any scope (subject to membership role permissions)
- The `generations:write` scope now includes generation cloning operations for both tiers (with tier-specific access constraints)
- The `team:admin` scope allows team creation only for Creator tier users
