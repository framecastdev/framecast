## 2. User Model

### 2.1 User States

Let U be the set of all potential users.

```
U = Visitor ∪ Registered
Registered = Starter ∪ Creator
Visitor ∩ Registered = ∅
Starter ∩ Creator = ∅
```

| State | Name | Definition |
|-------|------|------------|
| Non-existent | `visitor` | User not registered in system |
| Basic User | `starter` | Registered, tier = "starter" |
| Full User | `creator` | Registered, tier = "creator" |

### 2.2 State Transitions

```
         ┌─────────────────────────────────────────────────┐
         │                                                 │
         ▼                                                 │
    ┌─────────┐      signup        ┌──────────────┐       │
    │ Visitor │ ─────────────────► │   Starter    │       │
    │ (none)  │                    │(tier=starter)│       │
    └─────────┘                    └──────┬───────┘       │
         │                                │               │
         │                                │ self_upgrade  │
         │                                │ OR            │
         │   invite_accept                │ invite_accept │
         │   [tier := creator]            ▼               │
         │                         ┌──────────────┐       │
         └────────────────────────►│   Creator    │◄──────┘
              signup_via_invite    │(tier=creator)│  invite_accept
                                   └──────────────┘  [already creator]
```

### 2.3 Transition Definitions

#### T1: Signup
```
T1: Visitor → Starter

trigger:    signup(email, password)
guard:      ∄ u ∈ Registered : u.email = email
action:     CREATE User {
              id: uuid(),
              email: email,
              tier: "starter",
              created_at: now()
            }
```

#### T2: Self-Upgrade
```
T2: Starter → Creator

trigger:    upgrade()
guard:      user.tier = "starter"
action:     BEGIN TRANSACTION
              UPDATE User SET tier = "creator", upgraded_at = now()
              CREATE Team {
                id: uuid(),
                name: "My Team",
                slug: generate_slug()
              }
              CREATE Membership {
                team_id: team.id,
                user_id: user.id,
                role: "owner"
              }
              CREATE Project {
                id: uuid(),
                team_id: team.id,
                name: "Welcome to Splice",
                status: "draft",
                spec: WELCOME_SPEC,
                created_by: user.id
              }
            COMMIT
```

#### T3: Invite Accept (New User)
```
T3: Visitor → Creator

trigger:    accept_invite(token)
guard:      ∃ i ∈ Invitation : i.token = token ∧ i.expires_at > now() ∧ i.accepted_at = null
            ∧ ∄ u ∈ Registered : u.email = i.email
action:     BEGIN TRANSACTION
              -- Create user as creator
              CREATE User {
                id: uuid(),
                email: invitation.email,
                tier: "creator",
                upgraded_at: now()
              }
              -- Create their first team
              CREATE Team {
                id: uuid(),
                name: "My Team",
                slug: generate_slug()
              }
              CREATE Membership {
                team_id: new_team.id,
                user_id: user.id,
                role: "owner"
              }
              -- Join invited team
              CREATE Membership {
                team_id: invitation.team_id,
                user_id: user.id,
                role: invitation.role
              }
              -- Welcome project
              CREATE Project {
                id: uuid(),
                team_id: new_team.id,
                name: "Welcome to Splice",
                status: "draft",
                spec: WELCOME_SPEC,
                created_by: user.id
              }
              -- Mark invitation used
              UPDATE Invitation SET accepted_at = now()
            COMMIT
```

#### T4: Invite Accept (Starter User)
```
T4: Starter → Creator

trigger:    accept_invite(token)
guard:      ∃ i ∈ Invitation : i.token = token ∧ i.expires_at > now() ∧ i.accepted_at = null
            ∧ user.email = i.email ∧ user.tier = "starter"
action:     BEGIN TRANSACTION
              -- Upgrade user
              UPDATE User SET tier = "creator", upgraded_at = now()
              -- Create their first team
              CREATE Team {
                id: uuid(),
                name: "My Team",
                slug: generate_slug()
              }
              CREATE Membership {
                team_id: new_team.id,
                user_id: user.id,
                role: "owner"
              }
              -- Join invited team
              CREATE Membership {
                team_id: invitation.team_id,
                user_id: user.id,
                role: invitation.role
              }
              -- Welcome project
              CREATE Project {
                id: uuid(),
                team_id: new_team.id,
                name: "Welcome to Splice",
                status: "draft",
                spec: WELCOME_SPEC,
                created_by: user.id
              }
              UPDATE Invitation SET accepted_at = now()
            COMMIT
```

#### T5: Invite Accept (Creator User)
```
T5: Creator → Creator

trigger:    accept_invite(token)
guard:      ∃ i ∈ Invitation : i.token = token ∧ i.expires_at > now() ∧ i.accepted_at = null
            ∧ user.email = i.email ∧ user.tier = "creator"
action:     BEGIN TRANSACTION
              CREATE Membership {
                team_id: invitation.team_id,
                user_id: user.id,
                role: invitation.role
              }
              UPDATE Invitation SET accepted_at = now()
            COMMIT
```
