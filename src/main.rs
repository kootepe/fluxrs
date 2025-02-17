use std::env;
use std::process;

use fluxrs::Config;

fn main() {
    let inputs = env::args();
    let config = Config::build(inputs).unwrap_or_else(|err| {
        println!("Parsing problem {err}");
        process::exit(1)
    });
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "My egui App",
        native_options,
        Box::new(|cc| Ok(Box::new(MyEguiApp::new(cc)))),
    );

    // NOTE: I dont think this error will ever happen since they are being handled in run?
    if let Err(e) = fluxrs::run(config) {
        println!("App error: {e}.")
    }
}
#[derive(Default)]
struct MyEguiApp {}

impl MyEguiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self::default()
    }
}

impl eframe::App for MyEguiApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Hello World!");
        });
    }
}
