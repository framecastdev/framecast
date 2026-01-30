---
name: observability
description: Logging, metrics, tracing, and debug endpoints. Use when implementing structured logging, distributed tracing, metrics, health checks, or debugging production issues.
---

# Observability & Serviceability

## Philosophy

When something goes wrong, we must be able to:
1. **Trace** the exact path through all services
2. **Inspect** state at any point in time
3. **Understand** why something failed without guessing
4. **Reproduce** issues reliably

## Three Pillars

| Pillar | Purpose | Tools |
|--------|---------|-------|
| Logs | Structured JSON with correlation IDs | `tracing` |
| Metrics | Counters, gauges, histograms | Prometheus |
| Traces | Distributed tracing, span propagation | OpenTelemetry |

## Structured Logging

Every log entry must include:

```json
{
    "timestamp": "2025-01-30T12:00:00.000Z",
    "level": "info",
    "request_id": "req_abc123",
    "trace_id": "trace_xyz789",
    "span_id": "span_456",
    "service": "splice-api",
    "environment": "production",
    "user_id": "usr_123",
    "team_id": "tm_456",
    "job_id": "job_789",
    "message": "Job status changed",
    "data": { "old_status": "processing", "new_status": "completed" }
}
```

## Tracing with `tracing` Crate

```rust
use tracing::{info, instrument, Span};

#[instrument(
    skip(ctx),
    fields(user_id = %user.id, job_id = %job_id)
)]
pub async fn cancel_job(ctx: &AppContext, user: &User, job_id: Uuid) -> Result<Job> {
    info!("Attempting to cancel job");
    
    let job = ctx.job_repo.find(job_id).await?;
    Span::current().record("job_status", &job.status.as_str());
    
    if job.is_terminal() {
        info!(current_status = %job.status, "Job already terminal");
        return Err(JobError::AlreadyTerminal(job.status));
    }
    
    let canceled = ctx.job_service.cancel(job).await?;
    info!(credits_refunded = canceled.credits_refunded, "Job canceled");
    
    Ok(canceled)
}
```

## Key Metrics

```yaml
# API
api_requests_total{endpoint, method, status}
api_request_duration_seconds{endpoint, method}

# Jobs
jobs_created_total{owner_type}
jobs_completed_total{status}
job_duration_seconds{status}
job_credits_charged_total
job_credits_refunded_total{failure_type}

# Queue
jobs_queued_current
jobs_processing_current

# External Services
anthropic_requests_total{status}
anthropic_tokens_used_total{type}
runpod_requests_total{status}
runpod_gpu_seconds_total
```

## Health Check Endpoints

```rust
// GET /health - Liveness (is process running?)
pub async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

// GET /ready - Readiness (can we serve traffic?)
pub async fn ready(State(ctx): State<AppContext>) -> Result<impl IntoResponse> {
    let (db, inngest, s3) = tokio::join!(
        ctx.db.ping(),
        ctx.inngest.ping(),
        ctx.s3.head_bucket(),
    );
    
    let all_ok = db.is_ok() && inngest.is_ok() && s3.is_ok();
    
    Ok(Json(json!({
        "status": if all_ok { "ok" } else { "degraded" },
        "checks": {
            "database": db.map(|_| "ok").unwrap_or_else(|e| e.to_string()),
            "inngest": inngest.map(|_| "ok").unwrap_or_else(|e| e.to_string()),
            "s3": s3.map(|_| "ok").unwrap_or_else(|e| e.to_string()),
        },
        "version": env!("CARGO_PKG_VERSION"),
    })))
}
```

## RunPod Debug Endpoints

Pods MUST expose for E2E debugging:

```
GET /debug/status
{
    "pod_id": "pod_abc123",
    "status": "processing",
    "gpu": {
        "name": "NVIDIA A100",
        "memory_used_mb": 32000,
        "utilization_percent": 85
    },
    "current_job": {
        "job_id": "job_xyz",
        "progress_percent": 45
    }
}

GET /debug/workflow
{
    "nodes_total": 15,
    "nodes_completed": 7,
    "nodes": [
        {"id": "KSampler_3", "status": "running", "progress": {"steps": 12, "total": 30}}
    ]
}

GET /debug/logs?lines=100&level=debug
GET /debug/queue
GET /debug/memory
GET /debug/artifacts?job_id=job_xyz
```

## Error Response Format

All errors include debugging info:

```json
{
    "error": {
        "code": "JOB_ALREADY_TERMINAL",
        "message": "Cannot cancel job that is already completed",
        "request_id": "req_abc123",
        "trace_id": "trace_xyz789",
        "details": {
            "job_id": "job_456",
            "current_status": "completed"
        },
        "help": "Jobs can only be canceled while queued or processing"
    }
}
```

## Distributed Tracing

Propagate trace context across services:

```
Client → API → Inngest → RunPod
  │        │       │        │
  └─ trace_id: abc123 ──────┘
     span_id: span_1 → span_2 → span_3
```

## Debug Endpoints (Non-Production)

```rust
#[cfg(not(feature = "production"))]
mod debug {
    // GET /debug/config - Current config (redacted secrets)
    // GET /debug/connections - Active DB connections
    // POST /debug/log-level - Change log level dynamically
}
```
