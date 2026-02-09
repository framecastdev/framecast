//! Artifacts domain: creative outputs (storyboards, media), system assets

pub mod api;
pub mod domain;
pub mod repository;

// Re-export domain types at the crate root for convenience
pub use domain::entities::{
    Artifact, ArtifactKind, ArtifactSource, ArtifactStatus, SystemAsset, SystemAssetCategory,
};
pub use domain::state::{ArtifactEvent, ArtifactState, ArtifactStateMachine, StateError};

// Re-export repository types
pub use repository::{
    create_artifact_tx, ArtifactRepository, ArtifactsRepositories, SystemAssetRepository,
};

// Re-export API types
pub use api::routes;
pub use api::ArtifactsState;
