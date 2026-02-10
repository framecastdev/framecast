---
name: api-spec
description: Formal API specification reference. Use when implementing API operations, checking entity definitions, understanding permissions, or validating business rules.
---

# API Specification Reference

The formal specification is attached to the project. Key files:

| File | Contents |
|------|----------|
| `04_Entities.md` | All entity definitions with fields |
| `05_Relationships_States.md` | State machines, ER relationships |
| `06_Invariants.md` | Business rules, constraints |
| `07_Operations.md` | API operations with pre/post conditions |
| `08_Permissions.md` | Role/tier permission matrix |
| `09_Validation.md` | Spec validation, webhook payloads |
| `11_Storage.md` | S3 structure, credit source rules, refund policy |

## Entity Quick Reference

### User

```
id, email!, name?, avatar_url?, tier {starter|creator},
credits, ephemeral_storage_bytes, upgraded_at?, created_at, updated_at
```

### Team

```
id, name, slug!, credits, ephemeral_storage_bytes, settings, created_at, updated_at
```

### Membership

```
id, team_id FK, user_id FK, role {owner|admin|member|viewer}, created_at
UNIQUE(team_id, user_id)
```

### Generation

```
id, owner URN, triggered_by FK→User, project_id? FK→Project,
status {queued|processing|completed|failed|canceled},
spec_snapshot JSONB, options, progress, output?, error?,
credits_charged, failure_type?, credits_refunded,
idempotency_key?, started_at?, completed_at?, created_at, updated_at
```

### Project

```
id, team_id FK, created_by FK→User, name,
status {draft|rendering|completed|archived}, spec JSONB,
created_at, updated_at
```

### AssetFile

```
id, owner URN, uploaded_by FK→User, project_id? FK→Project,
filename, s3_key!, content_type, size_bytes,
status {pending|ready|failed}, metadata JSONB,
created_at, updated_at
```

### Invitation

```
id, team_id FK, invited_by FK→User, email,
role {admin|member|viewer}, token!, expires_at,
accepted_at?, revoked_at?, created_at
Derived: state ∈ {pending|accepted|expired|revoked}
```

## URN Patterns

| Pattern | Example | Who Can Use |
|---------|---------|-------------|
| `framecast:user:<user_id>` | `framecast:user:usr_abc` | Starter or Creator |
| `framecast:team:<team_id>` | `framecast:team:tm_xyz` | Creator only |
| `framecast:<team_id>:<user_id>` | `framecast:tm_xyz:usr_abc` | Creator only |

## Key Invariants

**User Invariants:**

- **INV-U3**: Starter users have no team memberships
- **INV-U5**: User credits ≥ 0

**Team Invariants:**

- **INV-T1**: Every team has ≥1 member
- **INV-T2**: Every team has ≥1 owner
- **INV-T6**: Team credits ≥ 0

**Membership:**

- **INV-M2**: Role ∈ {owner, admin, member, viewer}
- **INV-M4**: Only creator users can have memberships

**Generation:**

- **INV-J1**: Status ∈ {queued, processing, completed, failed, canceled}
- **INV-J6**: Failed/canceled generations must have failure_type
- **INV-J8**: credits_refunded ≤ credits_charged
- **INV-J11**: Project generations must be team-owned
- **INV-J12**: Max 1 active generation per project

**Cardinality Constraints:**

- **CARD-2**: Max 10 owned teams per user
- **CARD-3**: Max 50 team memberships per user
- **CARD-4**: Max 50 pending invitations per team
- **CARD-5**: Max 5 concurrent generations per team
- **CARD-6**: Max 1 concurrent generation per starter user

**Rate Limits:**

- Starter: 60 RPM, Creator: 300 RPM
- Invitation rate: 20 per day per team

## Permission Matrix (Brief)

| Operation | Owner | Admin | Member | Viewer |
|-----------|-------|-------|--------|--------|
| Edit team settings | ✓ | ✓ | ✗ | ✗ |
| Invite members | ✓ | ✓ | ✗ | ✗ |
| Create projects | ✓ | ✓ | ✓ | ✗ |
| Delete projects | ✓ | ✓ | ✗ | ✗ |
| Trigger render | ✓ | ✓ | ✓ | ✗ |
| Cancel generations | ✓ | ✓ | Own only | ✗ |
| Manage webhooks | ✓ | ✓ | ✗ | ✗ |

## State Machines

### Generation.status

```
queued → processing → completed
           ↓
         failed

queued/processing → canceled (user action)
```

### Project.status

```
draft → rendering → completed → archived
  ↑        ↓                       ↓
  └── (generation failed) ←────────┘ (unarchive)
```

### Invitation (derived)

```
pending → accepted (user accepts)
        → expired (expires_at reached)
        → revoked (admin action)
```

## Credit Refund Policy

| Failure Type | Refund |
|--------------|--------|
| `system` | 100% |
| `timeout` | 100% |
| `validation` | Proportional to remaining work |
| `canceled` | Proportional × 0.9 (10% fee) |

## Webhook Events

```
generation.queued, generation.started, generation.progress,
generation.completed, generation.failed, generation.canceled
```

Signature: `HMAC-SHA256(timestamp + "." + body, secret)`

## Endpoint Mapping (Key Operations)

| Operation | Method | Endpoint |
|-----------|--------|----------|
| **Generations** | | |
| Create ephemeral generation | POST | `/v1/generations` |
| Create project generation | POST | `/v1/projects/:id/render` |
| Get generation | GET | `/v1/generations/:id` |
| List generations | GET | `/v1/generations` |
| Cancel generation | POST | `/v1/generations/:id/cancel` |
| Clone generation | POST | `/v1/generations/:id/clone` |
| Get generation events (SSE) | GET | `/v1/generations/:id/events` |
| **Projects** | | |
| List projects | GET | `/v1/teams/:id/projects` |
| Create project | POST | `/v1/teams/:id/projects` |
| Get project | GET | `/v1/projects/:id` |
| Update project | PATCH | `/v1/projects/:id` |
| Archive project | POST | `/v1/projects/:id/archive` |
| Unarchive project | POST | `/v1/projects/:id/unarchive` |
| **Teams** | | |
| List teams | GET | `/v1/teams` |
| Create team | POST | `/v1/teams` |
| Get team | GET | `/v1/teams/:id` |
| List members | GET | `/v1/teams/:id/members` |
| **Invitations** | | |
| List invitations | GET | `/v1/teams/:id/invitations` |
| Create invitation | POST | `/v1/teams/:id/invitations` |
| Accept invitation | POST | `/v1/invitations/accept` |
| Revoke invitation | DELETE | `/v1/teams/:id/invitations/:id` |
| **Assets** | | |
| Create upload URL | POST | `/v1/assets/upload-url` |
| Confirm upload | POST | `/v1/assets/:id/confirm` |
| List assets | GET | `/v1/assets` |
| Delete asset | DELETE | `/v1/assets/:id` |
| **Webhooks** | | |
| Create webhook | POST | `/v1/teams/:id/webhooks` |
| List webhooks | GET | `/v1/teams/:id/webhooks` |
| Rotate secret | POST | `/v1/webhooks/:id/rotate-secret` |
| **Validation** | | |
| Validate spec | POST | `/v1/spec/validate` |
| Estimate credits | POST | `/v1/spec/estimate` |
