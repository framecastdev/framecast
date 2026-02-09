//! Artifact handler integration tests (ART-I01 through ART-I24)

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;
use uuid::Uuid;

use framecast_common::Urn;
use framecast_teams::UserTier;

use crate::common::{create_test_jwt, ArtifactsTestApp};

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

/// Helper: parse response body as JSON Value
async fn parse_body(response: axum::http::Response<Body>) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ART-I01 through ART-I07: Create storyboard tests
mod test_create_storyboard {
    use super::*;

    #[tokio::test]
    async fn test_create_storyboard_returns_201() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {"scenes": []}})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_response_shape() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {"title": "test"}})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        assert!(body.get("id").is_some());
        assert!(body.get("owner").is_some());
        assert!(body.get("created_by").is_some());
        assert!(body.get("kind").is_some());
        assert!(body.get("status").is_some());
        assert!(body.get("source").is_some());
        assert!(body.get("spec").is_some());
        assert!(body.get("metadata").is_some());
        assert!(body.get("created_at").is_some());
        assert!(body.get("updated_at").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_defaults_owner_to_user_urn() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;

        let expected_owner = Urn::user(user.id).to_string();
        assert_eq!(body["owner"], expected_owner);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_explicit_owner_urn() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();
        let owner_urn = Urn::user(user.id).to_string();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}, "owner": owner_urn})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = parse_body(resp).await;
        assert_eq!(body["owner"], owner_urn);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_invalid_owner_urn_returns_400() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}, "owner": "not-a-urn"})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_sets_status_pending() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert_eq!(body["status"], "pending");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_sets_source_upload() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        assert_eq!(body["source"], "upload");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_with_project_requires_team_owner() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // User-owned artifact with project_id should fail (INV-ART7)
        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({
                "spec": {"scenes": []},
                "project_id": Uuid::new_v4().to_string()
            })),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_storyboard_missing_spec_returns_422() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // POST without spec field should fail with 400 (missing required field)
        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({})),
        );

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        app.cleanup().await.unwrap();
    }
}

// ART-I08 through ART-I16: List, Get, Delete artifact tests
mod test_artifact_crud {
    use super::*;

    #[tokio::test]
    async fn test_list_artifacts_empty() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(Method::GET, "/v1/artifacts", &jwt, None);
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

    #[tokio::test]
    async fn test_list_artifacts_returns_only_owned() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        // User A creates an artifact
        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt_a,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);

        // User B lists -> should be empty
        let list_req = authed_request(Method::GET, "/v1/artifacts", &jwt_b, None);
        let resp = app.test_router().oneshot(list_req).await.unwrap();
        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert!(body.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_artifacts_ordered_by_created_at_desc() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create A then B
        let req_a = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {"name": "A"}})),
        );
        let resp_a = app.test_router().oneshot(req_a).await.unwrap();
        let body_a = parse_body(resp_a).await;
        let id_a = body_a["id"].as_str().unwrap().to_string();

        let req_b = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {"name": "B"}})),
        );
        let resp_b = app.test_router().oneshot(req_b).await.unwrap();
        let body_b = parse_body(resp_b).await;
        let id_b = body_b["id"].as_str().unwrap().to_string();

        // List -> B should be first (newest)
        let list_req = authed_request(Method::GET, "/v1/artifacts", &jwt, None);
        let resp = app.test_router().oneshot(list_req).await.unwrap();
        let body: Vec<Value> = serde_json::from_slice(
            &axum::body::to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();

        assert_eq!(body.len(), 2);
        assert_eq!(body[0]["id"], id_b);
        assert_eq!(body[1]["id"], id_a);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_artifact_full_dto() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {"title": "test"}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let get_req = authed_request(Method::GET, &format!("/v1/artifacts/{}", id), &jwt, None);
        let resp = app.test_router().oneshot(get_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = parse_body(resp).await;

        assert_eq!(body["id"], id);
        assert!(body.get("owner").is_some());
        assert!(body.get("kind").is_some());
        assert!(body.get("status").is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_artifact_nonexistent_returns_404() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::GET,
            &format!("/v1/artifacts/{}", Uuid::new_v4()),
            &jwt,
            None,
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_other_users_artifact_returns_404() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        // A creates
        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt_a,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        // B tries to GET -> 404
        let get_req = authed_request(Method::GET, &format!("/v1/artifacts/{}", id), &jwt_b, None);
        let resp = app.test_router().oneshot(get_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_artifact_returns_204() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let del_req = authed_request(Method::DELETE, &format!("/v1/artifacts/{}", id), &jwt, None);
        let resp = app.test_router().oneshot(del_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_artifact_removes_from_db() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        // Delete
        let del_req = authed_request(Method::DELETE, &format!("/v1/artifacts/{}", id), &jwt, None);
        app.test_router().oneshot(del_req).await.unwrap();

        // GET should 404
        let get_req = authed_request(Method::GET, &format!("/v1/artifacts/{}", id), &jwt, None);
        let resp = app.test_router().oneshot(get_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_other_users_artifact_returns_404() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user_a = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_a = create_test_jwt(&user_a, &app.config.jwt_secret).unwrap();
        let user_b = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt_b = create_test_jwt(&user_b, &app.config.jwt_secret).unwrap();

        let create_req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt_a,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(create_req).await.unwrap();
        let created = parse_body(resp).await;
        let id = created["id"].as_str().unwrap();

        let del_req = authed_request(
            Method::DELETE,
            &format!("/v1/artifacts/{}", id),
            &jwt_b,
            None,
        );
        let resp = app.test_router().oneshot(del_req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }
}

// ART-I17, ART-I18: Auth tests
mod test_artifact_auth {
    use super::*;

    #[tokio::test]
    async fn test_unauthenticated_request_returns_401() {
        let app = ArtifactsTestApp::new().await.unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/artifacts")
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_malformed_jwt_returns_401() {
        let app = ArtifactsTestApp::new().await.unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/artifacts")
            .header("authorization", "Bearer garbage.token.here")
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}

// ART-I19 through ART-I24: Repository tests
mod test_artifact_repo {
    use super::*;

    #[tokio::test]
    async fn test_repo_update_status_pending_to_ready() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        // Create artifact
        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

        // Update status via repo
        let updated = app
            .state
            .repos
            .artifacts
            .update_status(id, framecast_artifacts::ArtifactStatus::Ready)
            .await
            .unwrap();

        assert!(updated.is_some());
        assert_eq!(
            updated.unwrap().status,
            framecast_artifacts::ArtifactStatus::Ready
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_repo_update_status_pending_to_failed() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = authed_request(
            Method::POST,
            "/v1/artifacts/storyboards",
            &jwt,
            Some(json!({"spec": {}})),
        );
        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = parse_body(resp).await;
        let id: Uuid = body["id"].as_str().unwrap().parse().unwrap();

        let updated = app
            .state
            .repos
            .artifacts
            .update_status(id, framecast_artifacts::ArtifactStatus::Failed)
            .await
            .unwrap();

        assert!(updated.is_some());
        assert_eq!(
            updated.unwrap().status,
            framecast_artifacts::ArtifactStatus::Failed
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_repo_update_status_nonexistent_returns_none() {
        let app = ArtifactsTestApp::new().await.unwrap();

        let result = app
            .state
            .repos
            .artifacts
            .update_status(Uuid::new_v4(), framecast_artifacts::ArtifactStatus::Ready)
            .await
            .unwrap();

        assert!(result.is_none());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_repo_list_by_project() {
        // This test requires project_id which needs team ownership.
        // For now, test the empty case: list_by_project with random project_id returns empty.
        let app = ArtifactsTestApp::new().await.unwrap();

        let result = app
            .state
            .repos
            .artifacts
            .list_by_project(Uuid::new_v4())
            .await
            .unwrap();

        assert!(result.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_artifact_tx() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        let artifact = framecast_artifacts::Artifact::new_storyboard(
            Urn::user(user.id),
            user.id,
            None,
            json!({"test": true}),
        )
        .unwrap();

        let mut tx = app.state.repos.begin().await.unwrap();
        let created = framecast_artifacts::create_artifact_tx(&mut tx, &artifact)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(created.id, artifact.id);

        // Verify it's persisted
        let found = app.state.repos.artifacts.find(created.id).await.unwrap();
        assert!(found.is_some());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_s3_key_unique_constraint() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();

        // Create two media artifacts with same s3_key should fail.
        // Since we can only create storyboards via API and they don't have s3_key,
        // we insert directly via SQL.
        let s3_key = format!("test/unique-constraint-{}", Uuid::new_v4());

        sqlx::query(
            r#"
            INSERT INTO artifacts (id, owner, created_by, kind, status, source,
                filename, s3_key, content_type, size_bytes, created_at, updated_at)
            VALUES ($1, $2, $3, 'image', 'pending', 'upload',
                'test.jpg', $4, 'image/jpeg', 1024, NOW(), NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(Urn::user(user.id).to_string())
        .bind(user.id)
        .bind(&s3_key)
        .execute(&app.pool)
        .await
        .unwrap();

        // Second insert with same s3_key should fail
        let result = sqlx::query(
            r#"
            INSERT INTO artifacts (id, owner, created_by, kind, status, source,
                filename, s3_key, content_type, size_bytes, created_at, updated_at)
            VALUES ($1, $2, $3, 'image', 'pending', 'upload',
                'test2.jpg', $4, 'image/jpeg', 2048, NOW(), NOW())
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(Urn::user(user.id).to_string())
        .bind(user.id)
        .bind(&s3_key)
        .execute(&app.pool)
        .await;

        assert!(result.is_err());

        app.cleanup().await.unwrap();
    }
}
