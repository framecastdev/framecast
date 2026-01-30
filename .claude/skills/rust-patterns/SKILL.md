---
name: rust-patterns
description: Rust code conventions, error handling, database access patterns. Use when implementing new features, writing Rust code, or reviewing code structure.
---

# Rust Patterns & Conventions

## Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Entities | PascalCase, singular | `User`, `Job`, `TeamMembership` |
| DTOs | Suffix with purpose | `CreateJobRequest`, `JobResponse` |
| Repositories | Suffix with `Repository` | `UserRepository`, `JobRepository` |
| Services | Suffix with `Service` | `JobService`, `CreditService` |
| Handlers | Verb prefix | `create_job`, `cancel_job`, `list_jobs` |
| Errors | Suffix with `Error` | `JobError`, `AuthError` |

## Error Handling

### Library Crates (domain, db, etc.)

Use `thiserror` for explicit error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum JobError {
    #[error("Job not found: {0}")]
    NotFound(Uuid),
    
    #[error("Job already in terminal state: {0}")]
    AlreadyTerminal(JobStatus),
    
    #[error("Insufficient credits: required {required}, available {available}")]
    InsufficientCredits { required: i32, available: i32 },
    
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
```

### Application Crate (api)

Use `anyhow` for handler-level errors, convert to HTTP responses:

```rust
use axum::{response::IntoResponse, http::StatusCode, Json};

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match &self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg.clone()),
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", "...".into()),
            ApiError::Forbidden(msg) => (StatusCode::FORBIDDEN, "FORBIDDEN", msg.clone()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg.clone()),
            ApiError::Internal(e) => {
                tracing::error!(error = %e, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", "...".into())
            }
        };
        
        (status, Json(json!({ "error": { "code": code, "message": message } }))).into_response()
    }
}
```

## Database Access (sqlx)

### Repository Pattern

```rust
pub struct JobRepository {
    pool: PgPool,
}

impl JobRepository {
    pub async fn find(&self, id: Uuid) -> Result<Option<Job>, sqlx::Error> {
        sqlx::query_as!(
            Job,
            r#"
            SELECT id, owner, status as "status: JobStatus", 
                   credits_charged, created_at
            FROM jobs WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }
    
    pub async fn create(&self, job: &NewJob) -> Result<Job, sqlx::Error> {
        sqlx::query_as!(
            Job,
            r#"
            INSERT INTO jobs (id, owner, triggered_by, spec_snapshot, status)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, owner, status as "status: JobStatus",
                      credits_charged, created_at
            "#,
            job.id, job.owner, job.triggered_by, job.spec_snapshot, job.status as _
        )
        .fetch_one(&self.pool)
        .await
    }
}
```

### Transactions

```rust
pub async fn cancel_job_with_refund(
    &self,
    job_id: Uuid,
    refund_amount: i32,
) -> Result<Job, JobError> {
    let mut tx = self.pool.begin().await?;
    
    // Update job
    let job = sqlx::query_as!(...)
        .fetch_one(&mut *tx)
        .await?;
    
    // Update credits
    sqlx::query!(
        "UPDATE users SET credits = credits + $1 WHERE id = $2",
        refund_amount, job.triggered_by
    )
    .execute(&mut *tx)
    .await?;
    
    tx.commit().await?;
    Ok(job)
}
```

## Recommended Crates

| Purpose | Crate |
|---------|-------|
| HTTP Framework | `axum` |
| Database | `sqlx` |
| Serialization | `serde`, `serde_json` |
| Error (libs) | `thiserror` |
| Error (apps) | `anyhow` |
| Validation | `validator` |
| Async | `tokio` |
| HTTP Client | `reqwest` |
| UUID | `uuid` |
| Time | `time` |
| Tracing | `tracing` |
| AWS | `aws-sdk-*` |
| Lambda | `lambda_http`, `cargo-lambda` |
| Testing | `mockall`, `wiremock` |
| Config | `config` |

## Handler Pattern

```rust
pub async fn create_job(
    State(ctx): State<AppContext>,
    AuthUser(user): AuthUser,
    Json(req): Json<CreateJobRequest>,
) -> Result<Json<JobResponse>, ApiError> {
    // Validate
    req.validate()?;
    
    // Check permissions
    ctx.auth.check_can_create_job(&user, &req.owner)?;
    
    // Execute
    let job = ctx.job_service.create(&user, req).await?;
    
    Ok(Json(JobResponse::from(job)))
}
```

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_urn_valid() {
        let urn = "framecast:user:usr_abc123";
        let parsed = Urn::parse(urn).unwrap();
        assert_eq!(parsed.owner_type(), OwnerType::User);
    }

    #[tokio::test]
    async fn create_job_charges_credits() {
        let ctx = TestContext::new().await;
        let user = ctx.create_user_with_credits(100).await;
        
        let job = ctx.job_service.create(&user, valid_spec()).await.unwrap();
        
        let updated = ctx.user_repo.find(user.id).await.unwrap();
        assert!(updated.credits < 100);
    }
}
```

## Project Structure

```
crates/
├── api/            # Lambda handlers, middleware
├── domain/         # Entities, services, validation
├── db/             # Repositories, migrations
├── inngest/        # Job orchestration
├── comfyui/        # RunPod client
├── anthropic/      # LLM integration
└── common/         # Config, URN, ID generation
```
