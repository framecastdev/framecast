//! Domain entities for Projects domain
//!
//! This module contains project-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use framecast_common::{Error, Result, Urn};

use crate::domain::state::{ProjectEvent, ProjectState, ProjectStateMachine, StateError};

/// Project status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "project_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    #[default]
    Draft,
    Rendering,
    Completed,
    Archived,
}

impl ProjectStatus {
    /// Check if this is a terminal state (Project has no terminal states)
    #[mutants::skip] // Delegates to ProjectState::is_terminal() which always returns false
    pub fn is_terminal(&self) -> bool {
        self.to_state().is_terminal()
    }

    /// Convert to state machine state
    pub fn to_state(&self) -> ProjectState {
        match self {
            ProjectStatus::Draft => ProjectState::Draft,
            ProjectStatus::Rendering => ProjectState::Rendering,
            ProjectStatus::Completed => ProjectState::Completed,
            ProjectStatus::Archived => ProjectState::Archived,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: ProjectState) -> Self {
        match state {
            ProjectState::Draft => ProjectStatus::Draft,
            ProjectState::Rendering => ProjectStatus::Rendering,
            ProjectState::Completed => ProjectStatus::Completed,
            ProjectState::Archived => ProjectStatus::Archived,
        }
    }

    /// Get valid next states from current state
    pub fn valid_transitions(&self) -> Vec<ProjectStatus> {
        self.to_state()
            .valid_transitions()
            .iter()
            .map(|s| ProjectStatus::from_state(*s))
            .collect()
    }
}

/// Project entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Project {
    pub id: Uuid,
    pub team_id: Uuid,
    pub created_by: Uuid,
    pub name: String,
    pub status: ProjectStatus,
    pub spec: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    /// Create a new project with validation
    pub fn new(
        team_id: Uuid,
        created_by: Uuid,
        name: String,
        spec: serde_json::Value,
    ) -> Result<Self> {
        if name.len() > 200 {
            return Err(Error::Validation(
                "Project name must be ≤200 characters".to_string(),
            ));
        }

        let now = Utc::now();
        Ok(Project {
            id: Uuid::new_v4(),
            team_id,
            created_by,
            name,
            status: ProjectStatus::default(),
            spec: Json(spec),
            created_at: now,
            updated_at: now,
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        if self.name.len() > 200 {
            return Err(Error::Validation(
                "Project name must be ≤200 characters".to_string(),
            ));
        }
        Ok(())
    }

    /// Start rendering the project
    pub fn start_render(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Render)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as completed (called when job completes)
    pub fn on_job_completed(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobCompleted)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as draft (called when job fails)
    pub fn on_job_failed(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobFailed)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark project as draft (called when job is canceled)
    pub fn on_job_canceled(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::JobCanceled)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Archive the project
    pub fn archive(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Archive)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Unarchive the project
    pub fn unarchive(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ProjectEvent::Unarchive)?;
        self.status = ProjectStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: ProjectEvent) -> Result<ProjectState> {
        let current_state = self.status.to_state();
        ProjectStateMachine::transition(current_state, event).map_err(|e| match e {
            StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                "Invalid project transition: cannot apply '{}' event from '{}' state",
                event, from
            )),
            StateError::TerminalState(state) => Error::Validation(format!(
                "Project is in terminal state '{}' and cannot transition",
                state
            )),
            StateError::GuardFailed(msg) => Error::Validation(msg),
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &ProjectEvent) -> bool {
        ProjectStateMachine::can_transition(self.status.to_state(), event)
    }
}

/// Asset file status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "asset_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssetStatus {
    #[default]
    Pending,
    Ready,
    Failed,
}

/// Asset file entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AssetFile {
    pub id: Uuid,
    pub owner: String, // URN as string
    pub uploaded_by: Uuid,
    pub project_id: Option<Uuid>,
    pub filename: String,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: AssetStatus,
    pub metadata: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AssetFile {
    /// Allowed content types per spec
    pub const ALLOWED_CONTENT_TYPES: &'static [&'static str] = &[
        "image/jpeg",
        "image/png",
        "image/webp",
        "audio/mpeg",
        "audio/wav",
        "audio/ogg",
        "video/mp4",
    ];

    /// Maximum file size (50MB)
    pub const MAX_SIZE_BYTES: i64 = 50 * 1024 * 1024;

    /// Create a new asset file with validation
    pub fn new(
        owner: Urn,
        uploaded_by: Uuid,
        project_id: Option<Uuid>,
        filename: String,
        s3_key: String,
        content_type: String,
        size_bytes: i64,
    ) -> Result<Self> {
        // Validate filename
        if filename.is_empty() || filename.len() > 255 {
            return Err(Error::Validation(
                "Filename must be 1-255 characters".to_string(),
            ));
        }

        // Validate content type
        if !Self::ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
            return Err(Error::Validation(format!(
                "Content type '{}' not allowed",
                content_type
            )));
        }

        // Validate size
        if size_bytes <= 0 {
            return Err(Error::Validation("File size must be positive".to_string()));
        }

        if size_bytes > Self::MAX_SIZE_BYTES {
            return Err(Error::Validation(format!(
                "File size exceeds maximum of {} bytes",
                Self::MAX_SIZE_BYTES
            )));
        }

        let now = Utc::now();
        Ok(AssetFile {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            uploaded_by,
            project_id,
            filename,
            s3_key,
            content_type,
            size_bytes,
            status: AssetStatus::default(),
            metadata: Json(serde_json::Value::Object(serde_json::Map::new())),
            created_at: now,
            updated_at: now,
        })
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // Filename validation
        if self.filename.is_empty() || self.filename.len() > 255 {
            return Err(Error::Validation(
                "Filename must be 1-255 characters".to_string(),
            ));
        }

        // Content type validation
        if !Self::ALLOWED_CONTENT_TYPES.contains(&self.content_type.as_str()) {
            return Err(Error::Validation(format!(
                "Content type '{}' not allowed",
                self.content_type
            )));
        }

        // Size validation
        if self.size_bytes <= 0 {
            return Err(Error::Validation("File size must be positive".to_string()));
        }

        if self.size_bytes > Self::MAX_SIZE_BYTES {
            return Err(Error::Validation(format!(
                "File size exceeds maximum of {} bytes",
                Self::MAX_SIZE_BYTES
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_project_creation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let name = "Test Project".to_string();
        let spec = json!({"type": "storyboard", "scenes": []});

        let project = Project::new(team_id, created_by, name.clone(), spec.clone()).unwrap();

        assert_eq!(project.team_id, team_id);
        assert_eq!(project.created_by, created_by);
        assert_eq!(project.name, name);
        assert_eq!(project.status, ProjectStatus::Draft);
        assert_eq!(project.spec.0, spec);
    }

    #[test]
    fn test_project_name_validation() {
        let team_id = Uuid::new_v4();
        let created_by = Uuid::new_v4();
        let spec = json!({});

        // Test name too long
        let result = Project::new(team_id, created_by, "a".repeat(201), spec);
        assert!(result.is_err());
    }

    #[test]
    fn test_asset_file_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let uploaded_by = Uuid::new_v4();
        let filename = "test.jpg".to_string();
        let s3_key = "uploads/test.jpg".to_string();
        let content_type = "image/jpeg".to_string();
        let size_bytes = 1024;

        let asset = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            filename.clone(),
            s3_key.clone(),
            content_type.clone(),
            size_bytes,
        )
        .unwrap();

        assert_eq!(asset.owner_urn().unwrap(), owner);
        assert_eq!(asset.uploaded_by, uploaded_by);
        assert_eq!(asset.filename, filename);
        assert_eq!(asset.s3_key, s3_key);
        assert_eq!(asset.content_type, content_type);
        assert_eq!(asset.size_bytes, size_bytes);
        assert_eq!(asset.status, AssetStatus::Pending);
    }

    #[test]
    fn test_asset_file_validation() {
        let owner = Urn::user(Uuid::new_v4());
        let uploaded_by = Uuid::new_v4();

        // Test invalid content type
        let result = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            "test.txt".to_string(),
            "uploads/test.txt".to_string(),
            "text/plain".to_string(), // Not allowed
            1024,
        );
        assert!(result.is_err());

        // Test file too large
        let result = AssetFile::new(
            owner.clone(),
            uploaded_by,
            None,
            "large.jpg".to_string(),
            "uploads/large.jpg".to_string(),
            "image/jpeg".to_string(),
            AssetFile::MAX_SIZE_BYTES + 1,
        );
        assert!(result.is_err());

        // Test valid file
        let asset = AssetFile::new(
            owner,
            uploaded_by,
            None,
            "test.jpg".to_string(),
            "uploads/test.jpg".to_string(),
            "image/jpeg".to_string(),
            1024,
        )
        .unwrap();
        assert!(asset.validate().is_ok());
    }
}
