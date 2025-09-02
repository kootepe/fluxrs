use crate::fluxes_schema::{make_select_all_fluxes, OTHER_COLS};
use crate::gastype::GasType;
use crate::ui::validation_ui::ValidationApp;
use chrono::{DateTime, Utc};
use csv::Writer;
use rusqlite::{types::ValueRef, Connection, Result};
use std::error::Error;
use std::fs::File;
use std::path::Path;

impl ValidationApp {
    pub fn dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        ui.heading("Data downloader");
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if ui.button("Download all calculated fluxes for current project.").clicked() {
            match export_sqlite_to_csv("fluxrs.db", "fluxrs.csv", self.get_project().name.clone()) {
                Ok(_) => println!("Succesfully downloaded csv."),
                Err(e) => println!("Failed to download csv. Error: {}", e),
            }
        }
    }

    pub fn _dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        // WIP dl_ui function
        ui.heading("Data downloader");
        if self.selected_project.is_none() {
            ui.label("Add or select a project in the Initiate project tab.");
            return;
        }
        if ui.button("Download all calculated fluxes for current project.").clicked() {
            match export_sqlite_to_csv("fluxrs.db", "fluxrs.csv", self.get_project().name.clone()) {
                Ok(_) => println!("Succesfully downloaded csv."),
                Err(e) => println!("Failed to download csv. Error: {}", e),
            }
        }
        let mut checked = false;
        ui.horizontal(|ui| {
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.flux_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.measurement_r2_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.calc_range_start_col());
                    }
                });
            });
            ui.group(|ui| {
                ui.vertical(|ui| {
                    for gas in self.selected_project.as_ref().unwrap().instrument.available_gases()
                    {
                        ui.checkbox(&mut checked, gas.calc_range_end_col());
                    }
                });
            });
        });
        ui.group(|ui| {
            ui.vertical(|ui| {
                for col in OTHER_COLS {
                    ui.checkbox(&mut checked, *col);
                }
            });
        });
    }
}

pub fn export_sqlite_to_csv(
    db_path: &str,
    csv_path: &str,
    project: String,
) -> Result<(), Box<dyn Error>> {
    let conn = Connection::open(db_path)?;

    let query = make_select_all_fluxes();
    let mut stmt = conn.prepare(&query)?;
    let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let column_count = column_names.len();

    let rows = stmt.query_map([&project], {
        let column_names = column_names.clone();
        move |row| {
            let mut values = Vec::with_capacity(column_count);
            for (i, col_name) in column_names.iter().enumerate() {
                let val = match row.get_ref(i)? {
                    ValueRef::Null => "".to_string(),
                    ValueRef::Integer(ts) => {
                        if col_name == "start_time" {
                            if let Some(dt) = DateTime::<Utc>::from_timestamp(ts, 0) {
                                dt.format("%Y-%m-%d %H:%M:%S").to_string()
                            } else {
                                ts.to_string()
                            }
                        } else if col_name == "gas" {
                            if let Some(gas) = GasType::from_int(ts as usize) {
                                format!("{}", gas)
                            } else {
                                ts.to_string()
                            }
                        } else {
                            ts.to_string()
                        }
                    },
                    ValueRef::Real(f) => f.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                    ValueRef::Blob(_) => "[BLOB]".to_string(),
                };
                values.push(val);
            }
            Ok(values)
        }
    })?;

    let file = File::create(Path::new(csv_path))?;
    let mut wtr = Writer::from_writer(file);

    wtr.write_record(&column_names)?;

    for row in rows {
        wtr.write_record(&row?)?;
    }

    wtr.flush()?;
    Ok(())
}
