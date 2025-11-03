use crate::instruments::instruments::InstrumentType;
use crate::ui::manage_proj::project_ui::MsgType;
use crate::ui::manage_proj::project_ui::{clicked_outside_window, ProjectApp};
use crate::ui::tz_picker::timezone_combo;
use crate::ui::validation_ui::Mode;
use egui::{Align2, Area, Color32, Context, Frame, Id, Window};
use rusqlite::{params, Connection, Result};
use std::error::Error;

impl ProjectApp {
    pub fn show_proj_delete_prompt(&mut self, ctx: &egui::Context) {
        if !self.proj_delete_open {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.proj_delete_open = false;
            self.proj_to_delete = None;
            return;
        }

        let mut can_close = true;
        let wr = Window::new("Delete projects")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                Frame::window(&ctx.style()).fill(Color32::from_rgb(30, 30, 30)).corner_radius(8), // .shadow(egui::epaint::Shadow::big_dark()),
            )
            .show(ctx, |ui| {
                if !self.all_projects.is_empty() {
                    egui::ComboBox::from_label("Project to delete")
                        .selected_text(
                            self.proj_to_delete
                                .as_ref()
                                .map(|p| p.to_string())
                                .unwrap_or_else(|| "Select Project".to_string()),
                        )
                        .show_ui(ui, |ui| {
                            can_close = false;

                            for project in &self.all_projects {
                                let is_selected = self
                                    .proj_to_delete
                                    .as_ref()
                                    .map(|p| *p == project.name)
                                    .unwrap_or(false);
                                if ui.selectable_label(is_selected, &project.name).clicked() {
                                    self.proj_to_delete = Some(project.name.clone())
                                }
                            }
                        });
                } else {
                    ui.label("No projects found.");
                }
                if self.proj_to_delete.is_some() {
                    if ui
                        .button(format!(
                            "Delete project '{}' and all it's associated data",
                            self.proj_to_delete.as_ref().unwrap(),
                        ))
                        .clicked()
                    {
                        self.verify_delete_open = true;
                        self.proj_delete_open = false;
                    }
                }
                if ui.button("Close").clicked() {
                    self.proj_delete_open = false;
                    self.proj_to_delete = None;
                }
            });
        if clicked_outside_window(ctx, wr.as_ref()) && can_close {
            self.proj_delete_open = false;
        }
    }
    pub fn show_verify_delete(&mut self, ctx: &egui::Context) {
        if !self.verify_delete_open {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.verify_delete_open = false;
            self.proj_delete_open = true;
            self.del_message = None;
            return;
        }

        let wr = Window::new("Verify delete")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 100.0))
            .frame(
                Frame::window(&ctx.style()).fill(Color32::from_rgb(30, 30, 30)).corner_radius(8), // .shadow(egui::epaint::Shadow::big_dark()),
            )
            .show(ctx, |ui| {
                if ui
                    .button(format!(
                        "Delete project '{}' and all it's associated data",
                        self.proj_to_delete.as_ref().unwrap()
                    ))
                    .clicked()
                {
                    let mut conn = Connection::open("fluxrs.db").expect("Failed to open database");
                    let name_to_delete = self.proj_to_delete.as_ref().unwrap();
                    match delete_project_data(&mut conn, name_to_delete) {
                        Ok(_) => {
                            self.del_message = Some(MsgType::Good(format!(
                                "Successfully deleted project '{}'!",
                                name_to_delete
                            )));
                            self.all_projects.retain(|p| p.name != *name_to_delete);
                            if self.project.is_some()
                                && self.project.as_ref().unwrap().name == *name_to_delete
                            {
                                self.project = None
                            }
                        },
                        Err(e) => {
                            self.del_message = Some(MsgType::Good(format!(
                                "Couldn't delete all data for project '{}': {}!",
                                name_to_delete, e
                            )))
                        },
                    }
                }

                if ui.button("Close").clicked() {
                    self.verify_delete_open = false;
                    self.proj_delete_open = true;
                    self.del_message = None;
                }
                if let Some(msg) = &self.del_message {
                    let (text, color) = msg.as_str_and_color();
                    ui.label(egui::RichText::new(text).color(color));
                }
            });

        if clicked_outside_window(ctx, wr.as_ref()) {
            self.verify_delete_open = false;
            self.proj_delete_open = true;
            self.del_message = None;
        }
    }
}

pub fn delete_project_data(conn: &mut Connection, project_id: &str) -> Result<()> {
    let tables = [
        "measurements",
        "chamber_metadata",
        "height",
        "meteo",
        "cycles",
        "projects",
        "fluxes",
        "flux_history",
    ];

    let tx = conn.transaction()?; // optional transaction for atomic delete

    for table in &tables {
        let sql = format!("DELETE FROM {} WHERE project_id == ?1", table);
        tx.execute(&sql, params![project_id])?;
    }

    tx.commit()?;
    Ok(())
}
