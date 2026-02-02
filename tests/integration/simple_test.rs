//! Simple integration test to verify basic infrastructure works

#[tokio::test]
async fn test_basic_infrastructure() {
    // Basic test to verify the integration test setup works
    assert_eq!(2 + 2, 4);

    // Test that we can create async runtime
    tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;

    // Test that we can access environment
    let _env_test = std::env::var("HOME").unwrap_or_default();

    println!("✅ Integration test infrastructure is working");
}

#[tokio::test]
async fn test_config_loading() {
    // Test that our configuration loading works
    use crate::common::TestConfig;

    let config = TestConfig::from_env();
    assert!(!config.database_url.is_empty());
    assert!(!config.jwt_secret.is_empty());

    println!("✅ Configuration loading works");
}

mod common;
