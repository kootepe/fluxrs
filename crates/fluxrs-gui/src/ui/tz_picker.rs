use chrono_tz::{Tz, TZ_VARIANTS};
use egui::{ComboBox, Id, Key, Modifiers, ScrollArea, TextEdit, Ui};

#[derive(Default, Clone)]
pub struct TimezonePickerState {
    pub query: String,
    pub selected: Option<Tz>,
    pub highlight: usize,        // which result is highlighted
    pub focus_search_once: bool, // optional autofocus helper
}

pub fn timezone_combo(
    ui: &mut Ui,
    id: impl std::hash::Hash, // pass a STABLE id (e.g., a string literal)
    state: &mut TimezonePickerState,
) -> Option<Tz> {
    let mut newly_picked: Option<Tz> = None;

    let cb_id = Id::new(("tz_combo", id));
    let search_id = ui.make_persistent_id((cb_id, "search"));

    let inner = ComboBox::from_id_source(cb_id)
        .selected_text(
            state
                .selected
                .map(|tz| tz.to_string())
                .unwrap_or_else(|| "Select timezone…".to_owned()),
        )
        .show_ui(ui, |ui| {
            let search_resp = ui.add(
                TextEdit::singleline(&mut state.query)
                    .hint_text("Search timezones…")
                    .id(search_id)
                    .lock_focus(true),
            );
            let text_changed = search_resp.changed();

            // Build filtered list
            let q = state.query.trim().to_lowercase();
            let mut matches: Vec<Tz> = TZ_VARIANTS
                .iter()
                .copied()
                .filter(|tz| {
                    if q.is_empty() {
                        return true;
                    }
                    let n = tz.to_string().to_lowercase();
                    n.contains(&q)
                        || n.replace('_', " ").contains(&q)
                        || n.contains(&q.replace(' ', "_"))
                })
                .collect();

            // Reset / clamp highlight when typing or list size changes
            if text_changed {
                state.highlight = 0;
            }
            if state.highlight >= matches.len() {
                state.highlight = matches.len().saturating_sub(1);
            }

            // --- Keyboard nav: Tab / Shift+Tab / ArrowUp / ArrowDown / Enter ---
            // Consume Tab so focus doesn't leave the popup
            let pressed_tab = ui.input(|i| i.key_pressed(Key::Tab));
            if pressed_tab {
                ui.input_mut(|i| {
                    // consume both with/without Shift
                    i.consume_key(Modifiers::NONE, Key::Tab);
                    i.consume_key(Modifiers::SHIFT, Key::Tab);
                });
            }
            let shift_down = ui.input(|i| i.modifiers.shift);
            let pressed_down = ui.input(|i| i.key_pressed(Key::ArrowDown));
            let pressed_up = ui.input(|i| i.key_pressed(Key::ArrowUp));

            if !matches.is_empty() && (pressed_tab || pressed_down || pressed_up) {
                let len = matches.len();
                let mut idx = state.highlight as isize;

                if pressed_down || (pressed_tab && !shift_down) {
                    idx = (idx + 1).rem_euclid(len as isize);
                } else if pressed_up || (pressed_tab && shift_down) {
                    idx = (idx - 1).rem_euclid(len as isize);
                }
                state.highlight = idx as usize;
            }

            // Results list
            ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                if matches.is_empty() {
                    ui.label("No matches");
                } else {
                    for (i, tz) in matches.iter().enumerate() {
                        let label = tz.to_string();
                        let is_sel = state.selected == Some(*tz);
                        // Visually highlight the keyboard-selected row by marking it selected too
                        let row_selected = is_sel || i == state.highlight;

                        let resp = ui.selectable_label(row_selected, label);
                        if i == state.highlight {
                            // Keep highlighted row in view while navigating
                            ui.scroll_to_rect(resp.rect, Some(egui::Align::Center));
                        }
                        if resp.clicked() {
                            state.selected = Some(*tz);
                            state.query = tz.to_string();
                            newly_picked = Some(*tz);
                        }
                    }
                }
            });
            // Pick with Enter
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                if let Some(tz) = matches.get(state.highlight).copied() {
                    state.selected = Some(tz);
                    state.query = tz.to_string();
                    newly_picked = Some(tz);
                    ui.memory_mut(|m| m.close_popup());
                }
            }

            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    state.query.clear();
                    state.highlight = 0;
                }
                if ui.button("Reset selection").clicked() {
                    state.selected = None;
                    state.query.clear();
                    state.highlight = 0;
                    newly_picked = None;
                }
            });

            // Optional one-shot autofocus
            if state.focus_search_once {
                ui.memory_mut(|m| m.request_focus(search_id));
                state.focus_search_once = false;
            }
        });

    // Autofocus when the header is clicked to open the popup
    if inner.response.clicked() {
        state.focus_search_once = true;
    }

    newly_picked
}
