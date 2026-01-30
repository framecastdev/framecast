# Section 6: Invariants & Constraints

**Note:** System-wide constraints that must always hold true.

---

## 7.1 User Invariants

```
INV-U1: âˆ€ u âˆˆ User : u.tier = 'creator' â†” u.upgraded_at IS NOT NULL
        (Creator users have upgrade timestamp)

INV-U2: âˆ€ u âˆˆ User : u.tier = 'creator' â†’
        |{m âˆˆ Membership : m.user_id = u.id}| â‰¥ 1
        (Creator must belong to at least one team)

INV-U3: âˆ€ u âˆˆ User : u.tier = 'starter' â†’
        âˆ„ m âˆˆ Membership : m.user_id = u.id
        (Starter users have no team memberships)

INV-U4: âˆ€ u âˆˆ User : u.tier âˆˆ {'starter', 'creator'}
        (Tier must be one of allowed values)

INV-U5: âˆ€ u âˆˆ User : u.credits â‰¥ 0
        (Credits cannot be negative)

INV-U6: âˆ€ u âˆˆ User : u.ephemeral_storage_bytes â‰¥ 0
        (Storage cannot be negative)
```

## 7.2 Team Invariants

```
INV-T1: âˆ€ t âˆˆ Team :
        |{m âˆˆ Membership : m.team_id = t.id}| â‰¥ 1
        (Every team has at least one member)

INV-T2: âˆ€ t âˆˆ Team :
        |{m âˆˆ Membership : m.team_id = t.id âˆ§ m.role = 'owner'}| â‰¥ 1
        (Every team has at least one owner)

INV-T3: âˆ€ t1, t2 âˆˆ Team : t1 â‰  t2 â†’ t1.slug â‰  t2.slug
        (Team slugs are unique)

INV-T4: âˆ€ t âˆˆ Team : t.slug MATCHES '^[a-z0-9][a-z0-9-]*[a-z0-9]$|^[a-z0-9]$'
        (Slug format: lowercase alphanumeric with hyphens, no leading/trailing hyphen)

INV-T5: âˆ€ t âˆˆ Team : t.created_at â‰¤ t.updated_at
        (Creation timestamp precedes update timestamp)

INV-T6: âˆ€ t âˆˆ Team : t.credits â‰¥ 0
        (Team credits cannot be negative)

INV-T7: âˆ€ u âˆˆ User :                                                    // â† NEW in v0.4.1
        |{t âˆˆ Team : âˆƒ m âˆˆ Membership : m.team_id = t.id âˆ§ m.user_id = u.id âˆ§ m.role = 'owner'}| â‰¤ 10
        (User cannot own more than MAX_OWNED_TEAMS=10 teams)

INV-T8: âˆ€ u âˆˆ User :                                                    // â† NEW in v0.4.1
        |{m âˆˆ Membership : m.user_id = u.id}| â‰¤ 50
        (User cannot be member of more than MAX_TEAM_MEMBERSHIPS=50 teams)
```

## 7.3 Membership Invariants

```
INV-M1: âˆ€ m âˆˆ Membership :
        m.user_id IS NOT NULL âˆ§ m.team_id IS NOT NULL
        (All memberships must reference both user and team)

INV-M2: âˆ€ m âˆˆ Membership : m.role âˆˆ {'owner', 'admin', 'member', 'viewer'}
        (Role must be one of the four allowed values)

INV-M3: âˆ€ m1, m2 âˆˆ Membership : (m1.user_id = m2.user_id âˆ§ m1.team_id = m2.team_id) â†’ m1 = m2
        (User-team pair uniqueness; no duplicate memberships)

INV-M4: âˆ€ m âˆˆ Membership :
        (SELECT tier FROM User WHERE id = m.user_id) = 'creator'
        (Only creator users can have memberships)

INV-M5: âˆ€ m âˆˆ Membership : m.user_id âˆˆ {u.id : u âˆˆ User}
        (Membership user reference must exist)

INV-M6: âˆ€ m âˆˆ Membership : m.team_id âˆˆ {t.id : t âˆˆ Team}
        (Membership team reference must exist)
```

## 7.4 Invitation Invariants

```
INV-I1: âˆ€ i âˆˆ Invitation :
        i.team_id IS NOT NULL âˆ§ i.email IS NOT NULL
        (All invitations must reference team and have email)

INV-I2: âˆ€ i âˆˆ Invitation : i.role âˆˆ {'admin', 'member', 'viewer'}
        (Invitation role excludes 'owner' - owners created only on team creation)

INV-I3: âˆ€ i âˆˆ Invitation : i.accepted_at IS NOT NULL â†’ i.revoked_at IS NULL
        (Accepted invitations cannot be revoked)

INV-I4: âˆ€ i âˆˆ Invitation : i.revoked_at IS NOT NULL â†’ i.accepted_at IS NULL
        (Revoked invitations cannot be accepted)

INV-I5: âˆ€ i âˆˆ Invitation : i.team_id âˆˆ {t.id : t âˆˆ Team}
        (Invitation team reference must exist)

INV-I6: âˆ€ i âˆˆ Invitation : i.invited_by âˆˆ {u.id : u âˆˆ User}
        (Invitation inviter reference must exist)

INV-I7: âˆ€ i âˆˆ Invitation :
        i.invited_by â‰  (SELECT id FROM User WHERE email = i.email)
        (Cannot invite self)

INV-I8: âˆ€ i âˆˆ Invitation :
        i.is_actionable = true â†’
        âˆ„ m âˆˆ Membership : m.team_id = i.team_id âˆ§ m.user_id = (SELECT id FROM User WHERE email = i.email)
        (Actionable invitation cannot exist for existing team member)

INV-I9: âˆ€ i âˆˆ Invitation : i.created_at < i.expires_at
        (Expiration must be after creation)
```

## 7.5 Job Invariants

```
INV-J1: âˆ€ j âˆˆ Job : j.status âˆˆ {'queued', 'processing', 'completed', 'failed', 'canceled'}
        (Job status must be one of allowed values)

INV-J2: âˆ€ j âˆˆ Job : j.status âˆˆ {'completed', 'failed', 'canceled'} â†’
        j.completed_at IS NOT NULL âˆ§ j.completed_at â‰¥ j.created_at
        (Terminal jobs have completion timestamp after creation)

INV-J3: âˆ€ j âˆˆ Job : j.status = 'processing' â†’ j.started_at IS NOT NULL
        (Processing jobs have start timestamp)

INV-J4: âˆ€ j âˆˆ Job : j.status = 'completed' â†’ j.output IS NOT NULL
        (Completed jobs must have output)

INV-J5: âˆ€ j âˆˆ Job : j.status = 'failed' â†’ j.error IS NOT NULL
        (Failed jobs must have error details)

INV-J6: âˆ€ j âˆˆ Job :                                                     // â† NEW in v0.4.1
        j.status âˆˆ {'failed', 'canceled'} â†’ j.failure_type IS NOT NULL
        (Failed/canceled jobs must have failure_type)

INV-J7: âˆ€ j âˆˆ Job :                                                     // â† NEW in v0.4.1
        j.status = 'completed' â†’ j.failure_type IS NULL
        (Completed jobs must not have failure_type)

INV-J8: âˆ€ j âˆˆ Job :                                                     // â† NEW in v0.4.1
        j.credits_refunded â‰¤ j.credits_charged
        (Cannot refund more than charged)

INV-J9: âˆ€ j âˆˆ Job :                                                     // â† NEW in v0.4.1
        j.credits_refunded â‰¥ 0 âˆ§ j.credits_charged â‰¥ 0
        (Credits values cannot be negative)

INV-J10: âˆ€ j âˆˆ Job : j.created_at â‰¤ j.updated_at
        (Creation timestamp precedes update timestamp)

INV-J11: âˆ€ j âˆˆ Job :
        (j.project_id IS NOT NULL) â†’ (j.owner STARTS WITH 'splice:team:')
        (Project-based jobs must be team-owned)

INV-J12: âˆ€ p âˆˆ Project :
        |{j âˆˆ Job : j.project_id = p.id âˆ§ j.status âˆˆ {'queued', 'processing'}}| â‰¤ 1
        (At most one active job per project)

INV-J13: âˆ€ j âˆˆ Job : j.triggered_by âˆˆ {u.id : u âˆˆ User}
        (Job triggered_by reference must exist)
```

## 7.6 ApiKey Invariants

```
INV-A1: âˆ€ k âˆˆ ApiKey : k.user_id âˆˆ {u.id : u âˆˆ User}
        (API key user reference must exist)

INV-A2: âˆ€ k âˆˆ ApiKey : k.key_hash IS NOT NULL âˆ§ LENGTH(k.key_hash) > 0
        (All API keys must have non-empty hash)

INV-A3: âˆ€ k1, k2 âˆˆ ApiKey : k1 â‰  k2 â†’ k1.key_hash â‰  k2.key_hash
        (Key hashes are unique)

INV-A4: âˆ€ k âˆˆ ApiKey : k.revoked_at IS NOT NULL â†’ k.is_valid = false
        (Revoked keys are not valid)

INV-A5: âˆ€ k âˆˆ ApiKey : k.expires_at IS NOT NULL âˆ§ k.expires_at < now() â†’ k.is_valid = false
        (Expired keys are not valid)

INV-A6: âˆ€ k âˆˆ ApiKey :
        (SELECT tier FROM User WHERE id = k.user_id) = 'starter' â†’
        k.owner = 'splice:user:' || k.user_id
        (Starter user keys must be personal URN)

INV-A7: âˆ€ k âˆˆ ApiKey :
        (k.owner STARTS WITH 'splice:team:' âˆ¨ k.owner MATCHES 'splice:tm_[^:]+:usr_') â†’
        (SELECT tier FROM User WHERE id = k.user_id) = 'creator'
        (Team/membership URN keys require creator tier)
```

## 7.7 Project Invariants

```
INV-P1: âˆ€ p âˆˆ Project : p.status âˆˆ {'draft', 'rendering', 'completed', 'archived'}
        (Project status must be one of allowed values)

INV-P2: âˆ€ p âˆˆ Project : p.team_id âˆˆ {t.id : t âˆˆ Team}
        (Project team reference must exist)

INV-P3: âˆ€ p âˆˆ Project : p.created_by âˆˆ {u.id : u âˆˆ User}
        (Project creator reference must exist)

INV-P4: âˆ€ p âˆˆ Project : p.created_at â‰¤ p.updated_at
        (Creation timestamp precedes update timestamp)

INV-P5: âˆ€ p âˆˆ Project : p.status = 'rendering' â†’
        âˆƒ j âˆˆ Job : j.project_id = p.id âˆ§ j.status âˆˆ {'queued', 'processing'}
        (Rendering project has active job)
```

## 7.8 AssetFile Invariants

```
INV-AF1: âˆ€ a âˆˆ AssetFile : a.status âˆˆ {'pending', 'ready', 'failed'}
        (Asset status must be one of allowed values)

INV-AF2: âˆ€ a âˆˆ AssetFile : a.size_bytes > 0
        (Asset size must be positive)

INV-AF3: âˆ€ a âˆˆ AssetFile : a.size_bytes â‰¤ 50 * 1024 * 1024
        (Asset size cannot exceed 50MB)

INV-AF4: âˆ€ a âˆˆ AssetFile : a.content_type âˆˆ {
          'image/jpeg', 'image/png', 'image/webp',
          'audio/mpeg', 'audio/wav', 'audio/ogg',
          'video/mp4'
        }
        (Content type must be allowed)

INV-AF5: âˆ€ a âˆˆ AssetFile : a.uploaded_by âˆˆ {u.id : u âˆˆ User}
        (Asset uploader reference must exist)

INV-AF6: âˆ€ a1, a2 âˆˆ AssetFile : a1 â‰  a2 â†’ a1.s3_key â‰  a2.s3_key
        (S3 keys are unique)
```

## 7.9 Webhook Invariants

```
INV-W1: âˆ€ w âˆˆ Webhook : w.team_id âˆˆ {t.id : t âˆˆ Team}
        (Webhook team reference must exist)

INV-W2: âˆ€ w âˆˆ Webhook : |w.events| > 0
        (Webhook must subscribe to at least one event)

INV-W3: âˆ€ w âˆˆ Webhook : âˆ€ e âˆˆ w.events : e âˆˆ {
          'job.queued', 'job.started', 'job.progress',
          'job.completed', 'job.failed', 'job.canceled'
        }
        (Webhook events must be valid)

INV-W4: âˆ€ w âˆˆ Webhook : w.url STARTS WITH 'https://'
        (Webhook URL must be HTTPS)

INV-W5: âˆ€ w âˆˆ Webhook : w.created_by âˆˆ {u.id : u âˆˆ User}
        (Webhook creator reference must exist)
```

## 7.10 WebhookDelivery Invariants

```
INV-WD1: âˆ€ d âˆˆ WebhookDelivery : d.status âˆˆ {'pending', 'retrying', 'delivered', 'failed'}
        (Delivery status must be one of allowed values)

INV-WD2: âˆ€ d âˆˆ WebhookDelivery : d.webhook_id âˆˆ {w.id : w âˆˆ Webhook}
        (Delivery webhook reference must exist)

INV-WD3: âˆ€ d âˆˆ WebhookDelivery : d.attempts â‰¤ d.max_attempts
        (Attempts cannot exceed maximum)

INV-WD4: âˆ€ d âˆˆ WebhookDelivery : d.status = 'delivered' â†’ d.delivered_at IS NOT NULL
        (Delivered webhooks have delivery timestamp)
```

## 7.11 Usage Invariants

```
INV-US1: âˆ€ u âˆˆ Usage : u.period MATCHES '^\d{4}-(0[1-9]|1[0-2])$'
        (Period format is YYYY-MM)

INV-US2: âˆ€ u âˆˆ Usage : u.renders_count â‰¥ 0
        (Render count cannot be negative)

INV-US3: âˆ€ u âˆˆ Usage : u.credits_used â‰¥ 0
        (Credits used cannot be negative)

INV-US4: âˆ€ u1, u2 âˆˆ Usage : (u1.owner = u2.owner âˆ§ u1.period = u2.period) â†’ u1 = u2
        (Owner-period pair uniqueness)
```

## 7.12 SystemAsset Invariants

```
INV-SA1: âˆ€ a âˆˆ SystemAsset : a.category âˆˆ {'sfx', 'ambient', 'music', 'transition'}
        (Category must be one of allowed values)

INV-SA2: âˆ€ a âˆˆ SystemAsset : a.id MATCHES '^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$'
        (ID format must match pattern)

INV-SA3: âˆ€ a1, a2 âˆˆ SystemAsset : a1 â‰  a2 â†’ a1.s3_key â‰  a2.s3_key
        (S3 keys are unique)
```

## 7.13 Cross-Entity Invariants

```
INV-X1: âˆ€ j âˆˆ Job :
        (j.owner = 'splice:user:' || j.triggered_by) âˆ¨
        (âˆƒ m âˆˆ Membership : m.team_id âˆˆ extract_team_from_urn(j.owner) âˆ§ m.user_id = j.triggered_by)
        (Job owner URN must be accessible by triggered_by user)

INV-X2: âˆ€ a âˆˆ AssetFile :
        (a.owner = 'splice:user:' || a.uploaded_by) âˆ¨
        (âˆƒ m âˆˆ Membership : m.team_id âˆˆ extract_team_from_urn(a.owner) âˆ§ m.user_id = a.uploaded_by)
        (Asset owner URN must be accessible by uploaded_by user)

INV-X3: âˆ€ a âˆˆ AssetFile :
        (a.project_id IS NOT NULL) â†’
        (a.owner STARTS WITH 'splice:team:' âˆ§
         âˆƒ p âˆˆ Project : p.id = a.project_id âˆ§ a.owner = 'splice:team:' || p.team_id)
        (Project-scoped assets must be owned by project's team)

INV-X4: âˆ€ k âˆˆ ApiKey :
        (k.owner MATCHES 'splice:([^:]+):([^:]+)' AS (team_id, user_id)) â†’
        âˆƒ m âˆˆ Membership : m.team_id = team_id âˆ§ m.user_id = k.user_id
        (Membership URN keys require valid membership)
```

## 7.14 Temporal Invariants

```
INV-TIME1: âˆ€ e âˆˆ {User, Team, Project, Job, AssetFile, Webhook} :
           e.created_at â‰¤ e.updated_at
           (Creation precedes last update)

INV-TIME2: âˆ€ j âˆˆ Job : j.started_at IS NOT NULL â†’ j.created_at â‰¤ j.started_at
           (Job start is after creation)

INV-TIME3: âˆ€ j âˆˆ Job : j.completed_at IS NOT NULL â†’ j.started_at â‰¤ j.completed_at
           (Job completion is after start)

INV-TIME4: âˆ€ i âˆˆ Invitation : i.created_at < i.expires_at
           (Invitation expiration is after creation)

INV-TIME5: âˆ€ i âˆˆ Invitation : i.accepted_at IS NOT NULL â†’ i.created_at < i.accepted_at
           (Acceptance is after creation)
```

## 7.15 Cardinality Constraints

```
CARD-1: âˆ€ t âˆˆ Team : |{m âˆˆ Membership : m.team_id = t.id âˆ§ m.role = 'owner'}| â‰¥ 1
        (At least one owner per team)

CARD-2: âˆ€ u âˆˆ User : |{m âˆˆ Membership : m.user_id = u.id âˆ§ m.role = 'owner'}| â‰¤ 10
        (Max 10 owned teams per user)

CARD-3: âˆ€ u âˆˆ User : |{m âˆˆ Membership : m.user_id = u.id}| â‰¤ 50
        (Max 50 team memberships per user)

CARD-4: âˆ€ t âˆˆ Team : |{i âˆˆ Invitation : i.team_id = t.id âˆ§ i.is_actionable}| â‰¤ 50
        (Max 50 pending invitations per team)

CARD-5: âˆ€ t âˆˆ Team, owner âˆˆ URN :
        owner STARTS WITH 'splice:team:' || t.id â†’
        |{j âˆˆ Job : j.owner = owner âˆ§ j.status âˆˆ {'queued', 'processing'}}| â‰¤ 5
        (Max 5 concurrent jobs per team)

CARD-6: âˆ€ u âˆˆ User WHERE tier = 'starter' :
        |{j âˆˆ Job : j.owner = 'splice:user:' || u.id âˆ§ j.status âˆˆ {'queued', 'processing'}}| â‰¤ 1
        (Max 1 concurrent job per starter user)
```

---

**Document Version: 0.4.3
**Last Updated**: 2025-01-30
**Status**: Active Specification
