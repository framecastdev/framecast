# 14. ER Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────────────┐
│                                                                                         │
│  ┌──────────┐         ┌────────────┐         ┌──────────┐                               │
│  │   User   │────────<│ Membership │>────────│   Team   │                               │
│  │──────────│ 1    0..*│────────────│1..*   1 │──────────│                               │
│  │ id (PK)  │         │ id (PK)    │         │ id (PK)  │                               │
│  │ email    │         │ team_id(FK)│         │ name     │                               │
│  │ name     │         │ user_id(FK)│         │ slug     │                               │
│  │ tier     │         │ role       │         │ credits  │                               │
│  │ credits  │         └────────────┘         │ storage  │                               │
│  │ storage  │                                └────┬─────┘                               │
│  └────┬─────┘                                     │                                     │
│       │                                           │                                     │
│       │ 1                                         │ 1                                   │
│       │                                           │                                     │
│       │ 0..*                                      │ 0..*                                │
│  ┌────┴─────┐                                ┌────┴─────┐                               │
│  │  ApiKey  │                                │ Project  │                               │
│  │──────────│                                │──────────│                               │
│  │ id (PK)  │                                │ id (PK)  │                               │
│  │ user_id  │                                │ team_id  │                               │
│  │ owner    │                                │ name     │                               │
│  │ key_hash │                                │ status   │                               │
│  │ scopes   │                                │ spec     │                               │
│  └──────────┘                                └────┬─────┘                               │
│                                                   │                                     │
│       │                                           │ 1                                   │
│       │ (triggered_by)                            │                                     │
│       │                                           │ 0..*                                │
│       ▼ 1                                    ┌────┴─────┐      0..*  ┌──────────┐       │
│  ┌──────────┐      0..*  ┌────────────┐      │   Job    │◄───────────│  Asset   │       │
│  │   User   │◄───────────│    Job     │      │──────────│            │   File   │       │
│  └──────────┘ triggered  │────────────│      │ id (PK)  │            │──────────│       │
│                   by     │ id (PK)    │      │ owner    │            │ id (PK)  │       │
│                          │ owner      │      │project_id│            │ owner    │       │
│                          │ triggered  │      │ status   │            │project_id│       │
│                          │ project_id │      │ spec_snap│            │ s3_key   │       │
│                          │ status     │      │ progress │            │ status   │       │
│                          │ spec_snap  │      │ output   │            └──────────┘       │
│                          │ progress   │      └────┬─────┘                               │
│                          │ output     │           │                                     │
│                          └─────┬──────┘           │                                     │
│                                │                  │                                     │
│                                │ 1                │                                     │
│                                │                  │                                     │
│                                │ 0..*             │                                     │
│                          ┌─────┴──────┐           │                                     │
│                          │  JobEvent  │           │                                     │
│                          │────────────│           │                                     │
│                          │ id (PK)    │           │                                     │
│                          │ job_id     │           │                                     │
│                          │ sequence   │           │                                     │
│                          │ event_type │           │                                     │
│                          │ payload    │           │                                     │
│                          └────────────┘           │                                     │
│                                                   │                                     │
│  ┌──────────┐      0..*  ┌────────────┐           │                                     │
│  │   Team   │◄───────────│  Webhook   │           │                                     │
│  └──────────┘  team_id   │────────────│           │                                     │
│                          │ id (PK)    │           │                                     │
│                          │ team_id    │           │                                     │
│                          │ url        │           │                                     │
│                          │ events     │           │                                     │
│                          │ is_active  │           │                                     │
│                          └─────┬──────┘           │                                     │
│                                │                  │                                     │
│                                │ 1                │                                     │
│                                │                  │                                     │
│                                │ 0..*             │                                     │
│                          ┌─────┴──────┐           │                                     │
│                          │  Webhook   │           │                                     │
│                          │  Delivery  │───────────┘                                     │
│                          │────────────│   job_id (optional)                             │
│                          │ id (PK)    │                                                 │
│                          │ webhook_id │                                                 │
│                          │ status     │                                                 │
│                          │ payload    │                                                 │
│                          │ attempts   │                                                 │
│                          └────────────┘                                                 │
│                                                                                         │
│  ┌──────────┐      0..*  ┌────────────┐                                                 │
│  │   Team   │◄───────────│ Invitation │                                                 │
│  └──────────┘  team_id   │────────────│                                                 │
│                          │ id (PK)    │                                                 │
│                          │ invited_by │                                                 │
│                          │ email      │                                                 │
│       ┌──────────────────│ token      │                                                 │
│       │                  │ role       │                                                 │
│       │ 0..*             │ expires_at │                                                 │
│       ▼ 1                │ revoked_at │                                                 │
│  ┌──────────┐            └────────────┘                                                 │
│  │   User   │                                                                           │
│  └──────────┘                                                                           │
│                                                                                         │
│  ┌──────────┐      0..*  ┌────────────┐                                                 │
│  │   URN    │◄───────────│   Usage    │                                                 │
│  │ (owner)  │            │────────────│                                                 │
│  └──────────┘            │ id (PK)    │                                                 │
│                          │ owner      │                                                 │
│                          │ period     │                                                 │
│                          │ renders    │                                                 │
│                          │ credits    │                                                 │
│                          └────────────┘                                                 │
│                                                                                         │
│  ┌─────────────┐                                                                        │
│  │ SystemAsset │  (read-only, managed by system)                                        │
│  │─────────────│                                                                        │
│  │ id (PK)     │                                                                        │
│  │ category    │                                                                        │
│  │ name        │                                                                        │
│  │ duration    │                                                                        │
│  │ tags[]      │                                                                        │
│  └─────────────┘                                                                        │
│                                                                                         │
│  ┌──────────┐      0..*  ┌─────────────┐      0..*  ┌──────────┐                        │
│  │   User   │◄───────────│Conversation │◆───────────│ Message  │                        │
│  └──────────┘  user_id   │─────────────│            │──────────│                        │
│                          │ id (PK)     │            │ id (PK)  │                        │
│                          │ user_id(FK) │            │ conv_id  │                        │
│                          │ title       │            │ role     │                        │
│                          │ model       │            │ content  │                        │
│                          │ status      │            │ sequence │                        │
│                          │ msg_count   │            │ tokens   │                        │
│                          └──────┬──────┘            └────┬─────┘                        │
│                                 │                        │                              │
│                                 │ 0..*                   │ 0..*                         │
│                                 │ (conversation_id)      │ (message_artifacts)          │
│                                 ▼                        ▼                              │
│  ┌──────────┐      0..*  ┌─────────────┐                                                │
│  │ Project  │◄───────────│  Artifact   │                                                │
│  └──────────┘ project_id │─────────────│                                                │
│                          │ id (PK)     │                                                │
│       ┌──────────────────│ owner (URN) │                                                │
│       │                  │ created_by  │                                                │
│       │ 0..*             │ kind        │                                                │
│       ▼ 1                │ status      │                                                │
│  ┌──────────┐            │ source      │                                                │
│  │   User   │            │ spec/media  │                                                │
│  └──────────┘            └──────┬──────┘                                                │
│   (created_by)                  │                                                       │
│                                 │ 0..*                                                  │
│                                 │ (source_job_id)                                       │
│                                 ▼                                                       │
│                            ┌──────────┐                                                 │
│                            │   Job    │                                                 │
│                            └──────────┘                                                 │
│                                                                                         │
└─────────────────────────────────────────────────────────────────────────────────────────┘
```

---
