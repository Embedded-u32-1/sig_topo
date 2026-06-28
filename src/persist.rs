use crate::engine::TopologyEngine;
use crate::error::EngineError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub states: HashMap<String, String>,
}

impl TopologyEngine {
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
