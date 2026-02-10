//! Jobs domain integration tests (JI-01 through JI-40)

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_common::Urn;
use framecast_teams::UserTier;

use crate::common::{create_test_jwt, JobsTestApp};

/// Helper: build an authenticated request
fn authed_request(method: Method, uri: &str, jwt: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {}", jwt));

    if let Some(b) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(serde_json::to_string(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

/// Helper: build an unauthenticated request
fn unauthed_request(method: Method, uri: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);

    if let Some(b) = body {
        builder = builder.header("content-type", "application/json");
        builder
            .body(Body::from(serde_json::to_string(&b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

/// Helper: parse response body as JSON Value
async fn parse_body(response: axum::http::Response<Body>) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

/// Helper: create an ephemeral job and return the parsed response body
async fn create_job(
    app: &JobsTestApp,
    jwt: &str,
    spec: Value,
    idempotency_key: Option<&str>,
) -> (StatusCode, Value) {
    let mut body = json!({ "spec": spec });
    if let Some(key) = idempotency_key {
        body["idempotency_key"] = json!(key);
    }

    let req = authed_request(Method::POST, "/v1/generate", jwt, Some(body));
    let resp = app.test_router().oneshot(req).await.unwrap();
    let status = resp.status();
    let parsed = parse_body(resp).await;
    (status, parsed)
}

/// Helper: send a callback to the internal endpoint
async fn send_callback(app: &JobsTestApp, payload: Value) -> (StatusCode, Value) {
    let req = unauthed_request(Method::POST, "/internal/jobs/callback", Some(payload));
    let resp = app.test_router().oneshot(req).await.unwrap();
    let status = resp.status();
    let parsed = parse_body(resp).await;
    (status, parsed)
}

/// Helper: insert a test artifact directly into the DB for render testing
async fn insert_test_artifact(
    pool: &sqlx::PgPool,
    artifact_id: Uuid,
    user_id: Uuid,
    kind: &str,
) -> anyhow::Result<()> {
    let owner_urn = Urn::user(user_id).to_string();
    sqlx::query(
        r#"
        INSERT INTO artifacts (id, owner, created_by, project_id, kind, status, source,
                               filename, s3_key, content_type, size_bytes, spec,
                               conversation_id, source_job_id, metadata, created_at, updated_at)
        VALUES ($1, $2, $3, NULL, $4::artifact_kind, 'ready'::asset_status, 'upload'::artifact_source,
                'test.png', $5, 'image/png', 100, $6,
                NULL, NULL, '{}'::jsonb, NOW(), NOW())
        "#,
    )
    .bind(artifact_id)
    .bind(&owner_urn)
    .bind(user_id)
    .bind(kind)
    .bind(format!("test/{}.png", artifact_id))
    .bind(json!({"prompt": "A brave warrior"}))
    .execute(pool)
    .await?;
    Ok(())
}

// ============================================================================
// Job Creation (JI-01 through JI-08)
// ============================================================================
mod test_job_creation {
    use super::*;

    /// JI-01: Create ephemeral job -- 201, status=queued
    #[tokio::test]
    async fn test_create_ephemeral_job_returns_201_queued() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (status, body) =
            create_job(&app, &jwt, json!({"prompt": "A brave warrior"}), None).await;

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(body["status"], "queued");

        app.cleanup().await.unwrap();
    }

    /// JI-02: Create ephemeral job -- response has all expected fields
    #[tokio::test]
    async fn test_create_ephemeral_job_response_fields() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, body) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;

        assert!(body.get("id").is_some(), "missing 'id'");
        assert!(body.get("owner").is_some(), "missing 'owner'");
        assert!(body.get("status").is_some(), "missing 'status'");
        assert!(
            body.get("spec_snapshot").is_some(),
            "missing 'spec_snapshot'"
        );
        assert!(body.get("options").is_some(), "missing 'options'");
        assert!(body.get("created_at").is_some(), "missing 'created_at'");

        let expected_owner = Urn::user(user.id).to_string();
        assert_eq!(body["owner"], expected_owner);

        app.cleanup().await.unwrap();
    }

    /// JI-03: Create ephemeral job -- job_event created (verify via DB query)
    #[tokio::test]
    async fn test_create_ephemeral_job_creates_event() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, body) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

        // Query job_events directly
        let events = app
            .state
            .repos
            .job_events
            .list_by_job(job_id, None)
            .await
            .unwrap();
        assert!(!events.is_empty(), "Expected at least one job event");
        assert_eq!(
            events[0].event_type,
            framecast_jobs::JobEventType::Queued,
            "First event should be 'queued'"
        );

        app.cleanup().await.unwrap();
    }

    /// JI-04: Create ephemeral job -- Inngest event sent
    #[tokio::test]
    async fn test_create_ephemeral_job_sends_inngest_event() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        app.mock_inngest.reset();
        let (_, body) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = body["id"].as_str().unwrap();

        let recorded = app.mock_inngest.recorded_events();
        assert!(!recorded.is_empty(), "Expected at least one Inngest event");
        assert_eq!(recorded[0].name, "framecast/job.queued");
        assert_eq!(recorded[0].data["job_id"], job_id);

        app.cleanup().await.unwrap();
    }

    /// JI-05: Create ephemeral job -- idempotency key dedup returns existing job
    #[tokio::test]
    async fn test_create_ephemeral_job_idempotency_dedup() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let key = "test-idempotency-key-1";
        let (status1, body1) = create_job(&app, &jwt, json!({"prompt": "first"}), Some(key)).await;
        assert_eq!(status1, StatusCode::CREATED);

        let (status2, body2) = create_job(&app, &jwt, json!({"prompt": "second"}), Some(key)).await;
        assert_eq!(
            status2,
            StatusCode::OK,
            "Second call should return 200 (dedup)"
        );
        assert_eq!(body1["id"], body2["id"], "Should return the same job");

        app.cleanup().await.unwrap();
    }

    /// JI-06: Create ephemeral job -- different users same idempotency key -> separate jobs
    #[tokio::test]
    async fn test_create_ephemeral_job_idempotency_different_users() {
        let app = JobsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let key = "shared-idempotency-key";
        let (status_a, body_a) = create_job(&app, &jwt_a, json!({"prompt": "a"}), Some(key)).await;
        let (status_b, body_b) = create_job(&app, &jwt_b, json!({"prompt": "b"}), Some(key)).await;

        assert_eq!(status_a, StatusCode::CREATED);
        assert_eq!(status_b, StatusCode::CREATED);
        assert_ne!(
            body_a["id"], body_b["id"],
            "Different users should get separate jobs even with same idempotency key"
        );

        app.cleanup().await.unwrap();
    }

    /// JI-07: Create ephemeral job -- missing spec -> 422
    #[tokio::test]
    async fn test_create_ephemeral_job_missing_spec() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Send request without "spec" field
        let req = authed_request(
            Method::POST,
            "/v1/generate",
            &jwt,
            Some(json!({"options": {}})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();

        // Should be 400 or 422 (missing required field causes deserialization failure)
        let status = resp.status();
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
            "Expected 400 or 422, got {}",
            status
        );

        app.cleanup().await.unwrap();
    }

    /// JI-08: Create ephemeral job -- without auth -> 401
    #[tokio::test]
    async fn test_create_ephemeral_job_no_auth() {
        let app = JobsTestApp::new().await.unwrap();

        let req = Request::builder()
            .method(Method::POST)
            .uri("/v1/generate")
            .header("content-type", "application/json")
            .body(Body::from(
                serde_json::to_string(&json!({"spec": {"prompt": "test"}})).unwrap(),
            ))
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Job Read (JI-09 through JI-14)
// ============================================================================
mod test_job_read {
    use super::*;

    /// JI-09: Get job by ID -- 200 with all fields
    #[tokio::test]
    async fn test_get_job_by_id() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(Method::GET, &format!("/v1/jobs/{}", job_id), &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = parse_body(resp).await;
        assert_eq!(body["id"], job_id);
        assert!(body.get("owner").is_some());
        assert!(body.get("status").is_some());
        assert!(body.get("spec_snapshot").is_some());
        assert!(body.get("created_at").is_some());

        app.cleanup().await.unwrap();
    }

    /// JI-10: Get job -- nonexistent UUID -> 404
    #[tokio::test]
    async fn test_get_job_nonexistent() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::GET,
            &format!("/v1/jobs/{}", Uuid::new_v4()),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    /// JI-11: Get job -- other user's job -> 404
    #[tokio::test]
    async fn test_get_job_other_user() {
        let app = JobsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt_a, json!({"prompt": "private"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(Method::GET, &format!("/v1/jobs/{}", job_id), &jwt_b, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    /// JI-12: List jobs -- empty for new user
    #[tokio::test]
    async fn test_list_jobs_empty() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(Method::GET, "/v1/jobs", &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body.is_empty());

        app.cleanup().await.unwrap();
    }

    /// JI-13: List jobs -- returns own jobs ordered by created_at DESC
    #[tokio::test]
    async fn test_list_jobs_ordered_desc() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create 3 jobs
        let (_, j1) = create_job(&app, &jwt, json!({"prompt": "first"}), None).await;
        let (_, j2) = create_job(&app, &jwt, json!({"prompt": "second"}), None).await;
        let (_, j3) = create_job(&app, &jwt, json!({"prompt": "third"}), None).await;

        let req = authed_request(Method::GET, "/v1/jobs", &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body.len(), 3);
        // Newest first
        assert_eq!(body[0]["id"], j3["id"]);
        assert_eq!(body[1]["id"], j2["id"]);
        assert_eq!(body[2]["id"], j1["id"]);

        app.cleanup().await.unwrap();
    }

    /// JI-14: List jobs -- filter by status=queued
    #[tokio::test]
    async fn test_list_jobs_filter_by_status() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create a job (queued)
        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition one to processing via callback
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Create another job (stays queued)
        let (_, queued_job) = create_job(&app, &jwt, json!({"prompt": "still queued"}), None).await;

        // Filter by status=queued
        let req = authed_request(Method::GET, "/v1/jobs?status=queued", &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body.len(), 1);
        assert_eq!(body[0]["id"], queued_job["id"]);
        assert_eq!(body[0]["status"], "queued");

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Job Cancel (JI-15 through JI-20)
// ============================================================================
mod test_job_cancel {
    use super::*;

    /// JI-15: Cancel queued job -> 200, status=canceled
    #[tokio::test]
    async fn test_cancel_queued_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = parse_body(resp).await;
        assert_eq!(body["status"], "canceled");

        app.cleanup().await.unwrap();
    }

    /// JI-16: Cancel processing job -> 200, status=canceled
    #[tokio::test]
    async fn test_cancel_processing_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition to processing
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Cancel
        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = parse_body(resp).await;
        assert_eq!(body["status"], "canceled");

        app.cleanup().await.unwrap();
    }

    /// JI-17: Cancel completed job -> 409
    #[tokio::test]
    async fn test_cancel_completed_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition to processing, then completed
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;
        let _ = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "completed",
                "output": {"url": "https://example.com/video.mp4"}
            }),
        )
        .await;

        // Try to cancel
        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        app.cleanup().await.unwrap();
    }

    /// JI-18: Cancel -- failure_type set to "canceled"
    #[tokio::test]
    async fn test_cancel_sets_failure_type() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        assert_eq!(body["failure_type"], "canceled");

        app.cleanup().await.unwrap();
    }

    /// JI-19: Cancel -- completed_at is set
    #[tokio::test]
    async fn test_cancel_sets_completed_at() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        assert!(
            body["completed_at"].is_string(),
            "completed_at should be set after cancel"
        );

        app.cleanup().await.unwrap();
    }

    /// JI-20: Cancel -- job_event created
    #[tokio::test]
    async fn test_cancel_creates_event() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id_str = created["id"].as_str().unwrap();
        let job_id: Uuid = job_id_str.parse().unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id_str),
            &jwt,
            None,
        );
        let _ = app.test_router().oneshot(req).await.unwrap();

        // Check events: should have queued + canceled
        let events = app
            .state
            .repos
            .job_events
            .list_by_job(job_id, None)
            .await
            .unwrap();
        assert!(
            events.len() >= 2,
            "Expected at least 2 events, got {}",
            events.len()
        );

        let has_canceled = events
            .iter()
            .any(|e| e.event_type == framecast_jobs::JobEventType::Canceled);
        assert!(has_canceled, "Should have a 'canceled' event");

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Job Delete (JI-21 through JI-25)
// ============================================================================
mod test_job_delete {
    use super::*;

    /// JI-21: Delete terminal ephemeral job -> 204
    #[tokio::test]
    async fn test_delete_terminal_ephemeral_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Cancel to make terminal
        let cancel_req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let _ = app.test_router().oneshot(cancel_req).await.unwrap();

        // Delete
        let del_req = authed_request(Method::DELETE, &format!("/v1/jobs/{}", job_id), &jwt, None);
        let resp = app.test_router().oneshot(del_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        app.cleanup().await.unwrap();
    }

    /// JI-22: Delete queued job -> 400 (not terminal)
    #[tokio::test]
    async fn test_delete_queued_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(Method::DELETE, &format!("/v1/jobs/{}", job_id), &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    /// JI-23: Delete processing job -> 400
    #[tokio::test]
    async fn test_delete_processing_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition to processing
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        let req = authed_request(Method::DELETE, &format!("/v1/jobs/{}", job_id), &jwt, None);
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    /// JI-24: Delete nonexistent -> 404
    #[tokio::test]
    async fn test_delete_nonexistent_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::DELETE,
            &format!("/v1/jobs/{}", Uuid::new_v4()),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    /// JI-25: Delete other user's job -> 404
    #[tokio::test]
    async fn test_delete_other_users_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt_a, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Cancel to make terminal
        let cancel_req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt_a,
            None,
        );
        let _ = app.test_router().oneshot(cancel_req).await.unwrap();

        // User B tries to delete
        let req = authed_request(
            Method::DELETE,
            &format!("/v1/jobs/{}", job_id),
            &jwt_b,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Job Clone (JI-26 through JI-30)
// ============================================================================
mod test_job_clone {
    use super::*;

    /// JI-26: Clone completed job -> 201, new ID, same spec
    #[tokio::test]
    async fn test_clone_completed_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let spec = json!({"prompt": "clone me"});
        let (_, created) = create_job(&app, &jwt, spec.clone(), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition to completed
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;
        let _ = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "completed",
                "output": {"url": "https://example.com/v.mp4"}
            }),
        )
        .await;

        // Clone
        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/clone", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = parse_body(resp).await;
        assert_ne!(body["id"], job_id, "Clone should have a new ID");
        assert_eq!(body["status"], "queued");
        assert_eq!(body["spec_snapshot"], spec);

        app.cleanup().await.unwrap();
    }

    /// JI-27: Clone failed job -> 201
    #[tokio::test]
    async fn test_clone_failed_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "fail me"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Transition to failed
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;
        let _ = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "failed",
                "error": {"message": "GPU error"}
            }),
        )
        .await;

        // Clone
        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/clone", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = parse_body(resp).await;
        assert_eq!(body["status"], "queued");

        app.cleanup().await.unwrap();
    }

    /// JI-28: Clone queued job -> 400 (not terminal)
    #[tokio::test]
    async fn test_clone_queued_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/clone", job_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    /// JI-29: Clone with owner override -> new owner in response
    #[tokio::test]
    async fn test_clone_with_owner_override() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Cancel to make terminal
        let cancel_req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/cancel", job_id),
            &jwt,
            None,
        );
        let _ = app.test_router().oneshot(cancel_req).await.unwrap();

        // Clone with explicit owner (same user - should be allowed)
        let owner_urn = Urn::user(user.id).to_string();
        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/clone", job_id),
            &jwt,
            Some(json!({"owner": owner_urn})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = parse_body(resp).await;
        assert_eq!(body["owner"], owner_urn);

        app.cleanup().await.unwrap();
    }

    /// JI-30: Clone nonexistent -> 404
    #[tokio::test]
    async fn test_clone_nonexistent_job() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/jobs/{}/clone", Uuid::new_v4()),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Callback (JI-31 through JI-37)
// ============================================================================
mod test_job_callback {
    use super::*;

    /// JI-31: Callback "started" -> job status=processing, started_at set
    #[tokio::test]
    async fn test_callback_started() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        let (status, body) =
            send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "processing");
        assert!(
            body["started_at"].is_string(),
            "started_at should be set after 'started' callback"
        );

        app.cleanup().await.unwrap();
    }

    /// JI-32: Callback "progress" -> progress updated
    #[tokio::test]
    async fn test_callback_progress() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Start
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Progress
        let (status, body) = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "progress",
                "progress_percent": 50.0
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        let progress = &body["progress"];
        assert_eq!(progress["percent"], 50.0);

        app.cleanup().await.unwrap();
    }

    /// JI-33: Callback "completed" -> job status=completed, output set
    #[tokio::test]
    async fn test_callback_completed() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Start
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Complete
        let output = json!({"url": "https://example.com/video.mp4"});
        let (status, body) = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "completed",
                "output": output,
                "output_size_bytes": 12345
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "completed");
        assert_eq!(body["output"]["url"], "https://example.com/video.mp4");
        assert_eq!(body["output_size_bytes"], 12345);
        assert!(body["completed_at"].is_string());

        app.cleanup().await.unwrap();
    }

    /// JI-34: Callback "completed" -> artifact status updated to ready
    #[tokio::test]
    async fn test_callback_completed_updates_artifact_to_ready() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Insert a character artifact for render
        let artifact_id = Uuid::new_v4();
        insert_test_artifact(&app.pool, artifact_id, user.id, "character")
            .await
            .unwrap();

        // Render the artifact (creates a job + pending output artifact)
        let render_req = authed_request(
            Method::POST,
            &format!("/v1/artifacts/{}/render", artifact_id),
            &jwt,
            None,
        );
        let render_resp = app.test_router().oneshot(render_req).await.unwrap();
        assert_eq!(render_resp.status(), StatusCode::CREATED);
        let render_body = parse_body(render_resp).await;
        let job_id = render_body["job"]["id"].as_str().unwrap();
        let output_artifact_id = render_body["artifact"]["id"].as_str().unwrap();

        // Start the job
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Complete the job
        let _ = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "completed",
                "output": {"url": "https://example.com/rendered.png"}
            }),
        )
        .await;

        // Check that the output artifact status is "ready"
        let artifact_row: (String,) =
            sqlx::query_as("SELECT status::text FROM artifacts WHERE id = $1")
                .bind(output_artifact_id.parse::<Uuid>().unwrap())
                .fetch_one(&app.pool)
                .await
                .unwrap();

        assert_eq!(artifact_row.0, "ready");

        app.cleanup().await.unwrap();
    }

    /// JI-35: Callback "failed" -> job status=failed, error set
    #[tokio::test]
    async fn test_callback_failed() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let (_, created) = create_job(&app, &jwt, json!({"prompt": "test"}), None).await;
        let job_id = created["id"].as_str().unwrap();

        // Start
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Fail
        let error_payload = json!({"message": "GPU crashed", "code": "OOM"});
        let (status, body) = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "failed",
                "error": error_payload,
                "failure_type": "system"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["status"], "failed");
        assert_eq!(body["error"]["message"], "GPU crashed");
        assert_eq!(body["failure_type"], "system");
        assert!(body["completed_at"].is_string());

        app.cleanup().await.unwrap();
    }

    /// JI-36: Callback "failed" -> artifact status updated to failed
    #[tokio::test]
    async fn test_callback_failed_updates_artifact_to_failed() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Insert a character artifact for render
        let artifact_id = Uuid::new_v4();
        insert_test_artifact(&app.pool, artifact_id, user.id, "character")
            .await
            .unwrap();

        // Render the artifact
        let render_req = authed_request(
            Method::POST,
            &format!("/v1/artifacts/{}/render", artifact_id),
            &jwt,
            None,
        );
        let render_resp = app.test_router().oneshot(render_req).await.unwrap();
        assert_eq!(render_resp.status(), StatusCode::CREATED);
        let render_body = parse_body(render_resp).await;
        let job_id = render_body["job"]["id"].as_str().unwrap();
        let output_artifact_id = render_body["artifact"]["id"].as_str().unwrap();

        // Start the job
        let _ = send_callback(&app, json!({"job_id": job_id, "event": "started"})).await;

        // Fail the job
        let _ = send_callback(
            &app,
            json!({
                "job_id": job_id,
                "event": "failed",
                "error": {"message": "render error"}
            }),
        )
        .await;

        // Check that the output artifact status is "failed"
        let artifact_row: (String,) =
            sqlx::query_as("SELECT status::text FROM artifacts WHERE id = $1")
                .bind(output_artifact_id.parse::<Uuid>().unwrap())
                .fetch_one(&app.pool)
                .await
                .unwrap();

        assert_eq!(artifact_row.0, "failed");

        app.cleanup().await.unwrap();
    }

    /// JI-37: Callback for nonexistent job -> 404
    #[tokio::test]
    async fn test_callback_nonexistent_job() {
        let app = JobsTestApp::new().await.unwrap();

        let (status, _) = send_callback(
            &app,
            json!({
                "job_id": Uuid::new_v4(),
                "event": "started"
            }),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

// ============================================================================
// Render (JI-38 through JI-40)
// ============================================================================
mod test_render {
    use super::*;

    /// JI-38: Render character -> 201, response has job (status=queued) + artifact (kind=image, status=pending)
    #[tokio::test]
    async fn test_render_character() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Insert a character artifact
        let artifact_id = Uuid::new_v4();
        insert_test_artifact(&app.pool, artifact_id, user.id, "character")
            .await
            .unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/artifacts/{}/render", artifact_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        let body = parse_body(resp).await;

        // Verify job
        assert!(body.get("job").is_some(), "Response should have 'job'");
        assert_eq!(body["job"]["status"], "queued");

        // Verify artifact
        assert!(
            body.get("artifact").is_some(),
            "Response should have 'artifact'"
        );
        assert_eq!(body["artifact"]["kind"], "image");
        assert_eq!(body["artifact"]["status"], "pending");
        assert_eq!(body["artifact"]["source"], "job");
        assert!(
            body["artifact"]["source_job_id"].is_string(),
            "artifact should have source_job_id"
        );
        assert_eq!(body["artifact"]["source_job_id"], body["job"]["id"]);

        app.cleanup().await.unwrap();
    }

    /// JI-39: Render storyboard -> 201, response has job + artifact (kind=video, status=pending)
    #[tokio::test]
    async fn test_render_storyboard_not_renderable() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Insert a storyboard artifact
        let artifact_id = Uuid::new_v4();
        insert_test_artifact(&app.pool, artifact_id, user.id, "storyboard")
            .await
            .unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/artifacts/{}/render", artifact_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    /// JI-40: Render image artifact -> 400 (not renderable)
    #[tokio::test]
    async fn test_render_image_not_renderable() {
        let app = JobsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Creator).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Insert an image artifact (not renderable)
        let artifact_id = Uuid::new_v4();
        insert_test_artifact(&app.pool, artifact_id, user.id, "image")
            .await
            .unwrap();

        let req = authed_request(
            Method::POST,
            &format!("/v1/artifacts/{}/render", artifact_id),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }
}
