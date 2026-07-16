use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub enum TraceEvent {
    EventReceived {
        signal_id: String,
        event: String,
        timestamp_ms: u64,
        payload: Option<String>,
    },
    ActionStarted {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
    },
    ActionSucceeded {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
    },
    ActionFailed {
        signal_id: String,
        action_id: String,
        timestamp_ms: u64,
        error: String,
    },
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

#[derive(Debug, Clone, Default)]
pub struct TraceLog {
    events: Vec<TraceEvent>,
}

impl TraceLog {
    pub fn push(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[TraceEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn for_signal(&self, signal_id: &str) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.signal_id() == signal_id)
            .collect()
    }

    pub fn since(&self, timestamp_ms: u64) -> Vec<&TraceEvent> {
        self.events
            .iter()
            .filter(|e| e.timestamp_ms() >= timestamp_ms)
            .collect()
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
