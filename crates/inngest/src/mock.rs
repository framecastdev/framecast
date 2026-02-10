//! Mock Inngest Service Implementation
//!
//! Stores events in memory for test assertions.
//! Thread-safe via `Arc<Mutex<>>`.

use crate::{InngestError, InngestEvent, InngestService};
use std::sync::{Arc, Mutex};

/// Mock Inngest service that records events for test assertions.
#[derive(Debug, Clone)]
pub struct MockInngestService {
    events: Arc<Mutex<Vec<InngestEvent>>>,
}

impl MockInngestService {
    /// Create a new mock Inngest service.
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Return all recorded events.
    pub fn recorded_events(&self) -> Vec<InngestEvent> {
        self.events.lock().unwrap().clone()
    }

    /// Clear all recorded events.
    pub fn reset(&self) {
        self.events.lock().unwrap().clear();
    }
}

impl Default for MockInngestService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl InngestService for MockInngestService {
    async fn send_event(&self, event: InngestEvent) -> Result<(), InngestError> {
        tracing::debug!(event_name = %event.name, "Mock Inngest: recording event");
        self.events.lock().unwrap().push(event);
        Ok(())
    }

    async fn send_events(&self, events: Vec<InngestEvent>) -> Result<(), InngestError> {
        tracing::debug!(count = events.len(), "Mock Inngest: recording events");
        let mut stored = self.events.lock().unwrap();
        stored.extend(events);
        Ok(())
    }
}
