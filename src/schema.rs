use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct TopologySchema {
    pub version: String,
    pub signals: Vec<SignalDef>,
    pub transitions: Vec<TransitionDef>,
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
    #[serde(default)]
    pub components: Option<HashMap<String, ComponentDef>>,
    #[serde(default)]
    pub instances: Vec<InstanceDef>,
    // M17: cross-file import (field added in M16; parsing implemented in M17).
    #[serde(default)]
    pub includes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentDef {
    pub params: Vec<String>,
    pub signals: Vec<SignalDef>,
    pub transitions: Vec<TransitionDef>,
    #[serde(default)]
    pub reactions: Vec<ReactionDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstanceDef {
    pub component: String,
    pub bindings: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalDef {
    pub id: String,
    pub initial_state: String,
    pub states: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TransitionDef {
    pub signal_id: String,
    pub from: String,
    pub event: String,
    pub to: String,
    #[serde(default)]
    pub actions: ActionBinding,
    #[serde(default)]
    pub guard: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ActionBinding {
    #[serde(default)]
    pub on_exit: Vec<String>,
    #[serde(default)]
    pub on_transition: Vec<String>,
    #[serde(default)]
    pub on_enter: Vec<String>,
}

impl ActionBinding {
    pub fn all_actions(&self) -> Vec<&String> {
        let mut actions = Vec::new();
        actions.extend(self.on_exit.iter());
        actions.extend(self.on_transition.iter());
        actions.extend(self.on_enter.iter());
        actions
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReactionDef {
    pub from_signal: String,
    pub from_state: String,
    pub to_signal: String,
    pub event: String,
    pub payload: Option<serde_json::Value>,
}
