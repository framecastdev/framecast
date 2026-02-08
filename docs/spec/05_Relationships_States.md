# Section 5: Relationships

## 5.1 Relationship Diagram (Textual)

```
User 1 ─────────────────────────────── 0..* Membership
User 1 ─────────────────────────────── 0..* ApiKey
User 1 ─────────────────────────────── 0..* Job (triggered_by)
User 1 ─────────────────────────────── 0..* Project (created_by)
User 1 ─────────────────────────────── 0..* Invitation (invited_by)
User 1 ─────────────────────────────── 0..* AssetFile (uploaded_by)

Team 1 ─────────────────────────────── 1..* Membership
Team 1 ◆───────────────────────────── 0..* Project
Team 1 ◆───────────────────────────── 0..* Webhook

Membership ────────────────────────── User (many-to-one)
Membership ────────────────────────── Team (many-to-one)

Project 1 ─────────────────────────── 0..* Job
Project 1 ◆────────────────────────── 0..* AssetFile

Job 1 ◆────────────────────────────── 0..* JobEvent

Webhook 1 ◆────────────────────────── 0..* WebhookDelivery
```

## 5.2 Relationship Matrix

| Entity A | Relationship | Entity B | A:B | Cascade |
|----------|--------------|----------|-----|---------|
| User | has | Membership | 1:0..* | CASCADE on User delete |
| Team | has | Membership | 1:1..* | CASCADE on Team delete |
| Team | owns | Project | 1:0..* | CASCADE |
| User | creates | Project | 1:0..* | No cascade (audit) |
| Project | has | Job | 1:0..* | SET NULL on delete |
| User | triggers | Job | 1:0..* | No cascade |
| Job | emits | JobEvent | 1:0..* | CASCADE |
| Team | has | Webhook | 1:0..* | CASCADE |
| Webhook | has | WebhookDelivery | 1:0..* | CASCADE |
| User | has | ApiKey | 1:0..* | CASCADE |
| Project | has | AssetFile | 1:0..* | CASCADE |
| User | uploads | AssetFile | 1:0..* | No cascade |

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

## 6.2 Job.status State Machine

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
States(Job.status) = {queued, processing, completed, failed, canceled}
Initial = queued
Terminal = {completed, failed, canceled}

Transitions:
  δ(queued, worker_picks_up) = processing
  δ(queued, cancel) = canceled
  δ(processing, success) = completed
  δ(processing, failure) = failed
  δ(processing, cancel) = canceled

Guards:
  cancel: triggered_by = job.triggered_by ∨ user has admin/owner role on job's team
```

## 6.3 Project.status State Machine

```
              ┌──────────┐
   [create]──►│  draft   │◄─────────────────────┐
              └────┬─────┘                      │
                   │                            │
                   │ [render]                   │ [job.failed ∨ job.canceled]
                   ▼                            │
              ┌──────────┐                      │
              │rendering │──────────────────────┘
              └────┬─────┘
                   │
                   │ [job.completed]
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
  δ(rendering, job_completed) = completed
  δ(rendering, job_failed) = draft
  δ(rendering, job_canceled) = draft
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
