use crate::engine::TopologyEngine;
use crate::error::EngineError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A point-in-time snapshot of every signal's current state.
///
/// `states` maps each signal id to its current state. This is the format
/// written by `save_state` and read by `load_state`, and is also what `stp`
/// persists to disk. It is serde-serializable, so a snapshot can round-trip
/// through JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Maps each signal id to its current state.
    pub states: HashMap<String, String>,
}

impl TopologyEngine {
    /// Snapshot the engine's current state to a JSON file at `path`.
    ///
    /// The written file is a `StateSnapshot` and can be restored with
    /// `load_state`. Returns `EngineError::PersistenceError` on a write
    /// failure.
    pub fn save_state(&self, path: &Path) -> Result<(), EngineError> {
        let mut states = HashMap::new();
        for (id, signal) in &self.signals {
            states.insert(id.clone(), signal.current.clone());
        }
        let snapshot = StateSnapshot { states };
        let json = serde_json::to_string_pretty(&snapshot)
            .map_err(|e| EngineError::PersistenceError(e.to_string()))?;
        fs::write(path, json).map_err(|e| EngineError::PersistenceError(e.to_string()))?;
        Ok(())
    }

    /// Restore the engine's state from a JSON snapshot at `path`.
    ///
    /// Every `(signal, state)` in the snapshot is validated against the
    /// engine's current topology: unknown signals and states not in the
    /// signal's `states` list are rejected with an error and the engine is left
    /// unchanged. Returns `EngineError::PersistenceError` on a read/parse
    /// failure.
    pub fn load_state(&mut self, path: &Path) -> Result<(), EngineError> {
        let json = fs::read_to_string(path)
            .map_err(|e| EngineError::PersistenceError(e.to_string()))?;
        let snapshot: StateSnapshot = serde_json::from_str(&json)
            .map_err(|e| EngineError::PersistenceError(e.to_string()))?;

        for (signal_id, state) in &snapshot.states {
            let signal = self
                .signals
                .get(signal_id)
                .ok_or_else(|| EngineError::PersistenceError(format!("Unknown signal: {signal_id}")))?;
            if !signal.states.contains(state) {
                return Err(EngineError::StateNotFound {
                    signal: signal_id.clone(),
                    state: state.clone(),
                });
            }
        }

        for (signal_id, state) in snapshot.states {
            if let Some(signal) = self.signals.get_mut(&signal_id) {
                signal.current = state;
            }
        }

        Ok(())
    }
}
