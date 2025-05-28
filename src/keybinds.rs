use egui::Key;
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::fmt;
use std::fs;

#[derive(Hash, Eq, PartialEq, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Action {
    ToggleDebug,
    NextCycle,
    PreviousCycle,
    ZoomToMeasurement,
    ResetCycle,
    SearchLag,
    IncrementLag,
    DecrementLag,
    ToggleValidity,
    ToggleBad,
    ToggleShowValids,
    ToggleShowInvalids,
    ToggleShowBad,
    ToggleShowSettings,
    ToggleShowLinear,
    ToggleShowRobLinear,
    ToggleShowPoly,
    ToggleShowResiduals,
    ToggleShowStandResiduals,
    ToggleShowDetails,
    ToggleShowLegend,
    TogglePlotWidthsWindow,
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Action::ToggleDebug => write!(f, "Toggle Debug"),
            Action::NextCycle => write!(f, "Next Cycle"),
            Action::PreviousCycle => write!(f, "Previous Cycle"),
            Action::ZoomToMeasurement => write!(f, "Zoom To Measurement"),
            Action::ResetCycle => write!(f, "Reset Cycle"),
            Action::SearchLag => write!(f, "Search Lag"),
            Action::IncrementLag => write!(f, "Increment Lag"),
            Action::DecrementLag => write!(f, "Decrement Lag"),
            Action::ToggleValidity => write!(f, "Toggle validity"),
            Action::ToggleBad => write!(f, "Toggle bad measurement"),
            Action::ToggleShowValids => write!(f, "Toggle Hide Valids"),
            Action::ToggleShowInvalids => write!(f, "Toggle Hide Invalids"),
            Action::ToggleShowBad => write!(f, "Toggle Show Bad"),
            Action::ToggleShowSettings => write!(f, "Toggle settings panel"),
            Action::ToggleShowLinear => write!(f, "Toggle Linear model"),
            Action::ToggleShowRobLinear => write!(f, "Toggle robust linear model"),
            Action::ToggleShowPoly => write!(f, "Toggle Poly model"),
            Action::ToggleShowStandResiduals => write!(f, "Toggle standardized residuals plots"),
            Action::ToggleShowResiduals => {
                write!(f, "Toggle residuals bar plots")
            },
            Action::ToggleShowDetails => write!(f, "Toggle cycle details window"),
            Action::ToggleShowLegend => write!(f, "Toggle legend window"),
            Action::TogglePlotWidthsWindow => write!(f, "Toggle plot size adjustment window"),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyBindings {
    bindings: HashMap<Action, Key>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        bindings.insert(Action::NextCycle, Key::ArrowRight);
        bindings.insert(Action::PreviousCycle, Key::ArrowLeft);
        bindings.insert(Action::ZoomToMeasurement, Key::Z);
        bindings.insert(Action::ResetCycle, Key::R);
        bindings.insert(Action::SearchLag, Key::S);
        bindings.insert(Action::IncrementLag, Key::ArrowUp);
        bindings.insert(Action::DecrementLag, Key::ArrowDown);
        bindings.insert(Action::ToggleValidity, Key::I);
        bindings.insert(Action::ToggleBad, Key::B);
        bindings.insert(Action::ToggleShowValids, Key::Q);
        bindings.insert(Action::ToggleShowInvalids, Key::W);
        bindings.insert(Action::ToggleShowBad, Key::E);
        bindings.insert(Action::ToggleShowSettings, Key::F1);
        bindings.insert(Action::ToggleShowLegend, Key::F2);
        bindings.insert(Action::ToggleShowDetails, Key::F3);
        Self { bindings }
    }
}
impl KeyBindings {
    pub fn set(&mut self, action: Action, new_key: Key) {
        self.bindings.retain(|_, &mut k| k != new_key);
        self.bindings.insert(action, new_key);
    }
    pub fn remove(&mut self, action: &Action) {
        self.bindings.remove(action);
    }
    pub fn key_for(&self, action: Action) -> Option<Key> {
        self.bindings.get(&action).copied()
    }

    pub fn action_triggered(&self, action: Action, input: &egui::InputState) -> bool {
        if let Some(&key) = self.bindings.get(&action) {
            input.key_pressed(key)
        } else {
            false
        }
    }
    pub fn save_to_file(&self, path: &str) -> std::io::Result<()> {
        let data = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, data)
    }

    pub fn load_from_file(path: &str) -> std::io::Result<Self> {
        let content = fs::read_to_string(path)?;
        let parsed: Self = serde_json::from_str(&content).unwrap();
        Ok(parsed)
    }

    pub fn to_runtime(&self) -> HashMap<Action, egui::Key> {
        self.bindings.iter().map(|(a, k)| (*a, (*k).into())).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBind {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
