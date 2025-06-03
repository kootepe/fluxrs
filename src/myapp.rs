use crate::keybinds::Action;
use crate::validation_app::keybind_triggered;
use crate::validation_app::MainApp;
use egui::FontFamily;

#[derive(Default)]
pub struct MyApp {
    pub main_app: MainApp,
    pub show_settings: bool,
}
impl MyApp {
    pub fn new() -> Self {
        Default::default()
    }
}
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // self.apply_font_size(ctx, self.main_app.validation_panel.font_size);
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            for (_text_style, font_id) in ui.style_mut().text_styles.iter_mut() {
                // font_id.size = self.validation_panel.font_size;
                font_id.family = FontFamily::Monospace;
            }
            egui::menu::bar(ui, |ui| {
                use egui::special_emojis::GITHUB;
                egui::widgets::global_theme_preference_buttons(ui);
                ui.add_space(16.0);

                ui.input(|i| {
                    for event in &i.raw.events {
                        if keybind_triggered(
                            event,
                            &self.main_app.validation_panel.keybinds,
                            Action::ToggleShowSettings,
                        ) {
                            self.show_settings = !self.show_settings;
                        }
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::RIGHT), |ui| {
                    ui.hyperlink_to(
                        format!("{GITHUB} fluxrs on GitHub"),
                        "https://github.com/kootepe/fluxrs.git",
                    );
                });
            });
        });

        if self.show_settings {
            self.main_app.settings_ui(ctx);
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_app.ui(ui, ctx);
        });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.main_app.validation_panel.commit_all_dirty_cycles(); // <-- do cleanup here
    }
}
