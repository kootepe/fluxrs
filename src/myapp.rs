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
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_app.ui(ui, ctx);
        });
    }
}
