# Section 5: Relationships

## 5.1 Relationship Diagram (Textual)

```
User 1 ─────────────────────────────── 0..* Membership
User 1 ─────────────────────────────── 0..* ApiKey
User 1 ─────────────────────────────── 0..* Generation (triggered_by)
User 1 ─────────────────────────────── 0..* Project (created_by)
User 1 ─────────────────────────────── 0..* Invitation (invited_by)
User 1 ─────────────────────────────── 0..* AssetFile (uploaded_by)
User 1 ─────────────────────────────── 0..* Conversation (user_id)
User 1 ─────────────────────────────── 0..* Artifact (created_by)

Team 1 ─────────────────────────────── 1..* Membership
Team 1 ◆───────────────────────────── 0..* Project
Team 1 ◆───────────────────────────── 0..* Webhook

Membership ────────────────────────── User (many-to-one)
Membership ────────────────────────── Team (many-to-one)

Project 1 ─────────────────────────── 0..* Generation
Project 1 ◆────────────────────────── 0..* AssetFile
Project 1 ◆────────────────────────── 0..* Artifact

Conversation 1 ◆──────────────────── 0..* Message
Conversation 1 ─────────────────────── 0..* Artifact (conversation_id)

Generation 1 ──────────────────────── 0..* Artifact (source_generation_id)
Generation 1 ◆─────────────────────── 0..* GenerationEvent

Webhook 1 ◆────────────────────────── 0..* WebhookDelivery
```

## 5.2 Relationship Matrix

| Entity A | Relationship | Entity B | A:B | Cascade |
|----------|--------------|----------|-----|---------|
| User | has | Membership | 1:0..* | CASCADE on User delete |
| Team | has | Membership | 1:1..* | CASCADE on Team delete |
| Team | owns | Project | 1:0..* | CASCADE |
| User | creates | Project | 1:0..* | No cascade (audit) |
| Project | has | Generation | 1:0..* | SET NULL on delete |
| User | triggers | Generation | 1:0..* | No cascade |
| Generation | emits | GenerationEvent | 1:0..* | CASCADE |
| Team | has | Webhook | 1:0..* | CASCADE |
| Webhook | has | WebhookDelivery | 1:0..* | CASCADE |
| User | has | ApiKey | 1:0..* | CASCADE |
| Project | has | AssetFile | 1:0..* | CASCADE |
| User | uploads | AssetFile | 1:0..* | No cascade |
| User | has | Conversation | 1:0..* | CASCADE |
| Conversation | has | Message | 1:0..* | CASCADE |
| User | creates | Artifact | 1:0..* | No cascade (audit) |
| Project | has | Artifact | 1:0..* | CASCADE |
| Conversation | produces | Artifact | 1:0..* | SET NULL on delete |
| Generation | outputs | Artifact | 1:0..* | SET NULL on delete |

---

# Section 6: State Machines

## 6.1 User.tier State Machine

```
            ┌──────────────────────────────────────┐
            │                                      │
   [signup] │                                      │ [invite_accept where
            ▼                                      │  invitee ∉ system]
       ┌─────────┐                                 │
       │ starter │                                 │
       └────┬────┘                                 │
            │                                      │
            │ [self_upgrade]                       │
            │ OR                                   │
            │ [invite_accept]                      │
            ▼                                      │
       ┌─────────┐◄────────────────────────────────┘
       │ creator │
       └─────────┘
            │
            ╳ (no downgrade)
```

**Formal Definition:**

```
States(User.tier) = {starter, creator}
Initial = starter (on signup) OR creator (on invite_accept for new user)
Terminal = ∅ (no terminal states, but creator is absorbing)

Transitions:
  δ(starter, self_upgrade) = creator
  δ(starter, invite_accept) = creator
  δ(creator, _) = creator  (absorbing state for tier)
```

## 6.2 Generation.status State Machine

```
                    ┌─────────────────────────────┐
                    │                             │
                    ▼                             │
              ┌──────────┐                        │
   [create]──►│  queued  │────[cancel]───────────┼──────────┐
              └────┬─────┘                        │          │
                   │                              │          │
                   │ [worker_picks_up]            │          │
                   ▼                              │          │
              ┌──────────┐                        │          │
              │processing│────[cancel]────────────┘          │
              └────┬─────┘                                   │
                   │                                         │
          ┌────────┼────────┐                                │
          │        │        │                                │
          │[success]    [failure]                            │
          ▼                 ▼                                ▼
     ┌──────────┐      ┌──────────┐                   ┌──────────┐
     │completed │      │  failed  │                   │ canceled │
     └──────────┘      └──────────┘                   └──────────┘
```

**Formal Definition:**

```
States(Generation.status) = {queued, processing, completed, failed, canceled}
Initial = queued
Terminal = {completed, failed, canceled}

Transitions:
  δ(queued, worker_picks_up) = processing
  δ(queued, cancel) = canceled
  δ(processing, success) = completed
  δ(processing, failure) = failed
  δ(processing, cancel) = canceled

Guards:
  cancel: triggered_by = generation.triggered_by ∨ user has admin/owner role on generation's team
```

## 6.3 Project.status State Machine

```
              ┌──────────┐
   [create]──►│  draft   │◄─────────────────────┐
              └────┬─────┘                      │
                   │                            │
                   │ [render]                   │ [generation.failed ∨ generation.canceled]
                   ▼                            │
              ┌──────────┐                      │
              │rendering │──────────────────────┘
              └────┬─────┘
                   │
                   │ [generation.completed]
                   ▼
              ┌──────────┐
              │completed │
              └────┬─────┘
                   │
        ┌──────────┴──────────┐
        │                     │
        │ [archive]           │ [render]
        ▼                     ▼
   ┌──────────┐          ┌──────────┐
   │ archived │          │rendering │
   └────┬─────┘          └──────────┘
        │
        │ [unarchive]
        ▼
   ┌──────────┐
   │  draft   │
   └──────────┘
```

**Formal Definition:**

```
States(Project.status) = {draft, rendering, completed, archived}
Initial = draft
Terminal = ∅

Transitions:
  δ(draft, render) = rendering
  δ(rendering, generation_completed) = completed
  δ(rendering, generation_failed) = draft
  δ(rendering, generation_canceled) = draft
  δ(completed, archive) = archived
  δ(completed, render) = rendering  (re-render)
  δ(draft, archive) = archived
  δ(archived, unarchive) = draft
```

## 6.4 Invitation State Machine

```
              ┌──────────┐
   [create]──►│ pending  │
              └────┬─────┘
                   │
         ┌────────┼─────────┬──────────┐
         │        │         │          │
    [accept]  [decline] [expire]  [revoke]
         │        │         │          │
         ▼        ▼         ▼          ▼
    ┌────────┐ ┌────────┐ ┌───────┐ ┌───────┐
    │accepted│ │declined│ │expired│ │revoked│
    └────────┘ └────────┘ └───────┘ └───────┘
```

**Formal Definition:**

```
States(Invitation) = {pending, accepted, declined, expired, revoked}
Initial = pending
Terminal = {accepted, declined, expired, revoked}

Derived state (computed, not stored):
  state =
    IF accepted_at IS NOT NULL THEN accepted
    ELSE IF declined_at IS NOT NULL THEN declined
    ELSE IF revoked_at IS NOT NULL THEN revoked
    ELSE IF expires_at < now() THEN expired
    ELSE pending

Transitions:
  δ(pending, accept) = accepted    [guard: expires_at > now()]
  δ(pending, decline) = declined
  δ(pending, expire) = expired     [automatic when expires_at reached]
  δ(pending, revoke) = revoked

Notes:
  - resend_invitation does NOT change state; invitation remains pending
  - It re-sends the email notification and extends expires_at by 7 days
```

## 6.5 WebhookDelivery.status State Machine

```
              ┌──────────┐
   [create]──►│ pending  │
              └────┬─────┘
                   │
                   │ [attempt]
                   ▼
              ┌──────────┐
         ┌────│ attempt  │────┐
         │    └──────────┘    │
         │                    │
    [2xx response]       [5xx or timeout]
         │                    │
         ▼                    ▼
    ┌──────────┐        ┌──────────┐
    │delivered │        │ retrying │──[max attempts]──► failed
    └──────────┘        └────┬─────┘
                             │
                        [4xx response]
                             │
                             ▼
                        ┌──────────┐
                        │  failed  │
                        └──────────┘
```

**Formal Definition:**

```
States(WebhookDelivery.status) = {pending, attempt, delivered, retrying, failed}
Initial = pending
Terminal = {delivered, failed}

Transitions:
  δ(pending, attempt) = attempt
  δ(attempt, success) = delivered    [guard: 2xx response received]
  δ(attempt, retry) = retrying       [guard: 5xx or timeout]
  δ(attempt, permanent_failure) = failed    [guard: 4xx response]
  δ(retrying, attempt) = attempt     [exponential backoff, max 5 retries]
  δ(retrying, max_exceeded) = failed
```

## 6.6 Conversation.status State Machine

```
              ┌──────────┐
   [create]──►│  active  │
              └────┬─────┘
                   │
                   │ [archive]
                   ▼
              ┌──────────┐
              │ archived │
              └────┬─────┘
                   │
                   │ [unarchive]
                   ▼
              ┌──────────┐
              │  active  │
              └──────────┘
```

**Formal Definition:**

```
States(Conversation.status) = {active, archived}
Initial = active
Terminal = ∅

Transitions:
  δ(active, archive) = archived
  δ(archived, unarchive) = active

Guards:
  archive: conversation.user_id = requesting_user.id
  unarchive: conversation.user_id = requesting_user.id
```

## 6.7 Artifact.status State Machine

```
              ┌──────────┐
   [create]──►│ pending  │
              └────┬─────┘
                   │
          ┌────────┼────────┐
          │                 │
     [confirm]          [fail]
          │                 │
          ▼                 ▼
     ┌──────────┐      ┌──────────┐
     │  ready   │      │  failed  │
     └──────────┘      └────┬─────┘
                            │
                            │ [retry]
                            ▼
                       ┌──────────┐
                       │ pending  │
                       └──────────┘
```

**Formal Definition:**

```
States(Artifact.status) = {pending, ready, failed}
Initial = pending
Terminal = ∅

Transitions:
  δ(pending, confirm) = ready      [guard: S3 object exists and validates]
  δ(pending, fail) = failed        [guard: validation/upload failure]
  δ(failed, retry) = pending       [guard: created_by = requesting_user.id]

Notes:
  - Storyboard artifacts may transition directly to 'ready' on creation
  - Media artifacts start as 'pending' and require upload confirmation
  - Generation-output artifacts transition to 'ready' when generation completes
```
