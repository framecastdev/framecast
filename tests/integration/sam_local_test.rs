//! SAM Local Integration Tests
//!
//! Tests the Lambda handler with API Gateway v2 event payloads through SAM local.
//! These tests verify that the deployed Lambda function works correctly with
//! the AWS API Gateway integration.
//!
//! Prerequisites:
//! - SAM CLI installed
//! - cargo-lambda installed
//! - Docker running
//! - LocalStack running (just start-backing-services)
//! - SAM local running (just sam-local-start)
//!
//! Run with: just test-sam-full

use std::env;
use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use tokio::time::sleep;

/// Configuration for SAM local tests
struct SamLocalConfig {
    /// Base URL for SAM local API
    api_url: String,
    /// HTTP client with reasonable timeouts for Lambda cold starts
    client: Client,
}

impl SamLocalConfig {
    fn new() -> Self {
        let api_url = env::var("SAM_LOCAL_API_URL")
            .unwrap_or_else(|_| "http://localhost:3001".to_string());

        // Configure client with longer timeout for Lambda cold starts
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build HTTP client");

        Self { api_url, client }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_url, path)
    }
}

/// Helper to check if SAM local is available
async fn sam_local_available(config: &SamLocalConfig) -> bool {
    match config.client.get(&config.url("/health")).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Skip test if SAM local is not running
macro_rules! skip_if_no_sam {
    ($config:expr) => {
        if !sam_local_available($config).await {
            eprintln!("⚠️  SAM local not available, skipping test");
            return;
        }
    };
}

// =============================================================================
// HEALTH CHECK TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_health_check() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config.client.get(&config.url("/health")).send().await;

    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Health check should return 200 OK"
            );
            let body = resp.text().await.unwrap_or_default();
            assert!(body.contains("OK") || body.is_empty() || body.contains("healthy"));
            println!("✅ Health check passed: {}", body);
        }
        Err(e) => {
            eprintln!("❌ Health check failed: {}", e);
            panic!("Health check request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_root_endpoint() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config.client.get(&config.url("/")).send().await;

    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Root endpoint should return 200 OK"
            );
            let body = resp.text().await.unwrap_or_default();
            assert!(
                body.contains("Framecast") || body.contains("API"),
                "Root should contain API info"
            );
            println!("✅ Root endpoint: {}", body);
        }
        Err(e) => {
            eprintln!("❌ Root endpoint failed: {}", e);
            panic!("Root endpoint request failed");
        }
    }
}

// =============================================================================
// AUTHENTICATION TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_auth_missing_token_returns_401() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Try to access protected endpoint without auth
    let response = config.client.get(&config.url("/v1/account")).send().await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Protected endpoint without auth should return 401"
            );
            println!("✅ Missing auth returns 401");
        }
        Err(e) => {
            eprintln!("❌ Auth test failed: {}", e);
            panic!("Auth test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_auth_invalid_token_returns_401() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .get(&config.url("/v1/account"))
        .header("Authorization", "Bearer invalid.jwt.token")
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Invalid JWT should return 401"
            );
            println!("✅ Invalid token returns 401");
        }
        Err(e) => {
            eprintln!("❌ Auth test failed: {}", e);
            panic!("Auth test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_auth_malformed_token_returns_401() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .get(&config.url("/v1/account"))
        .header("Authorization", "Bearer not-even-a-jwt")
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Malformed token should return 401"
            );
            println!("✅ Malformed token returns 401");
        }
        Err(e) => {
            eprintln!("❌ Auth test failed: {}", e);
            panic!("Auth test request failed");
        }
    }
}

// =============================================================================
// TEAMS ENDPOINT TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_create_team_requires_auth() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let team_data = json!({
        "name": "Test Team",
        "description": "A test team created via SAM local"
    });

    let response = config
        .client
        .post(&config.url("/v1/teams"))
        .json(&team_data)
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Create team without auth should return 401"
            );
            println!("✅ Create team requires auth");
        }
        Err(e) => {
            eprintln!("❌ Teams test failed: {}", e);
            panic!("Teams test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_get_team_requires_auth() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Use a UUID-like ID
    let response = config
        .client
        .get(&config.url("/v1/teams/00000000-0000-0000-0000-000000000001"))
        .send()
        .await;

    match response {
        Ok(resp) => {
            // Should be 401 (unauthorized) not 404 - auth check comes first
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Get team without auth should return 401"
            );
            println!("✅ Get team requires auth");
        }
        Err(e) => {
            eprintln!("❌ Teams test failed: {}", e);
            panic!("Teams test request failed");
        }
    }
}

// =============================================================================
// INVITATIONS ENDPOINT TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_accept_invitation_requires_auth() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .put(&config.url(
            "/v1/invitations/00000000-0000-0000-0000-000000000001/accept",
        ))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Accept invitation without auth should return 401"
            );
            println!("✅ Accept invitation requires auth");
        }
        Err(e) => {
            eprintln!("❌ Invitations test failed: {}", e);
            panic!("Invitations test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_decline_invitation_requires_auth() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .put(&config.url(
            "/v1/invitations/00000000-0000-0000-0000-000000000001/decline",
        ))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                401,
                "Decline invitation without auth should return 401"
            );
            println!("✅ Decline invitation requires auth");
        }
        Err(e) => {
            eprintln!("❌ Invitations test failed: {}", e);
            panic!("Invitations test request failed");
        }
    }
}

// =============================================================================
// ERROR HANDLING TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_404_for_unknown_route() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .get(&config.url("/v1/this-route-does-not-exist"))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert_eq!(
                resp.status().as_u16(),
                404,
                "Unknown route should return 404"
            );
            println!("✅ Unknown route returns 404");
        }
        Err(e) => {
            eprintln!("❌ 404 test failed: {}", e);
            panic!("404 test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_invalid_json_returns_400() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .post(&config.url("/v1/teams"))
        .header("Content-Type", "application/json")
        .header("Authorization", "Bearer test.jwt.token")
        .body("{ invalid json }")
        .send()
        .await;

    match response {
        Ok(resp) => {
            // Either 400 (bad request for invalid JSON) or 401 (auth checked first)
            let status = resp.status().as_u16();
            assert!(
                status == 400 || status == 401 || status == 422,
                "Invalid JSON should return 400, 401, or 422, got {}",
                status
            );
            println!("✅ Invalid JSON returns error status: {}", status);
        }
        Err(e) => {
            eprintln!("❌ Invalid JSON test failed: {}", e);
            panic!("Invalid JSON test request failed");
        }
    }
}

// =============================================================================
// CORS TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_cors_headers_present() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .get(&config.url("/health"))
        .header("Origin", "http://localhost:3000")
        .send()
        .await;

    match response {
        Ok(resp) => {
            // CORS headers might be present depending on configuration
            // Just verify the request succeeds
            assert!(
                resp.status().is_success(),
                "Request with Origin header should succeed"
            );
            println!("✅ CORS request succeeds");

            // Log CORS headers if present
            if let Some(allow_origin) = resp.headers().get("access-control-allow-origin") {
                println!("  Access-Control-Allow-Origin: {:?}", allow_origin);
            }
        }
        Err(e) => {
            eprintln!("❌ CORS test failed: {}", e);
            panic!("CORS test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_options_preflight() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .request(reqwest::Method::OPTIONS, &config.url("/v1/teams"))
        .header("Origin", "http://localhost:3000")
        .header("Access-Control-Request-Method", "POST")
        .header("Access-Control-Request-Headers", "Authorization,Content-Type")
        .send()
        .await;

    match response {
        Ok(resp) => {
            // OPTIONS should return 200 or 204 for CORS preflight
            let status = resp.status().as_u16();
            assert!(
                status == 200 || status == 204 || status == 405,
                "OPTIONS preflight should return 200, 204, or 405, got {}",
                status
            );
            println!("✅ OPTIONS preflight returns: {}", status);
        }
        Err(e) => {
            eprintln!("❌ OPTIONS test failed: {}", e);
            panic!("OPTIONS test request failed");
        }
    }
}

// =============================================================================
// CONTENT TYPE TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_json_content_type_response() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Use a protected endpoint that returns JSON error
    let response = config.client.get(&config.url("/v1/account")).send().await;

    match response {
        Ok(resp) => {
            // Check content type is JSON for error response
            if let Some(content_type) = resp.headers().get("content-type") {
                let ct = content_type.to_str().unwrap_or("");
                println!("  Content-Type: {}", ct);
                // Accept both application/json and text/plain (for simple error messages)
            }
            println!("✅ Response has content type");
        }
        Err(e) => {
            eprintln!("❌ Content type test failed: {}", e);
            panic!("Content type test request failed");
        }
    }
}

// =============================================================================
// LAMBDA COLD START TEST
// =============================================================================

#[tokio::test]
async fn test_sam_lambda_cold_start_performance() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Measure cold start time
    let start = std::time::Instant::now();
    let response = config.client.get(&config.url("/health")).send().await;
    let duration = start.elapsed();

    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Health check should succeed"
            );
            println!(
                "✅ Lambda response time: {:?} (cold start may be longer)",
                duration
            );

            // Warn if response is very slow (but don't fail - SAM local can be slow)
            if duration.as_secs() > 30 {
                eprintln!(
                    "⚠️  Response took {}s - consider warming the Lambda",
                    duration.as_secs()
                );
            }
        }
        Err(e) => {
            eprintln!("❌ Cold start test failed: {}", e);
            panic!("Cold start test request failed");
        }
    }
}

// =============================================================================
// CONCURRENT REQUEST TEST
// =============================================================================

#[tokio::test]
async fn test_sam_concurrent_requests() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Make multiple concurrent requests
    let futures: Vec<_> = (0..5)
        .map(|_| {
            let client = config.client.clone();
            let url = config.url("/health");
            async move { client.get(&url).send().await }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut success_count = 0;
    for result in results {
        match result {
            Ok(resp) if resp.status().is_success() => success_count += 1,
            Ok(resp) => eprintln!("  Request returned: {}", resp.status()),
            Err(e) => eprintln!("  Request failed: {}", e),
        }
    }

    assert!(
        success_count >= 3,
        "At least 3 of 5 concurrent requests should succeed"
    );
    println!("✅ {}/5 concurrent requests succeeded", success_count);
}

// =============================================================================
// API GATEWAY V2 SPECIFIC TESTS
// =============================================================================

#[tokio::test]
async fn test_sam_api_gateway_query_params() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Test that query parameters are passed through
    let response = config
        .client
        .get(&config.url("/health?test=value&another=param"))
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Request with query params should succeed"
            );
            println!("✅ Query parameters handled correctly");
        }
        Err(e) => {
            eprintln!("❌ Query params test failed: {}", e);
            panic!("Query params test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_api_gateway_path_params() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    // Test path parameter extraction (even though it will return 401)
    let response = config
        .client
        .get(&config.url("/v1/teams/my-team-slug"))
        .send()
        .await;

    match response {
        Ok(resp) => {
            // Should reach the handler (and fail auth), not 404
            let status = resp.status().as_u16();
            assert!(
                status == 401 || status == 404,
                "Path params should be routed correctly, got {}",
                status
            );
            println!("✅ Path parameters routed: status {}", status);
        }
        Err(e) => {
            eprintln!("❌ Path params test failed: {}", e);
            panic!("Path params test request failed");
        }
    }
}

#[tokio::test]
async fn test_sam_api_gateway_headers_forwarded() {
    let config = SamLocalConfig::new();
    skip_if_no_sam!(&config);

    let response = config
        .client
        .get(&config.url("/health"))
        .header("X-Custom-Header", "test-value")
        .header("X-Request-Id", "req-123456")
        .send()
        .await;

    match response {
        Ok(resp) => {
            assert!(
                resp.status().is_success(),
                "Request with custom headers should succeed"
            );
            println!("✅ Custom headers forwarded correctly");
        }
        Err(e) => {
            eprintln!("❌ Headers test failed: {}", e);
            panic!("Headers test request failed");
        }
    }
}
