use super::validation_app::AsyncCtx;
use crate::keybinds::Action;
use crate::keybinds::KeyBindings;
use crate::ui::main_app::save_app_state;
use crate::ui::main_app::MainApp;
use egui::FontFamily;
use std::path::Path;

#[derive(Default)]
pub struct FluxApp {
    pub main_app: MainApp,
    pub show_settings: bool,
    pub async_ctx: AsyncCtx,
    pub keybinds: KeyBindings,
}
impl FluxApp {
    pub fn new() -> Self {
        let keybinds = KeyBindings::load_from_file("keybinds.json").unwrap_or_default();
        // using non a non derived default will cause a infinite recursion loop
        let main_app = MainApp::new();
        Self { keybinds, main_app, ..Default::default() }
    }
}
impl eframe::App for FluxApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            for (_text_style, font_id) in ui.style_mut().text_styles.iter_mut() {
                font_id.family = FontFamily::Monospace;
            }
            egui::menu::bar(ui, |ui| {
                use egui::special_emojis::GITHUB;
                egui::widgets::global_theme_preference_buttons(ui);
                ui.add_space(16.0);

                ui.input(|i| {
                    if self.keybinds.action_triggered(Action::ToggleShowSettings, i) {
                        self.show_settings = !self.show_settings;
                    }
                });

                if self.show_settings {
                    ui.toggle_value(&mut self.show_settings, "Hide settings");
                } else {
                    ui.toggle_value(&mut self.show_settings, "Show settings");
                }
                ui.add_space(16.0);

                egui::ComboBox::from_label("Select font size")
                    .selected_text(format!("{}", self.main_app.validation_panel.font_size))
                    .show_ui(ui, |ui| {
                        for size in 10..=32 {
                            if ui
                                .selectable_label(
                                    self.main_app.validation_panel.font_size == size as f32,
                                    size.to_string(),
                                )
                                .clicked()
                            {
                                self.main_app.validation_panel.font_size = size as f32;
                            }
                        }
                    });
                ui.add_space(16.0);
                ui.label(format!(
                    "Current project: {}",
                    self.main_app
                        .validation_panel
                        .selected_project
                        .as_ref()
                        .map(|p| &p.name)
                        .unwrap_or(&"None selected".to_owned())
                ));
                ui.add_space(16.0);
                ui.label(format!(
                    "Display timezone: {}",
                    self.main_app
                        .validation_panel
                        .selected_project
                        .as_ref()
                        .map(|p| p.tz.to_string())
                        .unwrap_or("None selected".to_owned())
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::RIGHT), |ui| {
                    ui.hyperlink_to(
                        format!("{GITHUB} fluxrs on GitHub"),
                        "https://github.com/kootepe/fluxrs.git",
                    );
                });
            });
        });

        if self.show_settings {
            self.main_app.settings_ui(ctx, &self.async_ctx, &mut self.keybinds);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_app.ui(ui, ctx, &mut self.async_ctx, &self.keybinds);
        });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.main_app.validation_panel.commit_all_dirty_cycles(&self.async_ctx); // <-- do cleanup here
        let app = &self.main_app.validation_panel;
        let path = Path::new("app_state.json");
        let _ = save_app_state(app, path);
    }
}
