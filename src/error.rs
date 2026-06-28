use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Signal not found: {0}")]
    SignalNotFound(String),

    #[error("State not found: signal={signal}, state={state}")]
    StateNotFound { signal: String, state: String },

    #[error("Transition not found: signal={signal}, event={event}")]
    TransitionNotFound { signal: String, event: String },

    #[error("Action not found: {0}")]
    ActionNotFound(String),

    #[error("Action execution error: {0}")]
    ActionExecutionError(String),

    #[error("Guard evaluation error: {0}")]
    GuardEvaluationError(String),

    #[error("Transition blocked by guard '{guard}' for signal '{signal}' on event '{event}'")]
    GuardBlocked { signal: String, event: String, guard: String },

    #[error("Reload error: {0}")]
    ReloadError(String),

    #[error("Persistence error: {0}")]
    PersistenceError(String),
}
