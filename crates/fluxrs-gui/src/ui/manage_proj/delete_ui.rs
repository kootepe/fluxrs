use crate::ui::manage_proj::project_ui::MsgType;
use crate::ui::manage_proj::project_ui::{clicked_outside_window, ProjectApp};
use egui::{Align2, Color32, Frame, Window};
use rusqlite::{params, Connection, Result};

impl ProjectApp {
    fn close_proj_delete(&mut self) {
        self.proj_delete_open = false;
        self.proj_to_delete = None;
    }

    pub fn show_proj_delete_prompt(&mut self, ctx: &egui::Context) {
        if !self.proj_delete_open {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_proj_delete();
            return;
        }

        // Use a local 'open' to avoid borrowing self inside .open(...)
        let mut open = self.proj_delete_open;

        // Optional: block background-close while a popup is open
        // let mut can_close = true;
        let mut can_close = !self.verify_delete_open;
        let wr = egui::Window::new("Delete projects")
            .open(&mut open) // <-- local, not borrowing self
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(!self.verify_delete_open, |ui| {
                if !self.all_projects.is_empty() {
                    egui::ComboBox::from_label("Project to delete")
                        .selected_text(self
                            .proj_to_delete
                            .as_deref()
                            .unwrap_or("Select Project"))
                        .show_ui(ui, |ui| {
                            can_close = false; // keep window open while popup is active
                            for project in &self.all_projects {
                                let is_selected = self
                                    .proj_to_delete
                                    .as_deref()
                                    .map(|p| p == project.name)
                                    .unwrap_or(false);
                                if ui.selectable_label(is_selected, &project.name).clicked() {
                                    self.proj_to_delete = Some(project.name.clone());
                                }
                            }
                        });
                } else {
                    ui.label("No projects found.");
                }

                if let Some(name) = self.proj_to_delete.as_ref() {
                    if ui.button(format!("Delete project '{}' and all its associated data", name)).clicked() {
                        self.verify_delete_open = true;
                        can_close = false;
                    }
                }

                if ui.button("Close").clicked() {
                    self.close_proj_delete();
                }
 });           });

        // Close if user clicked outside (unless a popup is open)
        if clicked_outside_window(ctx, wr.as_ref()) && can_close {
            self.close_proj_delete();
        }
    }
    fn close_verify_delete(&mut self) {
        self.verify_delete_open = false;
        self.del_message = None;
        self.delete_success = false;
    }

    pub fn show_verify_delete(&mut self, ctx: &egui::Context) {
        if !self.verify_delete_open {
            return;
        }

        // Close on Esc
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_verify_delete();
            return;
        }
        // Use a local 'open' to avoid borrowing self inside .open(...)
        let mut open = self.verify_delete_open;

        let wr = egui::Window::new("Verify delete")
            .open(&mut open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 100.0))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(egui::Color32::from_rgb(30, 30, 30))
                    .corner_radius(8),
            )
            .show(ctx, |ui| {
                ui.heading("Are you sure? You are about to delete the project and all it's associated data.");
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!self.delete_success, |ui| {
                        if ui.button("Yes").clicked() {
                            let mut conn =
                                Connection::open("fluxrs.db").expect("Failed to open database");
                            let name_to_delete = self.proj_to_delete.as_ref().unwrap();
                            match delete_project_data(&mut conn, name_to_delete) {
                                Ok(_) => {
                                    self.del_message = Some(MsgType::Good(format!(
                                        "Successfully deleted project '{}'!",
                                        name_to_delete
                                    )));
                                    self.all_projects.retain(|p| p.name != *name_to_delete);
                                    if self
                                        .project
                                        .as_ref()
                                        .is_some_and(|p| p.name == *name_to_delete)
                                    {
                                        self.project = None;
                                    }
                                    self.proj_to_delete = None;
                                    self.delete_success = true;
                                },
                                Err(e) => {
                                    self.del_message = Some(MsgType::Good(format!(
                                        "Couldn't delete all data for project '{}': {}!",
                                        name_to_delete, e
                                    )));
                                },
                            }
                        }

                        if ui.button("No").clicked() {
                            self.close_verify_delete();
                        }
                    });
                    if self.delete_success && ui.button("Close").clicked() {
                        self.close_verify_delete();
                    }
                });
                let reserved_h = 44.0;
                let (rect, _resp) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), reserved_h),
                    egui::Sense::hover(),
                );

                if let Some(msg) = &self.del_message {
                    let (text, color) = msg.as_str_and_color();
                    ui.put(rect, egui::Label::new(egui::RichText::new(text).color(color)));
                }
            });

        // Close if user clicked outside the window
        if clicked_outside_window(ctx, wr.as_ref()) {
            self.close_verify_delete();
        }

        // Close if the titlebar “X” was used this frame
    }
}

pub fn delete_project_data(conn: &mut Connection, project_name: &str) -> Result<()> {
    let tx = conn.transaction()?; // optional transaction for atomic delete

    let sql = "DELETE FROM projects WHERE project_name == ?1".to_string();
    tx.execute(&sql, params![project_name])?;

    tx.commit()?;
    Ok(())
}
