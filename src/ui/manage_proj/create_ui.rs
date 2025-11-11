use crate::instruments::instruments::InstrumentType;
use crate::ui::manage_proj::project_ui::MsgType;
use crate::ui::manage_proj::project_ui::{clicked_outside_window, ProjectApp};
use crate::ui::tz_picker::timezone_combo;
use crate::ui::validation_ui::Mode;
use egui::{Align2, Area, Color32, Context, Frame, Id, Window};
use std::error::Error;

impl ProjectApp {
    pub fn close_proj_create(&mut self) {
        self.proj_create_open = false;
        self.project_name = "".to_owned();
        self.selected_serial = "".to_owned();
        self.selected_instrument = InstrumentType::default();
        self.project_timezone = None;
        self.project_timezone_str = "".to_owned();
        self.main_gas = None;
        self.deadband = 30.;
        self.min_calc_len = 60.;
        self.mode = Mode::default();
        self.message = None;
        self.del_message = None;
        self.project_timezone_str.clear();
        self.project_timezone = None;
        self.tz_state.selected = None;
        self.tz_state.query.clear();
    }

    pub fn show_proj_create_prompt(&mut self, ctx: &egui::Context) {
        if !self.proj_create_open {
            return;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.close_proj_create();
            return;
        }

        let mut can_close = true;

        let mut open = self.proj_create_open;

        let wr = Window::new("Create new project")
            .open(&mut open)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .frame(
                Frame::window(&ctx.style())
                    .fill(Color32::from_rgb(30, 30, 30))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                ui.heading("Create New project");
                ui.add_space(20.);
                ui.label("Project name:");
                ui.text_edit_singleline(&mut self.project_name);
                ui.add_space(10.);

                ui.label("Project display timezone:");
                if let Some(tz) =
                    timezone_combo(ui, "project_timezone_combo_v031", &mut self.tz_state)
                {
                    self.project_timezone = Some(tz);
                    self.project_timezone_str = tz.to_string();
                }

                if let Some(tz) = self.project_timezone {
                    ui.label(format!("Selected timezone: {}", tz));
                } else {
                    ui.label("No timezone selected");
                }

                if ui.button("Clear timezone").clicked() {
                    self.project_timezone_str.clear();
                    self.project_timezone = None;
                    self.tz_state.selected = None;
                    self.tz_state.query.clear();
                }
                ui.add_space(10.);

                ui.label("Select instrument:");
                egui::ComboBox::from_label("Instrument")
                    .selected_text(self.selected_instrument.to_string())
                    .show_ui(ui, |ui| {
                        can_close = false;
                        for instrument in InstrumentType::available_instruments() {
                            ui.selectable_value(
                                &mut self.selected_instrument,
                                instrument,
                                instrument.to_string(),
                            );
                        }
                    });

                ui.add_space(10.);
                ui.label("Instrument serial:");
                ui.text_edit_singleline(&mut self.selected_serial);

                let available_gases = self.selected_instrument.available_gases();
                if !available_gases.is_empty() {
                    ui.label("Select Gas:");
                    egui::ComboBox::from_label("Gas Type")
                        .selected_text(
                            self.main_gas
                                .map_or_else(|| "Select Gas".to_string(), |g| g.to_string()),
                        )
                        .show_ui(ui, |ui| {
                            can_close = false;
                            for gas in available_gases {
                                ui.selectable_value(&mut self.main_gas, Some(gas), gas.to_string());
                            }
                        });

                    if let Some(gas) = self.main_gas {
                        ui.label(format!("Selected Gas: {}", gas));
                    }
                } else {
                    ui.label("No gases available for this instrument.");
                }

                ui.add_space(10.0);
                ui.label("Minimum calculation data length in seconds:");
                ui.add(egui::DragValue::new(&mut self.min_calc_len).speed(1.0).range(0.0..=3600.0));

                ui.add_space(10.0);
                ui.label("Deadband in seconds:");
                ui.add(egui::DragValue::new(&mut self.deadband).speed(1.0).range(0.0..=3600.0));

                ui.add_space(10.0);
                ui.label("Select flux finding mode:");
                egui::ComboBox::from_label("Mode").selected_text(format!("{}", self.mode)).show_ui(
                    ui,
                    |ui| {
                        can_close = false;
                        ui.selectable_value(
                            &mut self.mode,
                            Mode::AfterDeadband,
                            Mode::AfterDeadband.to_string(),
                        );
                        ui.selectable_value(
                            &mut self.mode,
                            Mode::BestPearsonsR,
                            Mode::BestPearsonsR.to_string(),
                        );
                    },
                );

                ui.add_space(10.0);

                let enable_add_proj = !self.project_name.trim().is_empty()
                    && self.project_timezone.is_some()
                    && !self.selected_serial.trim().is_empty();

                ui.horizontal(|ui| {
                    if ui.add_enabled(enable_add_proj, egui::Button::new("Add Project")).clicked() {
                        if let Some(project) = self.build_project_from_form() {
                            match self.save_project_to_db(&project) {
                                Ok(_) => {
                                    self.message = Some(MsgType::Good(format!(
                                        "Successfully created project '{}'",
                                        project.name
                                    )));
                                    // self.proj_create_open = true;
                                },
                                Err(err) => {
                                    let msg = err
                                        .source()
                                        .map(|source| source.to_string())
                                        .unwrap_or_else(|| err.to_string());
                                    self.message = Some(MsgType::Bad(format!(
                                        "Failed to create project: {}",
                                        msg
                                    )));
                                },
                            }
                        } else {
                            self.message = Some(MsgType::Bad(
                                "Please fill out all required fields.".to_string(),
                            ));
                        }
                    }

                    if ui.button("Close").clicked() {
                        self.close_proj_create();
                    }
                });
                if let Some(msg) = &self.message {
                    let (text, color) = msg.as_str_and_color();
                    ui.label(egui::RichText::new(text).color(color));
                }
            });
        if clicked_outside_window(ctx, wr.as_ref()) && can_close {
            self.close_proj_create();
        }
    }
}
