//! Repository implementations for Artifacts domain

pub mod artifacts;
pub mod system_assets;
pub mod transactions;

use sqlx::{PgPool, Postgres, Transaction};

pub use artifacts::ArtifactRepository;
pub use system_assets::SystemAssetRepository;
pub use transactions::create_artifact_tx;

/// Combined repository access for the Artifacts domain
#[derive(Clone)]
pub struct ArtifactsRepositories {
    pool: PgPool,
    pub artifacts: ArtifactRepository,
    pub system_assets: SystemAssetRepository,
}

impl ArtifactsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            artifacts: ArtifactRepository::new(pool.clone()),
            system_assets: SystemAssetRepository::new(pool.clone()),
            pool,
        }
    }

    /// Begin a new database transaction.
    pub async fn begin(&self) -> std::result::Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }
}
