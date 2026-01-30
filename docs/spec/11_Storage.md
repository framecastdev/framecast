# 12. Storage & Retention

## 12.1 S3 Structure

```
s3://framecast-outputs/
  ├── user/
  │   └── {user_id}/
  │       ├── jobs/
  │       │   └── {job_id}/
  │       │       ├── final.mp4
  │       │       ├── scene_{scene_id}.mp4
  │       │       └── thumbnail.jpg
  │       └── assets/
  │           └── {asset_id}/{filename}
  │
  └── team/
      └── {team_id}/
          ├── {user_id}/                    # framecast:{team_id}:{user_id}
          │   ├── jobs/{job_id}/
          │   └── assets/{asset_id}/
          └── shared/                        # framecast:team:{team_id}
              ├── jobs/{job_id}/
              └── assets/{asset_id}/

s3://framecast-system/
  └── assets/
      └── {category}/
          └── {asset_id}.{ext}
```

## 12.2 Storage Limits

| Tier | Personal Ephemeral Storage |
|------|---------------------------|
| Starter | Defined by plan (credits-based) |
| Creator | Team-based quota |

## 12.3 Retention Rules

| Owner | Retention |
|-------|-----------|
| `framecast:user:*` | Until job deleted or user deleted |
| `framecast:{team}:{user}` | Until job deleted or team deleted |
| `framecast:team:*` | Until job deleted or team deleted |

No time-based expiry. Storage limits enforce cleanup pressure.

## 12.4 Presigned URL Expiry

| Resource | Expiry |
|----------|--------|
| Job output (video) | 1 hour |
| Asset files (images) | 1 hour |
| Upload URLs | 15 minutes |
| System asset preview | 24 hours |

## 12.5 S3 Lifecycle

- 0-30 days: S3 Standard
- 30+ days: Glacier Instant Retrieval (implementation detail)

---

## 12.6 Credit Source Rules

Credits are debited and refunded based on the job's `owner` URN:

| Owner URN Pattern | Credit Source | Description |
|-------------------|---------------|-------------|
| `framecast:user:{user_id}` | `User.credits` | Personal jobs (Starter or Creator) |
| `framecast:team:{team_id}` | `Team.credits` | Team-shared jobs |
| `framecast:{team_id}:{user_id}` | `Team.credits` | Team-private jobs (team pays) |

### Rules

1. **Debit on job creation**: Credits are reserved from the source identified by `owner` URN
2. **Refund on failure/cancel**: Credits are returned to the same source
3. **Creator personal jobs**: When a Creator uses `framecast:user:X`, their personal `User.credits` are used (not any team's credits)
4. **Membership URN jobs**: The team pays for member's work; useful for tracking individual output while billing the team
5. **Insufficient credits**: Job creation fails with `INSUFFICIENT_CREDITS` error

### Validation

```
ON job.create:
  IF owner = 'framecast:user:{user_id}' THEN
    source = User WHERE id = user_id
  ELSE IF owner = 'framecast:team:{team_id}' THEN
    source = Team WHERE id = team_id
  ELSE IF owner = 'framecast:{team_id}:{user_id}' THEN
    source = Team WHERE id = team_id
  
  IF source.credits < estimated_credits THEN
    REJECT with INSUFFICIENT_CREDITS
```

---

## 12.7 Credit Refund Policy

### Overview

Splice uses a Runway-style refund policy: automatic refunds for system errors,
partial charges for cancellations, and no refunds for completed jobs.

### Refund Rules by Failure Type

| Failure Type | Refund | Calculation |
|--------------|--------|-------------|
| `system` | Full | credits_refunded = credits_charged |
| `timeout` | Full | credits_refunded = credits_charged |
| `validation` | Partial | credits_refunded = credits_charged × (1 - progress.percent / 100) |
| `canceled` | Partial | credits_refunded = credits_charged × (1 - progress.percent / 100) × 0.9 |
| `completed` | None | credits_refunded = 0 |

### Detailed Rules

**System Errors (failure_type = 'system'):**
  - Infrastructure failures, service outages, internal errors
  - User is not at fault
  - Full refund: 100% of credits_charged returned
  - Example: GPU crashed, storage unavailable, rendering service down

**Timeout (failure_type = 'timeout'):**
  - Job exceeded maximum processing time
  - Usually indicates system issue (not user's spec)
  - Full refund: 100% of credits_charged returned
  - Max processing time: 30 minutes per job (configurable)

**Validation Errors (failure_type = 'validation'):**
  - Spec issues detected during generation (not caught in pre-validation)
  - Examples: Asset became unavailable, reference integrity broken mid-job
  - Partial refund based on progress
  - Formula: refund = charged × (1 - progress%)
  - Example: Job failed at 40% → 60% refund

**User Cancellation (failure_type = 'canceled'):**
  - User explicitly canceled the job
  - Partial refund based on progress, minus 10% cancellation fee
  - Formula: refund = charged × (1 - progress%) × 0.9
  - Example: Canceled at 30% → 63% refund (70% × 0.9)
  - Minimum charge: 10% of estimated (cancellation fee)

**Completed Jobs:**
  - No refunds for completed jobs regardless of output quality
  - User received the generated video
  - Quality disputes handled via support (manual review)

### Refund Processing

1. When job transitions to terminal state (failed, canceled):
   - failure_type is set based on cause
   - credits_refunded is calculated per rules above
   - Owner's credit balance is incremented by credits_refunded

2. Credit balance update is atomic with job state transition

3. Refund is recorded in Usage entity for billing reconciliation

### Implementation Notes

```
ON job.status → {failed, canceled}:

  IF failure_type IN ('system', 'timeout') THEN
    credits_refunded = credits_charged

  ELSE IF failure_type = 'validation' THEN
    credits_refunded = FLOOR(credits_charged * (1 - progress.percent / 100))

  ELSE IF failure_type = 'canceled' THEN
    refund_percent = (1 - progress.percent / 100) * 0.9
    credits_refunded = FLOOR(credits_charged * refund_percent)
    -- Ensure minimum 10% charge
    credits_refunded = MIN(credits_refunded, credits_charged * 0.9)

  -- Update owner balance
  IF owner STARTS WITH 'framecast:user:' THEN
    UPDATE User SET credits = credits + credits_refunded WHERE id = owner_user_id
  ELSE
    UPDATE Team SET credits = credits + credits_refunded WHERE id = owner_team_id

  -- Record in usage
  UPDATE Usage SET credits_refunded = credits_refunded + :credits_refunded
    WHERE owner = job.owner AND period = current_period()
```

### Webhook Payload Update

The `job.failed` and `job.canceled` webhook events include refund information:

```json
{
  "event": "job.failed",
  "job": {
    "id": "uuid",
    "status": "failed",
    "failure_type": "system",
    "credits_charged": 100,
    "credits_refunded": 100,
    "progress": {
      "percent": 45
    }
  }
}
```
