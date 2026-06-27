use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Parse error: {0}")]
    ParseError(String),
    
    #[error("Validation error: {0}")]
    ValidationError(String),
    
    #[error("Signal not found")]
    SignalNotFound,
    
    #[error("State not found: signal={signal}, state={state}")]
    StateNotFound { signal: String, state: String },
    
    #[error("Transition not found: signal={signal}, event={event}")]
    TransitionNotFound { signal: String, event: String },
    
    #[error("Action not found")]
    ActionNotFound,
    
    #[error("Action execution error: {0}")]
    ActionExecutionError(String),
}