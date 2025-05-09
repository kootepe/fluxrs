use crate::validation_app::MainApp;

#[derive(Default)]
pub struct MyApp {
    pub main_app: MainApp,
}
impl MyApp {
    /// Called once before the first frame.
    pub fn new() -> Self {
        Default::default()
    }
}
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                ui.add_space(16.0);
                use egui::special_emojis::GITHUB;
                ui.hyperlink_to(
                    format!("{GITHUB} fluxrs on GitHub"),
                    "https://github.com/kootepe/fluxrs.git",
                );

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
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_app.ui(ui, ctx);
        });
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.main_app.validation_panel.commit_all_dirty_cycles(); // <-- do cleanup here
    }
}
