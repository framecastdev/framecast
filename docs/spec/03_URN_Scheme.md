## 3. URN Scheme

### 3.1 URN Patterns

| URN Pattern | Represents | Example |
|-------------|------------|---------|
| `splice:user:<user_id>` | User (standalone) | `splice:user:usr_abc123` |
| `splice:team:<team_id>` | Team (collective) | `splice:team:tm_xyz789` |
| `splice:<team_id>:<user_id>` | User within team context (membership) | `splice:tm_xyz789:usr_abc123` |

### 3.2 URN Regex

```
^splice:(user:[a-z0-9_]+|team:[a-z0-9_]+|[a-z0-9_]+:[a-z0-9_]+)$
```

### 3.3 Ownership Semantics

| Owner URN | Visibility | Storage Quota | On User Leave | On Entity Delete |
|-----------|------------|---------------|---------------|------------------|
| `splice:user:usr_abc` | User only | User's personal | Stays with user | Cascade delete |
| `splice:tm_xyz:usr_abc` | User only | Team's quota | Stays (inherits on rejoin) | Cascade with team |
| `splice:team:tm_xyz` | All team members | Team's quota | Unaffected | Cascade with team |

### 3.4 URN Constraints

```
// Personal URN must match triggering user
(owner = 'splice:user:X') â†’ (X = triggered_by.id)

// Membership URN requires valid membership
(owner MATCHES 'splice:([^:]+):([^:]+)' AS (team, user)) â†’
  âˆƒ m âˆˆ Membership : m.team_id = team âˆ§ m.user_id = user

// Team URN requires creator tier
(owner STARTS WITH 'splice:team:') â†’ (user.tier = 'creator')

// Membership URN requires creator tier
(owner MATCHES 'splice:tm_[^:]+:usr_') â†’ (user.tier = 'creator')

// Starter can only use personal URN
(user.tier = 'starter') â†’ (owner = 'splice:user:' + user.id)
```
