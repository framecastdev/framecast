//! Repository implementations for Generations domain

pub mod generation_events;
pub mod generations;
pub mod transactions;

use sqlx::{PgPool, Postgres, Transaction};

pub use generation_events::GenerationEventRepository;
pub use generations::GenerationRepository;

/// Combined repository access for the Generations domain
#[derive(Clone)]
pub struct GenerationsRepositories {
    pool: PgPool,
    pub generations: GenerationRepository,
    pub generation_events: GenerationEventRepository,
}

impl GenerationsRepositories {
    pub fn new(pool: PgPool) -> Self {
        Self {
            generations: GenerationRepository::new(pool.clone()),
            generation_events: GenerationEventRepository::new(pool.clone()),
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
