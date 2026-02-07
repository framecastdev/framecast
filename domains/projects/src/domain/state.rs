//! State machines for Projects domain entities
//!
//! This module implements formal state machines as defined in the specification
//! (docs/spec/05_Relationships_States.md). Each state machine defines:
//! - Valid states
//! - Events that trigger transitions
//! - Guard conditions for transitions
//! - Terminal states

use thiserror::Error;

/// Errors that can occur during state transitions
#[derive(Debug, Error, Clone, PartialEq)]
pub enum StateError {
    #[error("Invalid transition: cannot transition from {from} to {to} via {event}")]
    InvalidTransition {
        from: String,
        to: String,
        event: String,
    },

    #[error("Guard condition failed: {0}")]
    GuardFailed(String),

    #[error("Terminal state: {0} is a terminal state and cannot transition")]
    TerminalState(String),
}

// ============================================================================
// Project State Machine
// ============================================================================

/// Project status states as defined in spec section 6.3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectState {
    Draft,
    Rendering,
    Completed,
    Archived,
}

impl ProjectState {
    /// Check if this is a terminal state (Project has no terminal states)
    pub fn is_terminal(&self) -> bool {
        false // Project has no terminal states per spec
    }

    /// Get all valid next states from current state
    pub fn valid_transitions(&self) -> &'static [ProjectState] {
        match self {
            Self::Draft => &[Self::Rendering, Self::Archived],
            Self::Rendering => &[Self::Completed, Self::Draft],
            Self::Completed => &[Self::Archived, Self::Rendering],
            Self::Archived => &[Self::Draft],
        }
    }
}

impl std::fmt::Display for ProjectState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Rendering => write!(f, "rendering"),
            Self::Completed => write!(f, "completed"),
            Self::Archived => write!(f, "archived"),
        }
    }
}

/// Events that trigger project state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum ProjectEvent {
    /// Start rendering the project
    Render,
    /// Associated job completed successfully
    JobCompleted,
    /// Associated job failed
    JobFailed,
    /// Associated job was canceled
    JobCanceled,
    /// Archive the project
    Archive,
    /// Unarchive the project
    Unarchive,
}

impl std::fmt::Display for ProjectEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Render => write!(f, "render"),
            Self::JobCompleted => write!(f, "job_completed"),
            Self::JobFailed => write!(f, "job_failed"),
            Self::JobCanceled => write!(f, "job_canceled"),
            Self::Archive => write!(f, "archive"),
            Self::Unarchive => write!(f, "unarchive"),
        }
    }
}

/// Project state machine
pub struct ProjectStateMachine;

impl ProjectStateMachine {
    /// Attempt a state transition
    pub fn transition(
        current: ProjectState,
        event: ProjectEvent,
    ) -> Result<ProjectState, StateError> {
        let next = match (&current, &event) {
            // From Draft
            (ProjectState::Draft, ProjectEvent::Render) => ProjectState::Rendering,
            (ProjectState::Draft, ProjectEvent::Archive) => ProjectState::Archived,

            // From Rendering
            (ProjectState::Rendering, ProjectEvent::JobCompleted) => ProjectState::Completed,
            (ProjectState::Rendering, ProjectEvent::JobFailed) => ProjectState::Draft,
            (ProjectState::Rendering, ProjectEvent::JobCanceled) => ProjectState::Draft,

            // From Completed
            (ProjectState::Completed, ProjectEvent::Archive) => ProjectState::Archived,
            (ProjectState::Completed, ProjectEvent::Render) => ProjectState::Rendering, // Re-render

            // From Archived
            (ProjectState::Archived, ProjectEvent::Unarchive) => ProjectState::Draft,

            // Invalid transitions
            _ => {
                return Err(StateError::InvalidTransition {
                    from: current.to_string(),
                    to: "unknown".to_string(),
                    event: event.to_string(),
                });
            }
        };

        Ok(next)
    }

    /// Check if a transition is valid without performing it
    pub fn can_transition(current: ProjectState, event: &ProjectEvent) -> bool {
        Self::transition(current, event.clone()).is_ok()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------------
    // Project State Machine Tests
    // ------------------------------------------------------------------------

    mod project_state_machine {
        use super::*;

        #[test]
        fn test_valid_draft_to_rendering() {
            let result = ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::Render);
            assert_eq!(result, Ok(ProjectState::Rendering));
        }

        #[test]
        fn test_valid_draft_to_archived() {
            let result =
                ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::Archive);
            assert_eq!(result, Ok(ProjectState::Archived));
        }

        #[test]
        fn test_valid_rendering_to_completed() {
            let result = ProjectStateMachine::transition(
                ProjectState::Rendering,
                ProjectEvent::JobCompleted,
            );
            assert_eq!(result, Ok(ProjectState::Completed));
        }

        #[test]
        fn test_valid_rendering_to_draft_on_failure() {
            let result =
                ProjectStateMachine::transition(ProjectState::Rendering, ProjectEvent::JobFailed);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_valid_rendering_to_draft_on_cancel() {
            let result =
                ProjectStateMachine::transition(ProjectState::Rendering, ProjectEvent::JobCanceled);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_valid_completed_to_archived() {
            let result =
                ProjectStateMachine::transition(ProjectState::Completed, ProjectEvent::Archive);
            assert_eq!(result, Ok(ProjectState::Archived));
        }

        #[test]
        fn test_valid_completed_to_rendering_rerender() {
            let result =
                ProjectStateMachine::transition(ProjectState::Completed, ProjectEvent::Render);
            assert_eq!(result, Ok(ProjectState::Rendering));
        }

        #[test]
        fn test_valid_archived_to_draft() {
            let result =
                ProjectStateMachine::transition(ProjectState::Archived, ProjectEvent::Unarchive);
            assert_eq!(result, Ok(ProjectState::Draft));
        }

        #[test]
        fn test_invalid_draft_to_completed() {
            let result =
                ProjectStateMachine::transition(ProjectState::Draft, ProjectEvent::JobCompleted);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_invalid_archived_to_completed() {
            let result =
                ProjectStateMachine::transition(ProjectState::Archived, ProjectEvent::JobCompleted);
            assert!(matches!(result, Err(StateError::InvalidTransition { .. })));
        }

        #[test]
        fn test_project_valid_transitions() {
            // Kill mutant: ProjectState::valid_transitions -> Vec::leak(Vec::new())
            let draft = ProjectState::Draft.valid_transitions();
            assert!(!draft.is_empty());
            assert!(draft.contains(&ProjectState::Rendering));
            assert!(draft.contains(&ProjectState::Archived));
            assert_eq!(draft.len(), 2);

            let rendering = ProjectState::Rendering.valid_transitions();
            assert!(!rendering.is_empty());
            assert!(rendering.contains(&ProjectState::Completed));
            assert!(rendering.contains(&ProjectState::Draft));
            assert_eq!(rendering.len(), 2);

            let completed = ProjectState::Completed.valid_transitions();
            assert!(!completed.is_empty());
            assert!(completed.contains(&ProjectState::Archived));
            assert!(completed.contains(&ProjectState::Rendering));
            assert_eq!(completed.len(), 2);

            let archived = ProjectState::Archived.valid_transitions();
            assert!(!archived.is_empty());
            assert!(archived.contains(&ProjectState::Draft));
            assert_eq!(archived.len(), 1);
        }

        #[test]
        fn test_project_can_transition() {
            // Kill mutant: ProjectStateMachine::can_transition -> true / false
            // Valid transitions
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::Render
            ));
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::Archive
            ));
            assert!(ProjectStateMachine::can_transition(
                ProjectState::Completed,
                &ProjectEvent::Archive
            ));

            // Invalid transitions
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Draft,
                &ProjectEvent::JobCompleted
            ));
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Archived,
                &ProjectEvent::Render
            ));
            assert!(!ProjectStateMachine::can_transition(
                ProjectState::Completed,
                &ProjectEvent::Unarchive
            ));
        }

        #[test]
        fn test_project_has_no_terminal_states() {
            assert!(!ProjectState::Draft.is_terminal());
            assert!(!ProjectState::Rendering.is_terminal());
            assert!(!ProjectState::Completed.is_terminal());
            assert!(!ProjectState::Archived.is_terminal());
        }
    }
}
