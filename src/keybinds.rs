use egui::Key;
use egui::Key::*;
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
    SearchLagPrevious,
    IncrementLag,
    DecrementLag,
    IncrementDeadband,
    DecrementDeadband,
    IncrementCH4Deadband,
    DecrementCH4Deadband,
    IncrementCO2Deadband,
    DecrementCO2Deadband,
    IncrementH2ODeadband,
    DecrementH2ODeadband,
    IncrementN2ODeadband,
    DecrementN2ODeadband,
    ToggleValidity,
    ToggleCH4Validity,
    ToggleCO2Validity,
    ToggleH2OValidity,
    ToggleN2OValidity,
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
            Action::SearchLagPrevious => write!(f, "Search Lag From Previous"),
            Action::IncrementLag => write!(f, "Increment Lag"),
            Action::DecrementLag => write!(f, "Decrement Lag"),
            Action::IncrementDeadband => write!(f, "Increment Deadband"),
            Action::DecrementDeadband => write!(f, "Decrement Deadband"),
            Action::IncrementCH4Deadband => write!(f, "Increment CH4 Deadband"),
            Action::DecrementCH4Deadband => write!(f, "Decrement CH4 Deadband"),
            Action::IncrementCO2Deadband => write!(f, "Increment CO2 Deadband"),
            Action::DecrementCO2Deadband => write!(f, "Decrement CO2 Deadband"),
            Action::IncrementH2ODeadband => write!(f, "Increment H2O Deadband"),
            Action::DecrementH2ODeadband => write!(f, "Decrement H2O Deadband"),
            Action::IncrementN2ODeadband => write!(f, "Increment N2O Deadband"),
            Action::DecrementN2ODeadband => write!(f, "Decrement N2O Deadband"),
            Action::ToggleValidity => write!(f, "Toggle validity"),
            Action::ToggleCH4Validity => write!(f, "Toggle CH4 validity"),
            Action::ToggleCO2Validity => write!(f, "Toggle CO2 validity"),
            Action::ToggleH2OValidity => write!(f, "Toggle H2O validity"),
            Action::ToggleN2OValidity => write!(f, "Toggle N2O validity"),
            Action::ToggleBad => write!(f, "Toggle bad measurement"),
            Action::ToggleShowValids => write!(f, "Toggle Hide Valids"),
            Action::ToggleShowInvalids => write!(f, "Toggle Hide Invalids"),
            Action::ToggleShowBad => write!(f, "Toggle Show Bad"),
            Action::ToggleShowSettings => write!(f, "Toggle settings panel"),
            Action::ToggleShowLinear => write!(f, "Toggle Linear model"),
            Action::ToggleShowRobLinear => write!(f, "Toggle robust linear model"),
            Action::ToggleShowPoly => write!(f, "Toggle Poly model"),
            Action::ToggleShowStandResiduals => write!(f, "Toggle standardized residuals plots"),
            Action::ToggleShowResiduals => write!(f, "Toggle residuals bar plots"),
            Action::ToggleShowDetails => write!(f, "Toggle cycle details window"),
            Action::ToggleShowLegend => write!(f, "Toggle legend window"),
            Action::TogglePlotWidthsWindow => write!(f, "Toggle plot size adjustment window"),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyBindings {
    bindings: HashMap<Action, KeyBind>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();
        let no_mods = |key| KeyBind { key, ctrl: false, shift: false, alt: false };
        bindings.insert(Action::NextCycle, no_mods(ArrowRight));
        bindings.insert(Action::PreviousCycle, no_mods(ArrowLeft));
        bindings.insert(Action::ZoomToMeasurement, no_mods(Z));
        bindings.insert(Action::ResetCycle, no_mods(R));
        bindings.insert(Action::SearchLagPrevious, no_mods(S));
        bindings.insert(Action::SearchLag, no_mods(L));
        bindings.insert(Action::IncrementLag, no_mods(ArrowUp));
        bindings.insert(Action::DecrementLag, no_mods(ArrowDown));
        bindings.insert(Action::ToggleValidity, no_mods(I));
        bindings.insert(Action::ToggleBad, no_mods(B));
        bindings.insert(Action::ToggleShowValids, no_mods(Q));
        bindings.insert(Action::ToggleShowInvalids, no_mods(W));
        bindings.insert(Action::ToggleShowBad, no_mods(E));
        bindings.insert(Action::ToggleShowSettings, no_mods(F1));
        bindings.insert(Action::ToggleShowLegend, no_mods(F2));
        bindings.insert(Action::ToggleShowDetails, no_mods(F3));
        bindings.insert(Action::TogglePlotWidthsWindow, no_mods(F4));
        bindings
            .insert(Action::SearchLag, KeyBind { key: L, ctrl: true, shift: false, alt: false });
        Self { bindings }
    }
}
impl KeyBindings {
    pub fn set(&mut self, action: Action, new_bind: KeyBind) {
        self.bindings.retain(|_, &mut k| k != new_bind);
        self.bindings.insert(action, new_bind);
    }

    pub fn remove(&mut self, action: &Action) {
        self.bindings.remove(action);
    }
    pub fn key_for(&self, action: Action) -> Option<KeyBind> {
        self.bindings.get(&action).copied()
    }

    pub fn action_triggered(&self, action: Action, input: &egui::InputState) -> bool {
        if let Some(&bind) = self.bindings.get(&action) {
            input.key_pressed(bind.key)
                && input.modifiers.ctrl == bind.ctrl
                && input.modifiers.shift == bind.shift
                && input.modifiers.alt == bind.alt
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

    pub fn to_runtime(&self) -> HashMap<Action, KeyBind> {
        self.bindings.iter().map(|(a, k)| (*a, *k)).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyBind {
    pub key: Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
impl fmt::Display for KeyBind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();

        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }

        parts.push(format!("{:?}", self.key)); // Own the String, no borrowing

        write!(f, "{}", parts.join("+"))
    }
}
