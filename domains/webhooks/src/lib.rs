//! Webhooks domain: webhooks, webhook deliveries

pub mod domain;

// Re-export domain types at the crate root for convenience
pub use domain::entities::*;
pub use domain::state::{
    StateError, WebhookDeliveryEvent, WebhookDeliveryGuardContext, WebhookDeliveryState,
    WebhookDeliveryStateMachine,
};
