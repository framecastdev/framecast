//! Domain entities for Artifacts domain
//!
//! This module contains artifact-related domain entities as defined in the API specification.
//! Each entity includes proper validation, serialization, and business rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use std::sync::LazyLock;
use uuid::Uuid;

use framecast_common::{Error, Result, Urn};

/// Regex for validating system asset IDs (compiled once)
static SYSTEM_ASSET_ID_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"^asset_(sfx|ambient|music|transition)_[a-z0-9_]+$")
        .expect("system asset ID regex is valid")
});

use crate::domain::state::{ArtifactEvent, ArtifactStateMachine, StateError};

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
    Character,
}

impl std::fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArtifactKind::Storyboard => write!(f, "storyboard"),
            ArtifactKind::Image => write!(f, "image"),
            ArtifactKind::Audio => write!(f, "audio"),
            ArtifactKind::Video => write!(f, "video"),
            ArtifactKind::Character => write!(f, "character"),
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
            ArtifactKind::Storyboard | ArtifactKind::Character => &[],
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
    /// Check if this is a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [ArtifactStatus] {
        match self {
            Self::Pending => &[Self::Ready, Self::Failed],
            Self::Ready => &[],
            Self::Failed => &[Self::Pending],
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
            status: ArtifactStatus::Ready,
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

    /// Create a new character artifact
    #[allow(clippy::too_many_arguments)]
    pub fn new_character(
        owner: Urn,
        created_by: Uuid,
        project_id: Option<Uuid>,
        spec: serde_json::Value,
        source: ArtifactSource,
        conversation_id: Option<Uuid>,
    ) -> Result<Self> {
        let artifact = Self {
            id: Uuid::new_v4(),
            owner: owner.to_string(),
            created_by,
            project_id,
            kind: ArtifactKind::Character,
            status: ArtifactStatus::Ready,
            source,
            filename: None,
            s3_key: None,
            content_type: None,
            size_bytes: None,
            spec: Some(spec),
            conversation_id,
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
                "Use new_storyboard() or new_character() for non-media artifacts".to_string(),
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
        self.status = self.apply_transition(ArtifactEvent::Complete)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Mark artifact as failed
    pub fn mark_failed(&mut self) -> Result<()> {
        self.status = self.apply_transition(ArtifactEvent::Fail)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Retry a failed artifact (back to pending)
    pub fn retry(&mut self) -> Result<()> {
        self.status = self.apply_transition(ArtifactEvent::Retry)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    /// Apply a state transition using the state machine
    fn apply_transition(&self, event: ArtifactEvent) -> Result<ArtifactStatus> {
        ArtifactStateMachine::transition(self.status, event).map_err(|e| match e {
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
        ArtifactStateMachine::can_transition(self.status, event)
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

        // INV-ART2: Storyboard and character artifacts require spec
        if matches!(
            self.kind,
            ArtifactKind::Storyboard | ArtifactKind::Character
        ) && self.spec.is_none()
        {
            return Err(Error::Validation(format!(
                "{} artifacts require a spec",
                self.kind
            )));
        }

        // INV-ART-CHAR: Character spec must contain non-empty "prompt"
        if self.kind == ArtifactKind::Character {
            if let Some(ref spec) = self.spec {
                match spec.get("prompt").and_then(|v| v.as_str()) {
                    Some(prompt) if !prompt.trim().is_empty() => {}
                    _ => {
                        return Err(Error::Validation(
                            "Character artifacts require spec with non-empty \"prompt\""
                                .to_string(),
                        ));
                    }
                }
            }
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

        if !SYSTEM_ASSET_ID_REGEX.is_match(&id) {
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
        if !SYSTEM_ASSET_ID_REGEX.is_match(&self.id) {
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
        assert!(!ArtifactKind::Character.is_media());
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
        assert!(ArtifactKind::Character.allowed_content_types().is_empty());
    }

    #[test]
    fn test_artifact_kind_display() {
        assert_eq!(ArtifactKind::Storyboard.to_string(), "storyboard");
        assert_eq!(ArtifactKind::Image.to_string(), "image");
        assert_eq!(ArtifactKind::Audio.to_string(), "audio");
        assert_eq!(ArtifactKind::Video.to_string(), "video");
        assert_eq!(ArtifactKind::Character.to_string(), "character");
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
    fn test_artifact_status_is_terminal() {
        assert!(!ArtifactStatus::Pending.is_terminal());
        assert!(ArtifactStatus::Ready.is_terminal());
        assert!(!ArtifactStatus::Failed.is_terminal());
    }

    #[test]
    fn test_artifact_status_valid_transitions() {
        let pending = ArtifactStatus::Pending.valid_transitions();
        assert_eq!(pending.len(), 2);
        assert!(pending.contains(&ArtifactStatus::Ready));
        assert!(pending.contains(&ArtifactStatus::Failed));

        assert!(ArtifactStatus::Ready.valid_transitions().is_empty());

        let failed = ArtifactStatus::Failed.valid_transitions();
        assert_eq!(failed.len(), 1);
        assert!(failed.contains(&ArtifactStatus::Pending));
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
        assert_eq!(artifact.status, ArtifactStatus::Ready);
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
    fn test_media_filename_max_length_valid() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_media(
            owner,
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "a".repeat(255),
            "key".to_string(),
            "image/jpeg".to_string(),
            100,
        );
        assert!(result.is_ok());
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
    // Artifact — state transitions (use media artifacts which start Pending)
    // ========================================================================

    /// Helper: create a media artifact in Pending state for transition tests
    fn pending_media_artifact() -> Artifact {
        Artifact::new_media(
            Urn::user(Uuid::new_v4()),
            Uuid::new_v4(),
            None,
            ArtifactKind::Image,
            "test.jpg".to_string(),
            "uploads/test.jpg".to_string(),
            "image/jpeg".to_string(),
            1024,
        )
        .unwrap()
    }

    #[test]
    fn test_mark_ready() {
        let mut artifact = pending_media_artifact();
        assert_eq!(artifact.status, ArtifactStatus::Pending);

        artifact.mark_ready().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Ready);
    }

    #[test]
    fn test_mark_failed() {
        let mut artifact = pending_media_artifact();

        artifact.mark_failed().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Failed);
    }

    #[test]
    fn test_retry_from_failed() {
        let mut artifact = pending_media_artifact();

        artifact.mark_failed().unwrap();
        artifact.retry().unwrap();
        assert_eq!(artifact.status, ArtifactStatus::Pending);
    }

    #[test]
    fn test_cannot_retry_from_ready() {
        let mut artifact = pending_media_artifact();

        artifact.mark_ready().unwrap();
        assert!(artifact.retry().is_err());
    }

    #[test]
    fn test_storyboard_starts_ready_is_terminal() {
        let owner = Urn::user(Uuid::new_v4());
        let artifact = Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();

        assert_eq!(artifact.status, ArtifactStatus::Ready);
        assert!(!artifact.can_transition(&ArtifactEvent::Complete));
        assert!(!artifact.can_transition(&ArtifactEvent::Fail));
        assert!(!artifact.can_transition(&ArtifactEvent::Retry));
    }

    #[test]
    fn test_can_transition() {
        let artifact = pending_media_artifact();

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
    // Artifact — character creation
    // ========================================================================

    #[test]
    fn test_character_creation() {
        let owner = Urn::user(Uuid::new_v4());
        let created_by = Uuid::new_v4();
        let spec = json!({"prompt": "A brave warrior", "name": "Warrior"});

        let artifact = Artifact::new_character(
            owner.clone(),
            created_by,
            None,
            spec.clone(),
            ArtifactSource::Upload,
            None,
        )
        .unwrap();

        assert_eq!(artifact.owner, owner.to_string());
        assert_eq!(artifact.created_by, created_by);
        assert_eq!(artifact.kind, ArtifactKind::Character);
        assert_eq!(artifact.status, ArtifactStatus::Ready);
        assert_eq!(artifact.source, ArtifactSource::Upload);
        assert_eq!(artifact.spec, Some(spec));
        assert!(artifact.filename.is_none());
        assert!(artifact.s3_key.is_none());
    }

    #[test]
    fn test_character_missing_prompt_rejected() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"name": "Test"}),
            ArtifactSource::Upload,
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("prompt"));
    }

    #[test]
    fn test_character_empty_prompt_rejected() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"prompt": ""}),
            ArtifactSource::Upload,
            None,
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("prompt"));
    }

    #[test]
    fn test_character_whitespace_prompt_rejected() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"prompt": "   "}),
            ArtifactSource::Upload,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_character_whitespace_prompt_rejected() {
        // Test validate() directly (not via new_character) to kill mutant on guard condition
        let owner = Urn::user(Uuid::new_v4());
        let mut artifact =
            Artifact::new_storyboard(owner, Uuid::new_v4(), None, json!({})).unwrap();
        artifact.kind = ArtifactKind::Character;
        artifact.spec = Some(json!({"prompt": "   "}));

        assert!(artifact.validate().is_err());
    }

    #[test]
    fn test_character_with_conversation_source() {
        let owner = Urn::user(Uuid::new_v4());
        let conv_id = Uuid::new_v4();
        let artifact = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"prompt": "A hero"}),
            ArtifactSource::Conversation,
            Some(conv_id),
        )
        .unwrap();

        assert_eq!(artifact.source, ArtifactSource::Conversation);
        assert_eq!(artifact.conversation_id, Some(conv_id));
    }

    #[test]
    fn test_character_conversation_source_without_id_rejected() {
        let owner = Urn::user(Uuid::new_v4());
        let result = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"prompt": "A hero"}),
            ArtifactSource::Conversation,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_character_starts_ready_is_terminal() {
        let owner = Urn::user(Uuid::new_v4());
        let artifact = Artifact::new_character(
            owner,
            Uuid::new_v4(),
            None,
            json!({"prompt": "Test"}),
            ArtifactSource::Upload,
            None,
        )
        .unwrap();

        assert_eq!(artifact.status, ArtifactStatus::Ready);
        assert!(!artifact.can_transition(&ArtifactEvent::Complete));
        assert!(!artifact.can_transition(&ArtifactEvent::Fail));
        assert!(!artifact.can_transition(&ArtifactEvent::Retry));
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
    fn test_system_asset_creation_ambient() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Ambient,
            "rain_01".to_string(),
            "Ambient rain sounds".to_string(),
            "system/ambient/rain_01.wav".to_string(),
            "audio/wav".to_string(),
            4096,
            Some(rust_decimal::Decimal::new(300, 1)),
            vec!["rain".to_string(), "nature".to_string()],
        )
        .unwrap();

        assert_eq!(asset.id, "asset_ambient_rain_01");
        assert_eq!(asset.category, SystemAssetCategory::Ambient);
        assert_eq!(
            asset.duration_seconds,
            Some(rust_decimal::Decimal::new(300, 1))
        );
    }

    #[test]
    fn test_system_asset_creation_music() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Music,
            "chill_beat_01".to_string(),
            "Chill beat background music".to_string(),
            "system/music/chill_beat_01.mp3".to_string(),
            "audio/mpeg".to_string(),
            8192,
            Some(rust_decimal::Decimal::new(1200, 1)),
            vec!["chill".to_string(), "lofi".to_string()],
        )
        .unwrap();

        assert_eq!(asset.id, "asset_music_chill_beat_01");
        assert_eq!(asset.category, SystemAssetCategory::Music);
    }

    #[test]
    fn test_system_asset_creation_transition() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Transition,
            "fade_01".to_string(),
            "Smooth fade transition".to_string(),
            "system/transition/fade_01.mp4".to_string(),
            "video/mp4".to_string(),
            16384,
            Some(rust_decimal::Decimal::new(5, 1)),
            vec!["fade".to_string()],
        )
        .unwrap();

        assert_eq!(asset.id, "asset_transition_fade_01");
        assert_eq!(asset.category, SystemAssetCategory::Transition);
    }

    #[test]
    fn test_system_asset_duration_present() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "beep_01".to_string(),
            "Short beep".to_string(),
            "system/sfx/beep_01.wav".to_string(),
            "audio/wav".to_string(),
            512,
            Some(rust_decimal::Decimal::new(15, 1)),
            vec![],
        )
        .unwrap();

        assert_eq!(
            asset.duration_seconds,
            Some(rust_decimal::Decimal::new(15, 1))
        );
    }

    #[test]
    fn test_system_asset_duration_none() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "click_01".to_string(),
            "Click sound".to_string(),
            "system/sfx/click_01.wav".to_string(),
            "audio/wav".to_string(),
            256,
            None,
            vec![],
        )
        .unwrap();

        assert!(asset.duration_seconds.is_none());
    }

    #[test]
    fn test_system_asset_empty_tags_valid() {
        let asset = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "pop_01".to_string(),
            "Pop sound".to_string(),
            "system/sfx/pop_01.wav".to_string(),
            "audio/wav".to_string(),
            128,
            None,
            vec![],
        )
        .unwrap();

        assert!(asset.tags.is_empty());
    }

    #[test]
    fn test_system_asset_tags_preserved() {
        let tags = vec![
            "wind".to_string(),
            "whoosh".to_string(),
            "nature".to_string(),
        ];
        let asset = SystemAsset::new(
            SystemAssetCategory::Sfx,
            "gust_01".to_string(),
            "Wind gust".to_string(),
            "system/sfx/gust_01.wav".to_string(),
            "audio/wav".to_string(),
            1024,
            None,
            tags.clone(),
        )
        .unwrap();

        assert_eq!(asset.tags, tags);
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
