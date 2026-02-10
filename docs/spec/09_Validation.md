# Section 10: Validation Rules

## 10.1 Spec Validation

All validation rules for Framecast specs must satisfy the following constraints:

### Size Limits

```
spec.size â‰¤ 100 KB
|spec.scenes| â‰¤ 50
|spec.symbols| â‰¤ 20
|spec.transition_presets| â‰¤ 20
|spec.timeline| â‰¤ 100
```

### Field Limits

```
âˆ€ s âˆˆ spec.scenes : |s.prompt| â‰¤ 2000
âˆ€ s âˆˆ spec.scenes : 1 â‰¤ s.duration â‰¤ 30
âˆ€ sym âˆˆ spec.symbols : |sym.prompt| â‰¤ 1000
Î£(spec.scenes[].duration) â‰¤ 300  // 5 min max
```

### Audio Limits

```
âˆ€ s âˆˆ spec.scenes : |s.audio.sfx| â‰¤ 10
âˆ€ s âˆˆ spec.scenes : |s.audio.dialogue| â‰¤ 5
âˆ€ d âˆˆ spec.scenes[].audio.dialogue : |d.text| â‰¤ 500
```

### Volume Range

```
âˆ€ audio_ref : audio_ref.volume IS NULL âˆ¨ (0 â‰¤ audio_ref.volume â‰¤ 1)
```

### Reference Integrity Rules

#### Timeline Scene References

```
âˆ€ t âˆˆ spec.timeline :
  (t.scene IS NOT NULL) â†’ (t.scene âˆˆ spec.scenes[].id)
```

All scene IDs referenced in the timeline must exist in the scenes array.

#### Timeline Transition Preset References

```
âˆ€ t âˆˆ spec.timeline :
  (t.transition IS STRING) â†’ (t.transition âˆˆ spec.transition_presets.keys)
```

All transition presets referenced in the timeline must be defined in the transition_presets object.

#### Flashback Scene References

```
âˆ€ t âˆˆ spec.timeline :
  (t.flashback IS NOT NULL) â†’ (âˆ€ s âˆˆ t.flashback.scenes : s âˆˆ spec.scenes[].id)
```

All scene IDs in flashback elements must exist in the scenes array.

#### Montage Scene References

```
âˆ€ t âˆˆ spec.timeline :
  (t.montage IS NOT NULL) â†’ (âˆ€ s âˆˆ t.montage.scenes : s âˆˆ spec.scenes[].id)
```

All scene IDs in montage elements must exist in the scenes array.

#### Transitions Map Integrity

```
âˆ€ k âˆˆ spec.transitions.keys :
  (k â‰  'default') â†’ (
    k MATCHES '^(.+)->(.+)$' âˆ§
    $1 âˆˆ spec.scenes[].id âˆ§
    $2 âˆˆ spec.scenes[].id
  )
```

Transitions map keys (except 'default') must be in the format "scene1->scene2"
where both scene IDs exist in the scenes array.

#### Symbol References in Prompts

```
âˆ€ s âˆˆ spec.scenes :
  âˆ€ match âˆˆ s.prompt.match(/@(\w+)/g) :
    match âˆˆ spec.symbols.keys
```

Any symbol references in scene prompts (using @symbol notation) must reference symbols defined in the spec.

#### Dialogue Speaker Voice Integrity

```
âˆ€ s âˆˆ spec.scenes :
  âˆ€ d âˆˆ s.audio.dialogue :
    (d.speaker IS NOT NULL) â†’ (
      d.speaker âˆˆ spec.symbols.keys âˆ§
      spec.symbols[d.speaker].voice IS NOT NULL
    )
```

Any dialogue speaker must be a defined symbol with a voice asset configured.

#### Audio Asset Integrity

```
âˆ€ asset_id âˆˆ all_audio_asset_ids(spec) :
  (asset_id STARTS WITH 'asset_') â†’ (âˆƒ a âˆˆ SystemAsset : a.id = asset_id)
  âˆ¨
  (âˆƒ a âˆˆ AssetFile : a.id = asset_id âˆ§ a.content_type STARTS WITH 'audio/' âˆ§ a.status = 'ready')
```

All audio asset IDs must either reference a valid system asset (by ID) or a user-uploaded asset that is:

- Audio content type
- In 'ready' status

#### Image Asset Integrity

```
âˆ€ asset_id âˆˆ all_image_asset_ids(spec) :
  âˆƒ a âˆˆ AssetFile : a.id = asset_id âˆ§ a.content_type STARTS WITH 'image/' âˆ§ a.status = 'ready'
```

All image asset IDs must reference a user-uploaded asset that is:

- Image content type
- In 'ready' status

---

## 10.2 Validation Response Format

When validating a spec, the API returns a comprehensive validation response with errors and warnings:

```json
{
  "valid": false,
  "errors": [
    {
      "path": "timeline[0].scene",
      "message": "Scene 'escape' does not exist",
      "value": "escape",
      "valid_values": ["discovery", "chase"]
    },
    {
      "path": "scenes",
      "message": "Exceeds maximum of 50 scenes",
      "value": 73,
      "limit": 50
    }
  ],
  "warnings": [
    {
      "path": "scenes[2].duration",
      "message": "Duration > 10s may affect quality"
    }
  ]
}
```

### Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `valid` | boolean | Whether the spec passed all validation checks |
| `errors` | array | Fatal validation errors that prevent rendering |
| `warnings` | array | Non-fatal issues that may impact quality |

### Error Object Fields

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | JSONPath to the problematic field (e.g., "scenes[2].duration") |
| `message` | string | Human-readable error description |
| `value` | any | The value that failed validation |
| `valid_values` | array | List of acceptable values (if applicable) |
| `limit` | number | The limit that was exceeded (if applicable) |

### Warning Object Fields

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | JSONPath to the field with warning |
| `message` | string | Description of the potential issue |

---

## 10.3 Webhook Event Types (NEW in v0.4.0)

Webhooks allow your application to receive real-time notifications about generation state
changes. The Framecast API sends webhook events to your registered endpoints
for generation lifecycle events.

### Valid Webhook Events

```
- generation.queued       : Generation entered queue
- generation.started      : Generation processing began
- generation.progress     : Generation progress update (throttled to max 1/sec per generation)
- generation.completed    : Generation finished successfully
- generation.failed       : Generation failed
- generation.canceled     : Generation was canceled
```

### Payload Schemas

#### generation.queued / generation.started

Sent when a generation enters the queue or starts processing.

```json
{
  "event": "generation.queued",
  "timestamp": "2025-01-29T12:00:00Z",
  "delivery_id": "uuid",
  "generation": {
    "id": "uuid",
    "owner": "framecast:team:tm_xyz",
    "project_id": "uuid or null",
    "triggered_by": "uuid",
    "status": "queued",
    "created_at": "timestamp"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | Event type: "generation.queued" or "generation.started" |
| `timestamp` | string | ISO 8601 timestamp when event occurred |
| `delivery_id` | string | Unique UUID for this webhook delivery (for deduplication) |
| `generation.id` | string | Unique generation identifier |
| `generation.owner` | string | URN of generation owner (user or team) |
| `generation.project_id` | string or null | Associated project ID if applicable |
| `generation.triggered_by` | string | User ID who triggered the generation |
| `generation.status` | string | Current generation status ("queued" or "started") |
| `generation.created_at` | string | ISO 8601 timestamp when generation was created |

#### generation.progress

Sent periodically during generation processing (throttled to max 1 per second per generation).

```json
{
  "event": "generation.progress",
  "timestamp": "2025-01-29T12:02:00Z",
  "delivery_id": "uuid",
  "generation": {
    "id": "uuid",
    "status": "processing",
    "progress": {
      "phase": "generating",
      "percent": 45,
      "scenes_total": 5,
      "scenes_completed": 2,
      "current_scene": "scene_3"
    }
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | Event type: "generation.progress" |
| `timestamp` | string | ISO 8601 timestamp when event occurred |
| `delivery_id` | string | Unique UUID for this webhook delivery |
| `generation.id` | string | Unique generation identifier |
| `generation.status` | string | Current generation status ("processing") |
| `generation.progress.phase` | string | Current processing phase (e.g., "generating", "encoding") |
| `generation.progress.percent` | number | Overall completion percentage (0-100) |
| `generation.progress.scenes_total` | number | Total number of scenes to process |
| `generation.progress.scenes_completed` | number | Number of scenes successfully processed |
| `generation.progress.current_scene` | string | ID of scene currently being processed |

#### generation.completed

Sent when a generation finishes successfully.

```json
{
  "event": "generation.completed",
  "timestamp": "2025-01-29T12:05:00Z",
  "delivery_id": "uuid",
  "generation": {
    "id": "uuid",
    "owner": "framecast:team:tm_xyz",
    "project_id": "uuid or null",
    "status": "completed",
    "output": {
      "video_url": "presigned URL (1hr)",
      "thumbnail_url": "presigned URL (1hr)",
      "duration": 30,
      "resolution": "1920x1080",
      "size_bytes": 12345678
    },
    "credits_charged": 100,
    "started_at": "timestamp",
    "completed_at": "timestamp"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | Event type: "generation.completed" |
| `timestamp` | string | ISO 8601 timestamp when event occurred |
| `delivery_id` | string | Unique UUID for this webhook delivery |
| `generation.id` | string | Unique generation identifier |
| `generation.owner` | string | URN of generation owner |
| `generation.project_id` | string or null | Associated project ID if applicable |
| `generation.status` | string | Current generation status ("completed") |
| `generation.output.video_url` | string | Presigned S3 URL to final video (expires in 1 hour) |
| `generation.output.thumbnail_url` | string | Presigned S3 URL to thumbnail (expires in 1 hour) |
| `generation.output.duration` | number | Video duration in seconds |
| `generation.output.resolution` | string | Video resolution (e.g., "1920x1080") |
| `generation.output.size_bytes` | number | Final video file size in bytes |
| `generation.credits_charged` | number | Total credits charged for this generation |
| `generation.started_at` | string | ISO 8601 timestamp when processing started |
| `generation.completed_at` | string | ISO 8601 timestamp when processing completed |

#### generation.failed

Sent when a generation fails during processing.

```json
{
  "event": "generation.failed",
  "timestamp": "2025-01-29T12:05:00Z",
  "delivery_id": "uuid",
  "generation": {
    "id": "uuid",
    "status": "failed",
    "failure_type": "system",
    "error": {
      "code": "generation_failed",
      "message": "Scene 3 failed to generate after 3 attempts",
      "scene_id": "scene_3"
    },
    "progress": {
      "phase": "generating",
      "percent": 40,
      "scenes_completed": 2
    },
    "credits_charged": 50,
    "credits_refunded": 50,
    "completed_at": "timestamp"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | Event type: "generation.failed" |
| `timestamp` | string | ISO 8601 timestamp when event occurred |
| `delivery_id` | string | Unique UUID for this webhook delivery |
| `generation.id` | string | Unique generation identifier |
| `generation.status` | string | Current generation status ("failed") |
| `generation.failure_type` | string | Type of failure: "system", "timeout", "validation", or "user_error" |
| `generation.error.code` | string | Machine-readable error code |
| `generation.error.message` | string | Human-readable error description |
| `generation.error.scene_id` | string | Scene ID that caused failure (if applicable) |
| `generation.progress.phase` | string | Processing phase where failure occurred |
| `generation.progress.percent` | number | Completion percentage at time of failure (0-100) |
| `generation.progress.scenes_completed` | number | Number of scenes successfully processed before failure |
| `generation.credits_charged` | number | Credits charged before failure |
| `generation.credits_refunded` | number | Credits refunded based on failure type and progress |
| `generation.completed_at` | string | ISO 8601 timestamp when generation terminated |

#### generation.canceled

Sent when a generation is explicitly canceled by the user or system.

```json
{
  "event": "generation.canceled",
  "timestamp": "2025-01-29T12:03:00Z",
  "delivery_id": "uuid",
  "generation": {
    "id": "uuid",
    "status": "canceled",
    "canceled_by": "uuid",
    "progress": {
      "percent": 30
    },
    "credits_charged": 30,
    "credits_refunded": 19,
    "completed_at": "timestamp"
  }
}
```

| Field | Type | Description |
|-------|------|-------------|
| `event` | string | Event type: "generation.canceled" |
| `timestamp` | string | ISO 8601 timestamp when event occurred |
| `delivery_id` | string | Unique UUID for this webhook delivery |
| `generation.id` | string | Unique generation identifier |
| `generation.status` | string | Current generation status ("canceled") |
| `generation.canceled_by` | string | User ID who canceled the generation |
| `generation.progress.percent` | number | Completion percentage at cancellation time (0-100) |
| `generation.credits_charged` | number | Credits charged before cancellation |
| `generation.credits_refunded` | number | Partial refund based on progress (with 10% cancellation fee) |
| `generation.completed_at` | string | ISO 8601 timestamp when cancellation took effect |

### HTTP Headers

All webhook requests include these headers:

```
Content-Type: application/json
X-Webhook-Delivery-Id: {delivery_id}
X-Webhook-Signature: sha256={HMAC-SHA256(payload, secret)}
X-Webhook-Timestamp: {unix timestamp}
```

| Header | Description |
|--------|-------------|
| `Content-Type` | Always "application/json" |
| `X-Webhook-Delivery-Id` | Unique UUID for this delivery (used for deduplication) |
| `X-Webhook-Signature` | HMAC-SHA256 signature of the request for verification |
| `X-Webhook-Timestamp` | Unix timestamp (seconds) when webhook was sent |

### Signature Verification

To verify webhook authenticity, validate the `X-Webhook-Signature` header:

```
expected = HMAC-SHA256(
  timestamp + "." + raw_body,
  webhook.secret
)

// Verify the signature matches
if (X-Webhook-Signature == "sha256=" + expected) {
  // Signature is valid
}

// Reject if timestamp > 5 minutes old (replay protection)
if (current_timestamp - X-Webhook-Timestamp > 300) {
  // Reject as potential replay attack
}
```

**Verification Steps:**

1. Extract `X-Webhook-Timestamp` and `X-Webhook-Signature` from headers
2. Concatenate the timestamp with a dot and the raw request body
3. Calculate HMAC-SHA256 of this string using your webhook secret
4. Compare with the signature in the header (ignoring the "sha256=" prefix)
5. Reject requests with timestamps older than 5 minutes
6. Accept only if signatures match and timestamp is within acceptable range

**Example Implementation (Node.js):**

```javascript
const crypto = require('crypto');

function verifyWebhookSignature(headers, rawBody, webhookSecret) {
  const signature = headers['x-webhook-signature'];
  const timestamp = headers['x-webhook-timestamp'];

  // Check timestamp (replay protection)
  if (Math.abs(Date.now() / 1000 - parseInt(timestamp)) > 300) {
    return false;
  }

  // Verify signature
  const expected = crypto
    .createHmac('sha256', webhookSecret)
    .update(timestamp + '.' + rawBody)
    .digest('hex');

  return signature === `sha256=${expected}`;
}
```

### Delivery Guarantees

Framecast webhook delivery provides the following guarantees:

```
- At-least-once delivery: Each event will be delivered at least once, but may be delivered multiple times. Clients SHOULD use the delivery_id field to deduplicate.

- Best-effort timeliness: Events are delivered within 30 seconds of occurrence (best effort, not guaranteed).

- No ordering guarantee: For concurrent events, delivery order is not guaranteed. Clients SHOULD use timestamps and generation status to order events.
```

**Implementation Notes:**

- **Deduplication:** Store received delivery_ids and ignore duplicate events
- **Idempotency:** Design webhook handlers to be idempotent (safe to call multiple times)
- **Ordering:** Use generation status and timestamps to reconstruct correct state rather than relying on event order
- **Retry Logic:** The API will retry failed deliveries with exponential backoff for 24 hours
- **Timeouts:** Your webhook endpoint should respond within 5 seconds with HTTP 2xx status

---
