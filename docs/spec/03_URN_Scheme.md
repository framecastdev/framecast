## 3. URN Scheme

### 3.1 URN Patterns

| URN Pattern | Represents | Example |
|-------------|------------|---------|
| `framecast:user:<user_id>` | User (standalone) | `framecast:user:usr_abc123` |
| `framecast:team:<team_id>` | Team (collective) | `framecast:team:tm_xyz789` |
| `framecast:<team_id>:<user_id>` | User within team context (membership) | `framecast:tm_xyz789:usr_abc123` |

### 3.2 URN Regex

```
^framecast:(user:[a-z0-9_]+|team:[a-z0-9_]+|[a-z0-9_]+:[a-z0-9_]+)$
```

### 3.3 Ownership Semantics

| Owner URN | Visibility | Storage Quota | On User Leave | On Entity Delete |
|-----------|------------|---------------|---------------|------------------|
| `framecast:user:usr_abc` | User only | User's personal | Stays with user | Cascade delete |
| `framecast:tm_xyz:usr_abc` | User only | Team's quota | Stays (inherits on rejoin) | Cascade with team |
| `framecast:team:tm_xyz` | All team members | Team's quota | Unaffected | Cascade with team |

### 3.4 URN Constraints

```
// Personal URN must match triggering user
(owner = 'framecast:user:X') â†’ (X = triggered_by.id)

// Membership URN requires valid membership
(owner MATCHES 'framecast:([^:]+):([^:]+)' AS (team, user)) â†’
  âˆƒ m âˆˆ Membership : m.team_id = team âˆ§ m.user_id = user

// Team URN requires creator tier
(owner STARTS WITH 'framecast:team:') â†’ (user.tier = 'creator')

// Membership URN requires creator tier
(owner MATCHES 'framecast:tm_[^:]+:usr_') â†’ (user.tier = 'creator')

// Starter can only use personal URN
(user.tier = 'starter') â†’ (owner = 'framecast:user:' + user.id)
```
