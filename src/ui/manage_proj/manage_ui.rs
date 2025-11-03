use crate::instruments::instruments::InstrumentType;
use crate::ui::manage_proj::project_ui::MsgType;
use crate::ui::manage_proj::project_ui::{clicked_outside_window, ProjectApp};
use crate::ui::tz_picker::timezone_combo;
use crate::ui::validation_ui::Mode;
use egui::{Align2, Area, Color32, Context, Frame, Id, Window};
use std::error::Error;

impl ProjectApp {
    pub fn show_manage_proj_data(&mut self, ctx: &egui::Context) {
        if !self.proj_manage_open {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.proj_manage_open = false;
            return;
        }
        let can_close = true;
        let wr = Window::new("Manage project")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                Frame::window(&ctx.style()).fill(Color32::from_rgb(30, 30, 30)).corner_radius(8), // .shadow(egui::epaint::Shadow::big_dark()),
            )
            .show(ctx, |ui| {
                ui.button("Delete measurement data");
                ui.button("Delete calculated fluxes");
                ui.button("Delete meteo data");
                ui.button("Delete height data");
                ui.button("Delete chamber data");
            });

        if clicked_outside_window(ctx, wr.as_ref()) && can_close {
            self.proj_manage_open = false;
        }
    }
}
