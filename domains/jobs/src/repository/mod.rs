//! Repository implementations for Jobs domain

pub mod job_events;
pub mod jobs;
pub mod transactions;

use sqlx::{PgPool, Postgres, Transaction};

pub use job_events::JobEventRepository;
pub use jobs::JobRepository;

/// Combined repository access for the Jobs domain
#[derive(Clone)]
pub struct JobsRepositories {
    pool: PgPool,
    pub jobs: JobRepository,
    pub job_events: JobEventRepository,
}

impl JobsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            jobs: JobRepository::new(pool.clone()),
            job_events: JobEventRepository::new(pool.clone()),
            pool,
        }
    }

    /// Begin a new database transaction.
    pub async fn begin(&self) -> std::result::Result<Transaction<'static, Postgres>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Get a reference to the underlying database pool (for CQRS cross-domain queries).
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
