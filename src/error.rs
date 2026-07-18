use thiserror::Error;

/// The error type for every failure path in the crate.
///
/// Returned by the engine, the compose/expand layer, the DDL compiler and the
/// persistence helpers. Each variant carries enough context (signal, state,
/// action, line/column where applicable) to locate the problem.
#[derive(Debug, Error)]
pub enum EngineError {
    /// JSON (or DDL) input could not be parsed. The inner string holds the
    /// underlying parser message, often with a line/column.
    #[error("Parse error: {0}")]
    ParseError(String),

    /// A topology failed validation (duplicate signal, unknown reference, state
    /// not in the signal's `states` list, ...).
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// An operation referenced a signal id that does not exist.
    #[error("Signal not found: {0}")]
    SignalNotFound(String),

    /// A state is not a member of the signal's `states` list.
    #[error("State not found: signal={signal}, state={state}")]
    StateNotFound { signal: String, state: String },

    /// No transition matched the `(signal, event)` against the signal's current
    /// state (or the wildcard `*`).
    #[error("Transition not found: signal={signal}, event={event}")]
    TransitionNotFound { signal: String, event: String },

    /// A transition referenced an action id that was never registered.
    #[error("Action not found: {0}")]
    ActionNotFound(String),

    /// A registered action callback returned an error. The transition is rolled
    /// back to its source state.
    #[error("Action execution error: {0}")]
    ActionExecutionError(String),

    /// A guard expression could not be evaluated (syntax error, type mismatch,
    /// missing payload field, ...).
    #[error("Guard evaluation error: {0}")]
    GuardEvaluationError(String),

    /// A guard expression evaluated to `false`; the transition is blocked and
    /// the signal's state is unchanged.
    #[error("Transition blocked by guard '{guard}' for signal '{signal}' on event '{event}'")]
    GuardBlocked { signal: String, event: String, guard: String },

    /// `reload_topology` failed to parse or validate the replacement topology.
    #[error("Reload error: {0}")]
    ReloadError(String),

    /// A persistence operation (read/write/parse state snapshot) failed.
    #[error("Persistence error: {0}")]
    PersistenceError(String),

    /// Reaction cascade recursion exceeded `max_cascade_depth`.
    #[error("Cascade depth exceeded")]
    CascadeDepthExceeded,

    /// A reaction references a signal id that does not exist.
    #[error("Reaction references unknown signal: {0}")]
    ReactionSignalNotFound(String),

    /// An instance references a component name that is not declared.
    #[error("Component not found: {0}")]
    ComponentNotFound(String),

    /// An instance did not supply a binding for a declared parameter.
    #[error("Missing binding for param '{param}' in component '{component}'")]
    MissingBinding { component: String, param: String },

    /// After expansion, two signals share the same id (e.g. from two instances
    /// or two included files).
    #[error("Duplicate signal id after expand: {0}")]
    DuplicateSignalAfterExpand(String),

    /// A `${param}` reference in a component is not one of its declared params.
    #[error("Invalid param reference '${param}' in component '{component}' (not in params list)")]
    InvalidParamRef { component: String, param: String },

    /// A connection references a port that the component does not declare.
    #[error("Connection references unknown port '{port}' on component '{component}'")]
    UnknownPort { component: String, port: String },

    /// Two connections on the same instance wire the same port to different
    /// parent signals, which is ambiguous.
    #[error("Port '{port}' on component '{component}' is wired to multiple targets")]
    ConflictingPortConnection { component: String, port: String },

    /// A port references a signal id that the component does not declare.
    #[error("Port '{port}' on component '{component}' references unknown signal '{signal}'")]
    PortUnknownSignal { component: String, port: String, signal: String },

    /// A port references a state that is not a member of its signal's `states`.
    #[error("Port '{port}' on component '{component}' references unknown state '{state}' for signal '{signal}'")]
    PortUnknownState {
        component: String,
        port: String,
        signal: String,
        state: String,
    },

    /// An `includes` entry could not be read or parsed.
    #[error("Include file not found: {0}")]
    IncludeNotFound(String),

    /// A cycle was detected across `includes` (file A includes file B includes
    /// file A).
    #[error("Cyclic include detected: {0}")]
    IncludeCycle(String),
}
