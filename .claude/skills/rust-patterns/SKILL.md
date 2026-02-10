---
name: rust-patterns
description: Rust code conventions, error handling, database access patterns. Use when implementing new features, writing Rust code, or reviewing code structure.
---

# Rust Patterns & Conventions

## Naming Conventions

| Type | Convention | Example |
|------|------------|---------|
| Entities | PascalCase, singular | `User`, `Generation`, `TeamMembership` |
| DTOs | Suffix with purpose | `CreateGenerationRequest`, `GenerationResponse` |
| Repositories | Suffix with `Repository` | `UserRepository`, `GenerationRepository` |
| Services | Suffix with `Service` | `GenerationService`, `CreditService` |
| Handlers | Verb prefix | `create_generation`, `cancel_generation`, `list_generations` |
| Errors | Suffix with `Error` | `GenerationError`, `AuthError` |

## Error Handling

### Library Crates (domain, db, etc.)

Use `thiserror` for explicit error types:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GenerationError {
    #[error("Generation not found: {0}")]
    NotFound(Uuid),

    #[error("Generation already in terminal state: {0}")]
    AlreadyTerminal(GenerationStatus),

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
pub struct GenerationRepository {
    pool: PgPool,
}

impl GenerationRepository {
    pub async fn find(&self, id: Uuid) -> Result<Option<Generation>, sqlx::Error> {
        sqlx::query_as!(
            Generation,
            r#"
            SELECT id, owner, status as "status: GenerationStatus",
                   credits_charged, created_at
            FROM generations WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn create(&self, generation: &NewGeneration) -> Result<Generation, sqlx::Error> {
        sqlx::query_as!(
            Generation,
            r#"
            INSERT INTO generations (id, owner, triggered_by, spec_snapshot, status)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, owner, status as "status: GenerationStatus",
                      credits_charged, created_at
            "#,
            generation.id, generation.owner, generation.triggered_by, generation.spec_snapshot, generation.status as _
        )
        .fetch_one(&self.pool)
        .await
    }
}
```

### Transactions

```rust
pub async fn cancel_generation_with_refund(
    &self,
    generation_id: Uuid,
    refund_amount: i32,
) -> Result<Generation, GenerationError> {
    let mut tx = self.pool.begin().await?;

    // Update generation
    let generation = sqlx::query_as!(...)
        .fetch_one(&mut *tx)
        .await?;

    // Update credits
    sqlx::query!(
        "UPDATE users SET credits = credits + $1 WHERE id = $2",
        refund_amount, generation.triggered_by
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(generation)
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
pub async fn create_generation(
    State(ctx): State<AppContext>,
    AuthUser(user): AuthUser,
    Json(req): Json<CreateGenerationRequest>,
) -> Result<Json<GenerationResponse>, ApiError> {
    // Validate
    req.validate()?;

    // Check permissions
    ctx.auth.check_can_create_generation(&user, &req.owner)?;

    // Execute
    let generation = ctx.generation_service.create(&user, req).await?;

    Ok(Json(GenerationResponse::from(generation)))
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
    async fn create_generation_charges_credits() {
        let ctx = TestContext::new().await;
        let user = ctx.create_user_with_credits(100).await;

        let generation = ctx.generation_service.create(&user, valid_spec()).await.unwrap();

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
├── inngest/        # Generation orchestration
├── comfyui/        # RunPod client
├── anthropic/      # LLM integration
└── common/         # Config, URN, ID generation
```
