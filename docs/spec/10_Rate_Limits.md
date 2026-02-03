# 11 Rate Limits

Rate limits are enforced to ensure fair usage and system stability. Different plan tiers have different rate limit allocations.

## 11.1 API Rate Limits

API requests are rate-limited based on the user's subscription plan:

| Plan | Rate Limit | Window |
|------|-----------|--------|
| Starter | 60 RPM | Per minute |
| Creator | 300 RPM | Per minute |

**RPM** = Requests Per Minute

Rate limits are evaluated per authenticated user. Anonymous/unauthenticated requests are subject to stricter limits.

## 11.2 Concurrent Job Limits

The number of concurrent jobs that can be executed simultaneously is limited based on plan:

| Plan | Concurrent Jobs | Scope |
|------|-----------------|-------|
| Starter | 1 | Per user |
| Creator | 5 | Per team |

Once a user/team reaches their concurrent job limit, additional job submissions
will be queued and executed when resources become available.

## 11.3 Invitation Limits

Team invitations are rate-limited to prevent abuse:

| Limit | Value | Scope |
|-------|-------|-------|
| Max pending invitations | 50 | Per team |
| Invitation rate limit | 20 per day | Per team |

When the pending invitation limit is reached, additional invitations cannot be sent
until existing invitations are accepted or revoked.
The daily rate limit is reset at UTC midnight.

## 11.4 Team Limits Ã¢â€ Â NEW in v0.4.1

| Limit | Value | Description |
|-------|-------|-------------|
| Max owned teams per user | 10 | Teams where user has 'owner' role |
| Max team memberships per user | 50 | Total teams user belongs to |

**Notes:**

- "Owned teams" counts teams where `user.role = 'owner'`
- User can be member/admin/viewer of unlimited additional teams (up to membership limit)
- Limits can be increased for enterprise accounts (out of scope for v1)

## 11.5 Rate Limit Headers

All API responses include rate limit information in response headers:

```
X-RateLimit-Limit: 300
X-RateLimit-Remaining: 298
X-RateLimit-Reset: 1640000000
X-RateLimit-RetryAfter: 60
```

| Header | Description |
|--------|-------------|
| `X-RateLimit-Limit` | Total number of requests allowed in the current window |
| `X-RateLimit-Remaining` | Number of requests remaining in the current window |
| `X-RateLimit-Reset` | Unix timestamp when the current rate limit window resets |
| `X-RateLimit-RetryAfter` | Seconds to wait before retrying (only present when rate limited) |

## 11.6 Rate Limit Response

When a rate limit is exceeded, the API returns HTTP 429 (Too Many Requests):

```json
{
  "error": {
    "code": "RATE_LIMIT_EXCEEDED",
    "message": "Rate limit exceeded. Please retry after 60 seconds.",
    "retryAfter": 60
  }
}
```

**HTTP Status:** 429 Too Many Requests

**Response Headers:**

- `X-RateLimit-Reset`: Timestamp when limit resets
- `X-RateLimit-RetryAfter`: Seconds to wait before retry (also in response body)
- `Retry-After`: Standard HTTP header with retry delay in seconds

Clients should implement exponential backoff and respect the `Retry-After` header when handling rate limit responses.
