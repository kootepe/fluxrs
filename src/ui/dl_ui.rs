use crate::flux::FluxKind;
use crate::fluxes_schema::{make_select_all_fluxes, OTHER_COLS};
use crate::gastype::GasType;
use crate::ui::validation_ui::ValidationApp;
use crate::Project;
use chrono::offset::LocalResult;
use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use csv::Writer;
use rusqlite::{types::ValueRef, Connection, Result};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::path::Path;

#[derive(Default)]
pub struct DownloadApp {
    project: Option<Project>,
    pub gas_checked: std::collections::HashMap<GasType, bool>,
    pub model_checked: std::collections::HashMap<FluxKind, bool>,
    gases: Vec<GasType>,
}

impl DownloadApp {
    pub fn dl_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, project: Option<Project>) {
        self.project = project;

        ui.heading("data downloader");
        if self.project.is_none() {
            ui.label("add or select a project in the initiate project tab.");
            return;
        }
        let gases = self.project.as_ref().unwrap().instrument.available_gases();
        for gas in &gases {
            self.gas_checked.entry(*gas).or_insert(false);
        }
        let models = &[FluxKind::Linear, FluxKind::Poly, FluxKind::RobLin];
        for model in models {
            self.model_checked.entry(*model).or_insert(false);
        }
        ui.separator();
        ui.label("Select gases to include:");

        ui.horizontal(|ui| {
            for gas in &gases {
                let checked = self.gas_checked.get_mut(gas).unwrap();
                ui.checkbox(checked, gas.to_string());
            }
        });

        ui.separator();
        ui.label("Select models to include:");
        ui.horizontal(|ui| {
            for model in models {
                let checked = self.model_checked.get_mut(model).unwrap();
                ui.checkbox(checked, model.to_string());
            }
        });

        if ui.button("download all calculated fluxes for current project.").clicked() {
            let export_name = format!("fluxrs_{}.csv", self.project.as_ref().unwrap().name);
            match self.export_sqlite_to_csv(
                "fluxrs.db",
                &export_name,
                &self.project.clone().unwrap(),
            ) {
                Ok(_) => println!("succesfully downloaded csv."),
                Err(e) => println!("failed to download csv. error: {}", e),
            }
        }
    }
    pub fn export_sqlite_to_csv(
        &mut self,
        db_path: &str,
        csv_path: &str,
        project: &Project,
    ) -> Result<(), Box<dyn Error>> {
        let conn = Connection::open(db_path)?;

        let query = make_select_all_fluxes();
        let mut stmt = conn.prepare(&query)?;

        // Column names in DB order
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        // Build drop_after_processing based on which models are unchecked
        let mut drop_after_processing = vec![
            "start_lag_s",
            "close_lag_s",
            "open_lag_s",
            "end_lag_s",
            "lin_range_start",
            "lin_range_end",
            "roblin_range_start",
            "roblin_range_end",
            "poly_range_start",
            "poly_range_end",
        ];
        let lin_drops = [
            "lin_flux",
            "lin_r2",
            "lin_adj_r2",
            "lin_intercept",
            "lin_slope",
            "lin_sigma",
            "lin_p_value",
            "lin_aic",
            "lin_rmse",
        ];

        let roblin_drops = [
            "roblin_flux",
            "roblin_r2",
            "roblin_adj_r2",
            "roblin_intercept",
            "roblin_slope",
            "roblin_sigma",
            "roblin_p_value",
            "roblin_aic",
            "roblin_rmse",
        ];
        let poly_drops = [
            "poly_flux",
            "poly_r2",
            "poly_adj_r2",
            "poly_intercept",
            "poly_slope",
            "poly_sigma",
            "poly_p_value",
            "poly_aic",
            "poly_rmse",
            "poly_a0",
            "poly_a1",
            "poly_a2",
        ];
        if !self.model_checked.get(&FluxKind::Linear).copied().unwrap_or(false) {
            drop_after_processing.extend(lin_drops);
        }

        if !self.model_checked.get(&FluxKind::RobLin).copied().unwrap_or(false) {
            drop_after_processing.extend(roblin_drops);
        }

        if !self.model_checked.get(&FluxKind::Poly).copied().unwrap_or(false) {
            drop_after_processing.extend(poly_drops);
        }
        // Final output column order
        let final_columns: Vec<String> = column_names
            .iter()
            .filter(|c| !drop_after_processing.contains(&c.as_str()))
            .cloned()
            .collect();

        // We'll need to know where the "gas" column is in final_columns for filtering rows
        let gas_col_index = final_columns.iter().position(|c| c == "gas");

        let tz: Tz = project.tz;

        // Build the rows iterator (each item is Result<Vec<String>, rusqlite::Error>)
        let rows = stmt.query_map([&project.name], {
            let column_names = column_names.clone();
            let final_columns = final_columns.clone();
            let drop_after_processing = drop_after_processing.clone();
            move |row| {
                use std::collections::HashMap;
                let mut record: HashMap<String, String> = HashMap::new();

                // collect DB row into record
                for (i, col_name) in column_names.iter().enumerate() {
                    let val = match row.get_ref(i)? {
                        ValueRef::Null => "".to_string(),
                        ValueRef::Integer(ts) => ts.to_string(),
                        ValueRef::Real(f) => f.to_string(),
                        ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                        ValueRef::Blob(_) => "[BLOB]".to_string(),
                    };
                    record.insert(col_name.clone(), val);
                }

                // start_time local transform using lag
                if let (Some(start_time_str), Some(start_lag_str)) =
                    (record.get("start_time"), record.get("start_lag_s"))
                {
                    if let (Ok(ts_utc), Ok(lag_s)) =
                        (start_time_str.parse::<i64>(), start_lag_str.parse::<f64>())
                    {
                        let adjusted = ts_utc as f64 - lag_s;
                        let adjusted_i64 = adjusted as i64;

                        let tz_time_str = match Utc.timestamp_opt(adjusted_i64, 0) {
                            LocalResult::Single(dt_utc) => {
                                dt_utc.with_timezone(&tz).format("%Y-%m-%d %H:%M:%S").to_string()
                            },
                            LocalResult::Ambiguous(dt1, _) => {
                                dt1.with_timezone(&tz).format("%Y-%m-%d %H:%M:%S").to_string()
                            },
                            LocalResult::None => start_time_str.clone(),
                        };

                        record.insert("start_time".to_string(), tz_time_str);
                    }
                }

                // close_offset -= close_lag_s
                if let (Some(close_offset), Some(close_lag_s)) =
                    (record.get("close_offset"), record.get("close_lag_s"))
                {
                    if let (Ok(co), Ok(cl)) =
                        (close_offset.parse::<i64>(), close_lag_s.parse::<i64>())
                    {
                        record.insert("close_offset".to_string(), (co - cl).to_string());
                    }
                }

                // open_offset -= open_lag_s
                if let (Some(open_offset), Some(open_lag_s)) =
                    (record.get("open_offset"), record.get("open_lag_s"))
                {
                    if let (Ok(oo), Ok(ol)) =
                        (open_offset.parse::<i64>(), open_lag_s.parse::<i64>())
                    {
                        record.insert("open_offset".to_string(), (oo - ol).to_string());
                    }
                }

                // end_offset -= end_lag_s
                if let (Some(end_offset), Some(end_lag_s)) =
                    (record.get("end_offset"), record.get("end_lag_s"))
                {
                    if let (Ok(eo), Ok(el)) = (end_offset.parse::<i64>(), end_lag_s.parse::<i64>())
                    {
                        record.insert("end_offset".to_string(), (eo - el).to_string());
                    }
                }

                // gas: turn numeric code into readable gas
                if let Some(i) = record.get("gas").and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(gas) = GasType::from_int(i) {
                        record.insert("gas".to_string(), gas.to_string());
                    }
                }

                if let Some(i) = record.get("main_gas").and_then(|s| s.parse::<usize>().ok()) {
                    if let Some(gas) = GasType::from_int(i) {
                        record.insert("main_gas".to_string(), gas.to_string());
                    }
                }

                // Optionally drop internal columns from the row map
                for col in &drop_after_processing {
                    record.remove(*col);
                }

                // Build Vec<String> in final_columns order
                let row_values: Vec<String> = final_columns
                    .iter()
                    .map(|name| record.get(name).cloned().unwrap_or_default())
                    .collect();

                Ok(row_values)
            }
        })?;

        // Now we write the CSV manually, filtering rows by gas before writing
        let file = File::create(Path::new(csv_path))?;
        let mut wtr = Writer::from_writer(file);

        // header
        wtr.write_record(&final_columns)?;

        // rows
        for row_result in rows {
            let row_values = row_result?; // Vec<String>

            // If we know which column is "gas", enforce gas filter
            if let Some(gas_idx) = gas_col_index {
                if let Some(gas_value) = row_values.get(gas_idx) {
                    let gas = gas_value.parse::<GasType>().unwrap();
                    if !self.gas_checked.get(&gas).copied().unwrap_or(false) {
                        continue; // <- now we can use continue in the for-loop 😎
                    }
                }
            }

            wtr.write_record(&row_values)?;
        }

        wtr.flush()?;
        Ok(())
    }
}
