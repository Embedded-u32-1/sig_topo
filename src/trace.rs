use std::time::{SystemTime, UNIX_EPOCH};

/// One entry in the ordered trace log (`TraceLog`) produced while running.
///
/// Every `send_event` call appends an `EventReceived`, followed by one
/// `ActionStarted` / `ActionSucceeded` (or `ActionFailed`) per lifecycle action,
/// and a `StateChanged` on success or a `Rollbacked` after a failure.
#[derive(Debug, Clone)]
pub enum TraceEvent {
    /// An event arrived at a signal.
    EventReceived {
        signal_id: String,
        event: String,
        timestamp_ms: u64,
        payload: Option<String>,
    },
    /// A lifecycle action began running.
    ActionStarted {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
    },
    /// A lifecycle action completed successfully.
    ActionSucceeded {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
    },
    /// A lifecycle action failed; `error` is the action's error message. The
    /// transition is rolled back and a `Rollbacked` follows.
    ActionFailed {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
        error: String,
    },
    /// A transition committed: the signal moved from `from` to `to`.
    StateChanged {
        signal_id: String,
        from: String,
        to: String,
        timestamp_ms: u64,
    },
    /// A state transition was rolled back because a lifecycle action failed.
    /// `from` is the target state that was tentatively entered then abandoned;
    /// `to` is the source state the signal was restored to. So the event reads
    /// "rolled back from `from` to `to`". This variant only appears after an
    /// `ActionFailed`; a successful transition emits `StateChanged` instead.
    Rollbacked {
        signal_id: String,
        from: String,
        to: String,
        timestamp_ms: u64,
    },
}

impl TraceEvent {
    /// The signal this event relates to.
    pub fn signal_id(&self) -> &str {
        match self {
            TraceEvent::EventReceived { signal_id, .. } => signal_id,
            TraceEvent::ActionStarted { signal_id, .. } => signal_id,
            TraceEvent::ActionSucceeded { signal_id, .. } => signal_id,
            TraceEvent::ActionFailed { signal_id, .. } => signal_id,
            TraceEvent::StateChanged { signal_id, .. } => signal_id,
            TraceEvent::Rollbacked { signal_id, .. } => signal_id,
        }
    }

    /// The event's monotonic timestamp (milliseconds since the Unix epoch).
    pub fn timestamp_ms(&self) -> u64 {
        match self {
            TraceEvent::EventReceived { timestamp_ms, .. } => *timestamp_ms,
            TraceEvent::ActionStarted { timestamp_ms, .. } => *timestamp_ms,
            TraceEvent::ActionSucceeded { timestamp_ms, .. } => *timestamp_ms,
            TraceEvent::ActionFailed { timestamp_ms, .. } => *timestamp_ms,
            TraceEvent::StateChanged { timestamp_ms, .. } => *timestamp_ms,
            TraceEvent::Rollbacked { timestamp_ms, .. } => *timestamp_ms,
        }
    }
}

/// An append-only log of `TraceEvent`s produced while running the engine.
///
/// The engine holds one `TraceLog` internally; read it via
/// `TopologyEngine::traces` / `traces_for` / `clear_traces`. This type is also
/// used directly by the `stt` / `sts` replay tools.
#[derive(Debug, Clone, Default)]
pub struct TraceLog {
    events: Vec<TraceEvent>,
}

impl TraceLog {
    /// Append an event to the log.
    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    /// Return every event in order.
    pub fn events(&self) -> &[TraceEvent] {
        &self.events
    }

    /// Remove all events.
    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Return the events involving `signal_id`, in order.
    pub fn for_signal(&self, signal_id: &str) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.signal_id() == signal_id)
            .collect()
    }

    /// Return events with `timestamp_ms >= ` the given value, in order.
    pub fn since(&self, timestamp_ms: u64) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp_ms() >= timestamp_ms)
            .collect()
    }
}

/// Milliseconds since the Unix epoch, used to timestamp every trace event.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
