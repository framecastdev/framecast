//! Domain entities for Artifacts domain
//!
//! This module contains artifact-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use uuid::Uuid;

use framecast_common::{Error, Result, Urn};

use crate::domain::state::{ArtifactEvent, ArtifactState, ArtifactStateMachine, StateError};

/// Maximum file size (50MB)
pub const MAX_SIZE_BYTES: i64 = 52_428_800;

/// Artifact kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "artifact_kind", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Storyboard,
    Image,
    Audio,
    Video,
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactKind::Storyboard => write!(f, "storyboard"),
            ArtifactKind::Image => write!(f, "image"),
            ArtifactKind::Audio => write!(f, "audio"),
            ArtifactKind::Video => write!(f, "video"),
        }
    }
}

impl ArtifactKind {
    /// Whether this kind is a media type (requires file metadata)
    pub fn is_media(&self) -> bool {
        matches!(self, Self::Image | Self::Audio | Self::Video)
    }

    /// Allowed content types for this kind
    pub fn allowed_content_types(&self) -> &'static [&'static str] {
        match self {
            ArtifactKind::Image => &["image/jpeg", "image/png", "image/webp"],
            ArtifactKind::Audio => &["audio/mpeg", "audio/wav", "audio/ogg"],
            ArtifactKind::Video => &["video/mp4"],
            ArtifactKind::Storyboard => &[],
        }
    }
}

/// Artifact status — reuses existing asset_status enum from DB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "asset_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ArtifactStatus {
    #[default]
    Pending,
    Ready,
    Failed,
}

impl ArtifactStatus {
    /// Convert to state machine state
    pub fn to_state(&self) -> ArtifactState {
        match self {
            ArtifactStatus::Pending => ArtifactState::Pending,
            ArtifactStatus::Ready => ArtifactState::Ready,
            ArtifactStatus::Failed => ArtifactState::Failed,
        }
    }

    /// Create from state machine state
    pub fn from_state(state: ArtifactState) -> Self {
        match state {
            ArtifactState::Pending => ArtifactStatus::Pending,
            ArtifactState::Ready => ArtifactStatus::Ready,
            ArtifactState::Failed => ArtifactStatus::Failed,
        }
    }
}

impl std::fmt::Display for ArtifactStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactStatus::Pending => write!(f, "pending"),
            ArtifactStatus::Ready => write!(f, "ready"),
            ArtifactStatus::Failed => write!(f, "failed"),
        }
    }
}

/// Artifact source — how the artifact was created
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type, Default)]
#[sqlx(type_name = "artifact_source", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum ArtifactSource {
    #[default]
    Upload,
    Conversation,
    Job,
}

impl std::fmt::Display for ArtifactSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactSource::Upload => write!(f, "upload"),
            ArtifactSource::Conversation => write!(f, "conversation"),
            ArtifactSource::Job => write!(f, "job"),
        }
    }
}

/// Artifact entity — a creative output (storyboard spec, uploaded media, or job output)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Artifact {
    pub id: Uuid,
    pub owner: String,
    pub created_by: Uuid,
    pub project_id: Option<Uuid>,
    pub kind: ArtifactKind,
    pub status: ArtifactStatus,
    pub source: ArtifactSource,
    pub filename: Option<String>,
    pub s3_key: Option<String>,
    pub content_type: Option<String>,
    pub size_bytes: Option<i64>,
    pub spec: Option<serde_json::Value>,
    pub conversation_id: Option<Uuid>,
    pub source_job_id: Option<Uuid>,
    pub metadata: Json<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Artifact {
    /// Create a new storyboard artifact
    pub fn new_storyboard(
        owner: Urn,
        created_by: Uuid,
        project_id: Option<Uuid>,
        spec: serde_json::Value,
    ) -> Result<Self> {
        let artifact = Self {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            created_by,
            project_id,
            kind: ArtifactKind::Storyboard,
            status: ArtifactStatus::default(),
            source: ArtifactSource::Upload,
            filename: None,
            s3_key: None,
            content_type: None,
            size_bytes: None,
            spec: Some(spec),
            conversation_id: None,
            source_job_id: None,
            metadata: Json(serde_json::Value::Object(serde_json::Map::new())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Create a new media artifact (image, audio, video)
    #[allow(clippy::too_many_arguments)]
    pub fn new_media(
        owner: Urn,
        created_by: Uuid,
        project_id: Option<Uuid>,
        kind: ArtifactKind,
        filename: String,
        s3_key: String,
        content_type: String,
        size_bytes: i64,
    ) -> Result<Self> {
        if !kind.is_media() {
            return Err(Error::Validation(
                "Use new_storyboard() for storyboard artifacts".to_string(),
            ));
        }

        let artifact = Self {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            created_by,
            project_id,
            kind,
            status: ArtifactStatus::default(),
            source: ArtifactSource::Upload,
            filename: Some(filename),
            s3_key: Some(s3_key),
            content_type: Some(content_type),
            size_bytes: Some(size_bytes),
            spec: None,
            conversation_id: None,
            source_job_id: None,
            metadata: Json(serde_json::Value::Object(serde_json::Map::new())),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        artifact.validate()?;
        Ok(artifact)
    }

    /// Get owner URN
    pub fn owner_urn(&self) -> Result<Urn> {
        self.owner.parse()
    }

    /// Mark artifact as ready
    pub fn mark_ready(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ArtifactEvent::Complete)?;
        self.status = ArtifactStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark artifact as failed
    pub fn mark_failed(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ArtifactEvent::Fail)?;
        self.status = ArtifactStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Retry a failed artifact (back to pending)
    pub fn retry(&mut self) -> Result<()> {
        let new_state = self.apply_transition(ArtifactEvent::Retry)?;
        self.status = ArtifactStatus::from_state(new_state);
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: ArtifactEvent) -> Result<ArtifactState> {
        let current_state = self.status.to_state();
        ArtifactStateMachine::transition(current_state, event).map_err(|e| match e {
            StateError::InvalidTransition { from, event, .. } => Error::Validation(format!(
                "Invalid artifact transition: cannot apply '{}' event from '{}' state",
                event, from
            )),
            StateError::TerminalState(state) => Error::Validation(format!(
                "Artifact is in terminal state '{}' and cannot transition",
                state
            )),
            StateError::GuardFailed(msg) => Error::Validation(msg),
        })
    }

    /// Check if a transition is valid without applying it
    pub fn can_transition(&self, event: &ArtifactEvent) -> bool {
        ArtifactStateMachine::can_transition(self.status.to_state(), event)
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        // INV-ART1: Media artifacts require file metadata
        if self.kind.is_media() {
            if self.filename.is_none() {
                return Err(Error::Validation(
                    "Media artifacts require a filename".to_string(),
                ));
            }
            if self.s3_key.is_none() {
                return Err(Error::Validation(
                    "Media artifacts require an s3_key".to_string(),
                ));
            }
            if self.content_type.is_none() {
                return Err(Error::Validation(
                    "Media artifacts require a content_type".to_string(),
                ));
            }
            if self.size_bytes.is_none() {
                return Err(Error::Validation(
                    "Media artifacts require size_bytes".to_string(),
                ));
            }
        }

        // INV-ART2: Storyboard artifacts require spec
        if self.kind == ArtifactKind::Storyboard && self.spec.is_none() {
            return Err(Error::Validation(
                "Storyboard artifacts require a spec".to_string(),
            ));
        }

        // INV-ART4: size_bytes constraints
        if let Some(size) = self.size_bytes {
            if size <= 0 {
                return Err(Error::Validation("File size must be positive".to_string()));
            }
            if size > MAX_SIZE_BYTES {
                return Err(Error::Validation(format!(
                    "File size exceeds maximum of {} bytes",
                    MAX_SIZE_BYTES
                )));
            }
        }

        // INV-ART5: content_type must match kind
        if let Some(ref ct) = self.content_type {
            let allowed = self.kind.allowed_content_types();
            if !allowed.is_empty() && !allowed.contains(&ct.as_str()) {
                return Err(Error::Validation(format!(
                    "Content type '{}' not allowed for artifact kind '{}'",
                    ct, self.kind
                )));
            }
        }

        // Filename length
        if let Some(ref filename) = self.filename {
            if filename.is_empty() || filename.len() > 255 {
                return Err(Error::Validation(
                    "Filename must be 1-255 characters".to_string(),
                ));
            }
        }

        // INV-ART7: Project-scoped artifacts must be team-owned
        if self.project_id.is_some() && !self.owner.starts_with("framecast:team:") {
            return Err(Error::Validation(
                "Project-scoped artifacts must be team-owned".to_string(),
            ));
        }

        // INV-ART5: source=conversation requires conversation_id
        if self.source == ArtifactSource::Conversation && self.conversation_id.is_none() {
            return Err(Error::Validation(
                "Conversation-sourced artifacts require a conversation_id".to_string(),
            ));
        }

        // INV-ART6: source=job requires source_job_id
        if self.source == ArtifactSource::Job && self.source_job_id.is_none() {
            return Err(Error::Validation(
                "Job-sourced artifacts require a source_job_id".to_string(),
            ));
        }

        Ok(())
    }
}

/// System asset category
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "system_asset_category", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum SystemAssetCategory {
    Sfx,
    Ambient,
    Music,
    Transition,
}

/// System asset entity for pre-loaded assets
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SystemAsset {
    pub id: String,
    pub category: SystemAssetCategory,
    pub name: String,
    pub description: String,
    pub duration_seconds: Option<rust_decimal::Decimal>,
    pub s3_key: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
}

impl SystemAsset {
    /// Create new system asset with validation
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        category: SystemAssetCategory,
        name: String,
        description: String,
        s3_key: String,
        content_type: String,
        size_bytes: i64,
        duration_seconds: Option<rust_decimal::Decimal>,
        tags: Vec<String>,
    ) -> Result<Self> {
        let category_str = match category {
            SystemAssetCategory::Sfx => "sfx",
            SystemAssetCategory::Ambient => "ambient",
            SystemAssetCategory::Music => "music",
            SystemAssetCategory::Transition => "transition",
        };

        let id = format!("asset_{}_{}", category_str, name);

        let id_regex = regex::Regex::new(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$")
            .map_err(|e| Error::Validation(format!("Invalid regex pattern: {}", e)))?;
        if !id_regex.is_match(&id) {
            return Err(Error::Validation(
                "Invalid system asset ID format".to_string(),
            ));
        }

        if description.len() > 500 {
            return Err(Error::Validation(
                "Description must be ≤500 characters".to_string(),
            ));
        }

        Ok(SystemAsset {
            id,
            category,
            name,
            description,
            duration_seconds,
            s3_key,
            content_type,
            size_bytes,
            tags,
            created_at: Utc::now(),
        })
    }

    /// Validate invariants per spec
    pub fn validate(&self) -> Result<()> {
        let id_regex = regex::Regex::new(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$")
            .map_err(|e| Error::Validation(format!("Invalid regex pattern: {}", e)))?;
        if !id_regex.is_match(&self.id) {
            return Err(Error::Validation(
                "Invalid system asset ID format".to_string(),
            ));
        }

        if self.description.len() > 500 {
            return Err(Error::Validation(
                "Description must be ≤500 characters".to_string(),
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========================================================================
    // ArtifactKind tests
    // ========================================================================

    #[test]
    fn test_artifact_kind_is_media() {
        assert!(!ArtifactKind::Storyboard.is_media());
        assert!(ArtifactKind::Image.is_media());
        assert!(ArtifactKind::Audio.is_media());
        assert!(ArtifactKind::Video.is_media());
    }

    #[test]
    fn test_artifact_kind_allowed_content_types() {
        assert!(ArtifactKind::Storyboard.allowed_content_types().is_empty());
        assert!(ArtifactKind::Image
            .allowed_content_types()
            .contains(&"image/jpeg"));
        assert!(ArtifactKind::Audio
            .allowed_content_types()
            .contains(&"audio/mpeg"));
        assert!(ArtifactKind::Video
            .allowed_content_types()
            .contains(&"video/mp4"));
    }

    #[test]
    fn test_artifact_kind_display() {
        assert_eq!(ArtifactKind::Storyboard.to_string(), "storyboard");
        assert_eq!(ArtifactKind::Image.to_string(), "image");
        assert_eq!(ArtifactKind::Audio.to_string(), "audio");
        assert_eq!(ArtifactKind::Video.to_string(), "video");
    }

    // ========================================================================
    // ArtifactStatus tests
    // ========================================================================

    #[test]
    fn test_artifact_status_display() {
        assert_eq!(ArtifactStatus::Pending.to_string(), "pending");
        assert_eq!(ArtifactStatus::Ready.to_string(), "ready");
        assert_eq!(ArtifactStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_artifact_status_default() {
        assert_eq!(ArtifactStatus::default(), ArtifactStatus::Pending);
    }

    #[test]
    fn test_artifact_status_to_state_roundtrip() {
        for status in [
            ArtifactStatus::Pending,
            ArtifactStatus::Ready,
            ArtifactStatus::Failed,
        ] {
            let state = status.to_state();
            let roundtripped = ArtifactStatus::from_state(state);
            assert_eq!(status, roundtripped);
        }
    }

    // ========================================================================
    // ArtifactSource tests
    // ========================================================================

    #[test]
    fn test_artifact_source_display() {
        assert_eq!(ArtifactSource::Upload.to_string(), "upload");
        assert_eq!(ArtifactSource::Conversation.to_string(), "conversation");
        assert_eq!(ArtifactSource::Job.to_string(), "job");
    }

    #[test]
    fn test_artifact_source_default() {
        assert_eq!(ArtifactSource::default(), ArtifactSource::Upload);
    }

    // ========================================================================
    // Artifact — storyboard creation
    // ========================================================================

    #[test]
    fn test_storyboard_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let created_by = Uuid::new_v4();
        let spec = json!({"scenes": [{"title": "Opening"}]});

        let artifact =
            Artifact::new_storyboard(owner.clone(), created_by, None, spec.clone()).unwrap();

        assert_eq!(artifact.owner, owner.to_string());
        assert_eq!(artifact.created_by, created_by);
        assert_eq!(artifact.kind, ArtifactKind::Storyboard);
        assert_eq!(artifact.status, ArtifactStatus::Pending);
        assert_eq!(artifact.source, ArtifactSource::Upload);
        assert_eq!(artifact.spec, Some(spec));
        assert!(artifact.filename.is_none());
        assert!(artifact.s3_key.is_none());
    }

    #[test]
    fn test_storyboard_with_project_requires_team_owner() {
        let owner = Urn::user(Uuid::new_v4());
        let created_by = Uuid::new_v4();
        let project_id = Some(Uuid::new_v4());

        let result = Artifact::new_storyboard(owner, created_by, project_id, json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn test_storyboard_with_project_team_owner_succeeds() {
        let team_id = Uuid::new_v4();
        let owner = Urn::team(team_id);
        let created_by = Uuid::new_v4();
        let project_id = Some(Uuid::new_v4());

        let artifact = Artifact::new_storyboard(owner, created_by, project_id, json!({})).unwrap();
        assert_eq!(artifact.project_id, project_id);
    }

    // ========================================================================
    // Artifact — media creation
    // ========================================================================

    #[test]
    fn test_media_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let created_by = Uuid::new_v4();

        let artifact = Artifact::new_media(
            owner.clone(),
            created_by,
            None,
            ArtifactKind::Image,
            "photo.jpg".to_string(),
            "uploads/photo.jpg".to_string(),
            "image/jpeg".to_string(),
            1024,
        )
        .unwrap();

        assert_eq!(artifact.kind, ArtifactKind::Image);
        assert_eq!(artifact.filename, Some("photo.jpg".to_string()));
        assert_eq!(artifact.s3_key, Some("uploads/photo.jpg".to_string()));
        assert_eq!(artifact.content_type, Some("image/jpeg".to_string()));
        assert_eq!(artifact.size_bytes, Some(1024));
        assert!(artifact.spec.is_none());
    }

    #[test]
    fn test_media_creation_rejects_storyboard_kind() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Storyboard,
            "file.json".to_string(),
            "key".to_string(),
            "application/json".to_string(),
            100,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_invalid_content_type() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "file.txt".to_string(),
            "key".to_string(),
            "text/plain".to_string(),
            100,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_size_too_large() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "big.jpg".to_string(),
            "key".to_string(),
            "image/jpeg".to_string(),
            MAX_SIZE_BYTES + 1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_size_zero() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "empty.jpg".to_string(),
            "key".to_string(),
            "image/jpeg".to_string(),
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_size_negative() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "neg.jpg".to_string(),
            "key".to_string(),
            "image/jpeg".to_string(),
            -1,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_size_boundary_max() {
        let owner = Urn::user(Uuid::new_v4());
        let artifact = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "max.jpg".to_string(),
            "key".to_string(),
            "image/jpeg".to_string(),
            MAX_SIZE_BYTES,
        )
        .unwrap();
        assert_eq!(artifact.size_bytes, Some(MAX_SIZE_BYTES));
    }

    #[test]
    fn test_media_filename_empty() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "".to_string(),
            "key".to_string(),
            "image/jpeg".to_string(),
            100,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_media_filename_too_long() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "a".repeat(256),
            "key".to_string(),
            "image/jpeg".to_string(),
            100,
        );
        assert!(result.is_err());
    }

    // ========================================================================
    // Artifact — state transitions
    // ========================================================================

    #[test]
    fn test_mark_ready() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Pending);

        artifact.mark_ready().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Ready);
    }

    #[test]
    fn test_mark_failed() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();

        artifact.mark_failed().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Failed);
    }

    #[test]
    fn test_retry_from_failed() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();

        artifact.mark_failed().unwrap();
        artifact.retry().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Pending);
    }

    #[test]
    fn test_cannot_retry_from_ready() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();

        artifact.mark_ready().unwrap();
        assert!(artifact.retry().is_err());
    }

    #[test]
    fn test_can_transition() {
        let owner = Urn::user(Uuid::new_v4());
        let artifact = Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();

        assert!(artifact.can_transition(&ArtifactEvent::Complete));
        assert!(artifact.can_transition(&ArtifactEvent::Fail));
        assert!(!artifact.can_transition(&ArtifactEvent::Retry));
    }

    // ========================================================================
    // Artifact — validate
    // ========================================================================

    #[test]
    fn test_validate_source_conversation_requires_conversation_id() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        artifact.source = ArtifactSource::Conversation;
        artifact.conversation_id = None;

        assert!(artifact.validate().is_err());
    }

    #[test]
    fn test_validate_source_conversation_with_id_succeeds() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        artifact.source = ArtifactSource::Conversation;
        artifact.conversation_id = Some(Uuid::new_v4());

        assert!(artifact.validate().is_ok());
    }

    #[test]
    fn test_validate_source_job_requires_source_job_id() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        artifact.source = ArtifactSource::Job;
        artifact.source_job_id = None;

        assert!(artifact.validate().is_err());
    }

    #[test]
    fn test_validate_source_job_with_id_succeeds() {
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        artifact.source = ArtifactSource::Job;
        artifact.source_job_id = Some(Uuid::new_v4());

        assert!(artifact.validate().is_ok());
    }

    #[test]
    fn test_owner_urn() {
        let user_id = Uuid::new_v4();
        let owner = Urn::user(user_id);
        let artifact =
            Artifact::new_storyboard(owner.clone(), Uuid::new_v4(), None, json!({})).unwrap();

        assert_eq!(artifact.owner_urn().unwrap(), owner);
    }

    // ========================================================================
    // SystemAsset tests
    // ========================================================================

    #[test]
    fn test_system_asset_creation() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "whoosh_01".to_string(),
            "Wind whoosh sound effect".to_string(),
            "system/sfx/whoosh_01.wav".to_string(),
            "audio/wav".to_string(),
            2048,
            None,
            vec!["wind".to_string(), "whoosh".to_string()],
        )
        .unwrap();

        assert_eq!(asset.id, "asset_sfx_whoosh_01");
        assert_eq!(asset.name, "whoosh_01");
        assert_eq!(asset.tags, vec!["wind", "whoosh"]);
    }

    #[test]
    fn test_system_asset_invalid_name() {
        let result = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "INVALID_NAME".to_string(),
            "Description".to_string(),
            "key".to_string(),
            "audio/wav".to_string(),
            1024,
            None,
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_system_asset_description_too_long() {
        let result = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "valid_name".to_string(),
            "a".repeat(501),
            "key".to_string(),
            "audio/wav".to_string(),
            1024,
            None,
            vec![],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_system_asset_validate_returns_err_on_invalid() {
        let asset = SystemAsset {
            id: "INVALID_FORMAT".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "desc".to_string(),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: vec![],
            created_at: Utc::now(),
        };
        assert!(asset.validate().is_err());
    }

    #[test]
    fn test_system_asset_validate_id_regex() {
        let valid_asset = SystemAsset {
            id: "asset_sfx_test".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "desc".to_string(),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: vec![],
            created_at: Utc::now(),
        };
        assert!(valid_asset.validate().is_ok());

        let invalid_asset = SystemAsset {
            id: "not-matching-format".to_string(),
            ..valid_asset.clone()
        };
        assert!(invalid_asset.validate().is_err());
    }

    #[test]
    fn test_system_asset_validate_description_len_boundary() {
        let base = SystemAsset {
            id: "asset_sfx_test".to_string(),
            category: SystemAssetCategory::Sfx,
            name: "test".to_string(),
            description: "a".repeat(500),
            duration_seconds: None,
            s3_key: "key".to_string(),
            content_type: "audio/wav".to_string(),
            size_bytes: 1024,
            tags: vec![],
            created_at: Utc::now(),
        };
        assert!(base.validate().is_ok());

        let asset_501 = SystemAsset {
            description: "a".repeat(501),
            ..base.clone()
        };
        assert!(asset_501.validate().is_err());

        let asset_499 = SystemAsset {
            description: "a".repeat(499),
            ..base.clone()
        };
        assert!(asset_499.validate().is_ok());
    }
}
