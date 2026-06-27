use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct TopologySchema {
    pub version: String,
    pub signals: Vec<SignalDef>,
    pub transitions: Vec<TransitionDef>,
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