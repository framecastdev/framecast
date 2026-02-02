# 9. API Permissions

## 9.1 Permission Matrix by User Tier

| Endpoint | Starter | Creator |
|----------|---------|---------|
| `POST /v1/generate` | ✓ | ✓ |
| `GET /v1/jobs` | ✓ (own jobs) | ✓ (accessible via owner URN) |
| `GET /v1/jobs/:id` | ✓ (own jobs) | ✓ (accessible via owner URN) |
| `GET /v1/jobs/:id/events` | ✓ (own jobs) | ✓ (accessible via owner URN) |
| `POST /v1/jobs/:id/cancel` | ✓ (own jobs) | ✓ (accessible via owner URN) |
| `POST /v1/jobs/:id/clone` | ✓ (own jobs) | ✓ (accessible jobs) |
| `DELETE /v1/jobs/:id` | ✓ (own ephemeral) | ✓ (accessible ephemeral) |
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
| `GET /v1/status` | ✓ | ✓ |

**v4.1 Additions:**

- `POST /v1/teams`: Creators can create new teams
- `POST /v1/jobs/:id/clone`: Starters can clone own jobs; Creators can clone accessible jobs

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
| Cancel jobs | ✓ | ✓ | ✓ (own) | ✗ |
| Clone jobs | ✓ | ✓ | ✓ | ✗ |
| View assets | ✓ | ✓ | ✓ | ✓ |
| Upload assets | ✓ | ✓ | ✓ | ✗ |
| Delete assets | ✓ | ✓ | ✓ (own) | ✗ |
| Manage webhooks | ✓ | ✓ | ✗ | ✗ |
| Manage API keys | ✓ | ✓ | ✓ (own) | ✗ |

---

## 9.3 API Key Scopes

```
Scope: generate
  Allows: POST /v1/generate

Scope: jobs:read
  Allows: GET /v1/jobs, GET /v1/jobs/:id, GET /v1/jobs/:id/events

Scope: jobs:write
  Allows: POST /v1/jobs/:id/cancel, POST /v1/jobs/:id/clone, DELETE /v1/jobs/:id

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

- `jobs:write` now includes `POST /v1/jobs/:id/clone` in addition to existing operations
- `team:admin` scope expanded to include `POST /v1/teams` (team creation)

---

## Scope Restrictions by Tier

| Tier | Allowed Scopes |
|------|----------------|
| Starter | `generate`, `jobs:read`, `jobs:write`, `assets:read`, `assets:write` |
| Creator | All scopes |

**Notes:**

- Starters cannot create API keys with `team:read` or `team:admin` scopes
- Creators can create API keys with any scope (subject to membership role permissions)
- The `jobs:write` scope now includes job cloning operations for both tiers (with tier-specific access constraints)
- The `team:admin` scope allows team creation only for Creator tier users
