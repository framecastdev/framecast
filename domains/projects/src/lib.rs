//! Projects domain: projects, asset files

pub mod domain;

// Re-export domain types at the crate root for convenience
// Note: SystemAsset and SystemAssetCategory have been relocated to framecast-artifacts
pub use domain::entities::{AssetFile, AssetStatus, Project, ProjectStatus};
pub use domain::state::{ProjectEvent, ProjectState, ProjectStateMachine, StateError};
