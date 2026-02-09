//! System asset integration tests (SAI-I01 through SAI-I08)

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::Value;
use tower::ServiceExt;

use framecast_teams::UserTier;

use crate::common::{create_test_jwt, ArtifactsTestApp};

/// Helper: seed a system asset directly in DB
async fn seed_system_asset(
    pool: &sqlx::PgPool,
    id: &str,
    category: &str,
    name: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO system_assets (id, category, name, description, s3_key, content_type, size_bytes, tags, created_at)
        VALUES ($1, $2::system_asset_category, $3, $4, $5, $6, $7, $8, NOW())
        ON CONFLICT (id) DO NOTHING
        "#,
    )
    .bind(id)
    .bind(category)
    .bind(name)
    .bind(format!("Test {}", name))
    .bind(format!("system-assets/{}/{}", category, id))
    .bind("audio/mpeg")
    .bind(1024_i64)
    .bind(&["test"] as &[&str])
    .execute(pool)
    .await?;
    Ok(())
}

mod test_system_assets {
    use super::*;

    #[tokio::test]
    async fn test_list_system_assets_returns_200() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_system_assets_empty() {
        let app = ArtifactsTestApp::new().await.unwrap();
        // Clean system assets table
        sqlx::query("DELETE FROM system_assets")
            .execute(&app.pool)
            .await
            .unwrap();

        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let assets: Vec<Value> = serde_json::from_slice(&body).unwrap();
        assert!(assets.is_empty());

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_system_assets_returns_seeded() {
        let app = ArtifactsTestApp::new().await.unwrap();
        sqlx::query("DELETE FROM system_assets")
            .execute(&app.pool)
            .await
            .unwrap();

        seed_system_asset(&app.pool, "asset_sfx_whoosh_01", "sfx", "Whoosh 01")
            .await
            .unwrap();
        seed_system_asset(&app.pool, "asset_music_ambient_01", "music", "Ambient 01")
            .await
            .unwrap();
        seed_system_asset(&app.pool, "asset_ambient_rain_01", "ambient", "Rain 01")
            .await
            .unwrap();

        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let assets: Vec<Value> = serde_json::from_slice(&body).unwrap();
        assert_eq!(assets.len(), 3);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_system_asset_by_id() {
        let app = ArtifactsTestApp::new().await.unwrap();
        sqlx::query("DELETE FROM system_assets")
            .execute(&app.pool)
            .await
            .unwrap();

        let asset_id = "asset_sfx_test_get";
        seed_system_asset(&app.pool, asset_id, "sfx", "Test Get")
            .await
            .unwrap();

        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri(format!("/v1/system-assets/{}", asset_id))
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let asset: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(asset["id"], asset_id);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_get_system_asset_nonexistent_returns_404() {
        let app = ArtifactsTestApp::new().await.unwrap();
        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets/nonexistent_id")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_list_ordered_by_category_name() {
        let app = ArtifactsTestApp::new().await.unwrap();
        sqlx::query("DELETE FROM system_assets")
            .execute(&app.pool)
            .await
            .unwrap();

        // Insert in reverse order
        seed_system_asset(&app.pool, "asset_sfx_zebra", "sfx", "Zebra")
            .await
            .unwrap();
        seed_system_asset(&app.pool, "asset_ambient_alpha", "ambient", "Alpha")
            .await
            .unwrap();

        let user = app.create_test_user(UserTier::Starter).await.unwrap();
        let jwt = create_test_jwt(&user, &app.config.jwt_secret).unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets")
            .header("authorization", format!("Bearer {}", jwt))
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let assets: Vec<Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(assets.len(), 2);
        // ambient comes before sfx
        assert_eq!(assets[0]["category"], "ambient");
        assert_eq!(assets[1]["category"], "sfx");

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_repo_list_by_category() {
        let app = ArtifactsTestApp::new().await.unwrap();
        sqlx::query("DELETE FROM system_assets")
            .execute(&app.pool)
            .await
            .unwrap();

        seed_system_asset(&app.pool, "asset_sfx_test_cat", "sfx", "Test Cat")
            .await
            .unwrap();
        seed_system_asset(&app.pool, "asset_music_test_cat", "music", "Test Cat Music")
            .await
            .unwrap();

        let result = app
            .state
            .repos
            .system_assets
            .list_by_category(&framecast_artifacts::SystemAssetCategory::Sfx)
            .await
            .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].category,
            framecast_artifacts::SystemAssetCategory::Sfx
        );

        app.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn test_system_assets_require_auth() {
        let app = ArtifactsTestApp::new().await.unwrap();

        let req = Request::builder()
            .method(Method::GET)
            .uri("/v1/system-assets")
            .body(Body::empty())
            .unwrap();

        let resp = app.test_router().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        app.cleanup().await.unwrap();
    }
}
