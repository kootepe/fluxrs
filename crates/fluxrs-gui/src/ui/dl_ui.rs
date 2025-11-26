use crate::ui::validation_ui::ValidationApp;
use chrono::offset::LocalResult;
use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use csv::Writer;
use fluxrs_core::data_formats::meteodata::MeteoSource;
use fluxrs_core::db::fluxes_schema::{make_select_all_fluxes, OTHER_COLS};
use fluxrs_core::flux::{FluxKind, FluxUnit};
use fluxrs_core::gastype::GasType;
use fluxrs_core::project::Project;
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
    pub gas_unit_choice: std::collections::HashMap<GasType, FluxUnit>,
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
        let gases = self.project.as_ref().unwrap().instrument.model.available_gases();
        for gas in &gases {
            self.gas_checked.entry(*gas).or_insert(false);
        }
        let models = FluxKind::all();
        for model in models {
            self.model_checked.entry(*model).or_insert(false);
        }

        for gas in &gases {
            self.gas_unit_choice.entry(*gas).or_insert(FluxUnit::UmolM2S);
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

        ui.separator();
        ui.label("Select unit per gas:");

        for gas in &gases {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(gas.to_string());

                    // current chosen unit for this gas
                    let chosen_unit = self.gas_unit_choice.get_mut(gas).unwrap();

                    for unit in FluxUnit::all() {
                        ui.radio_value(chosen_unit, *unit, unit.to_string());
                    }
                });
            });
        }
        let any_gas_selected = self.gas_checked.values().any(|&v| v);
        let any_model_selected = self.model_checked.values().any(|&v| v);

        ui.add_enabled_ui(any_gas_selected && any_model_selected, |ui| {
            if ui.button("Download all calculated fluxes for current project.").clicked() {
                let export_name = format!("fluxrs_{}.csv", self.project.as_ref().unwrap().name);
                match self.export_sqlite_to_csv(
                    "fluxrs.db",
                    &export_name,
                    &self.project.clone().unwrap(),
                ) {
                    Ok(_) => println!("Successfully downloaded CSV."),
                    Err(e) => println!("Failed to download CSV. Error: {}", e),
                }
            }
        });
        if !any_gas_selected || !any_model_selected {
            ui.label("Select a gas and model to download data.");
        };
    }

    pub fn export_sqlite_to_csv(
        &mut self,
        db_path: &str,
        csv_path: &str,
        project: &Project,
    ) -> Result<(), Box<dyn Error>> {
        use std::collections::HashMap;

        let conn = Connection::open(db_path)?;

        let query = make_select_all_fluxes();
        let mut stmt = conn.prepare(&query)?;

        // Column names in DB order
        let column_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        // Build drop_after_processing based on which models are unchecked
        let mut drop_after_processing =
            vec!["start_lag_s", "close_lag_s", "open_lag_s", "end_lag_s"];

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
            "lin_cv",
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
            "roblin_cv",
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
            "poly_cv",
            "poly_a0",
            "poly_a1",
            "poly_a2",
        ];
        let exp_drops = [
            "exp_flux",
            "exp_r2",
            "exp_adj_r2",
            "exp_intercept",
            "exp_slope",
            "exp_sigma",
            "exp_p_value",
            "exp_aic",
            "exp_rmse",
            "exp_cv",
            "exp_a",
            "exp_b",
        ];

        let lin_enabled = self.model_checked.get(&FluxKind::Linear).copied().unwrap_or(false);
        let roblin_enabled = self.model_checked.get(&FluxKind::RobLin).copied().unwrap_or(false);
        let poly_enabled = self.model_checked.get(&FluxKind::Poly).copied().unwrap_or(false);
        let exp_enabled = self.model_checked.get(&FluxKind::Exponential).copied().unwrap_or(false);

        if !lin_enabled {
            drop_after_processing.extend(lin_drops);
        }
        if !roblin_enabled {
            drop_after_processing.extend(roblin_drops);
        }
        if !poly_enabled {
            drop_after_processing.extend(poly_drops);
        }
        if !exp_enabled {
            drop_after_processing.extend(exp_drops);
        }

        // Which model flux cols are active
        // (&str so we can reuse the literal names directly to look up "lin_flux", etc.)
        let mut enabled_models: Vec<&str> = Vec::new();
        if lin_enabled {
            enabled_models.push("lin_flux");
        }
        if roblin_enabled {
            enabled_models.push("roblin_flux");
        }
        if poly_enabled {
            enabled_models.push("poly_flux");
        }
        if exp_enabled {
            enabled_models.push("exp_flux");
        }

        // Which gases are selected
        let selected_gases: Vec<GasType> = self
            .gas_checked
            .iter()
            .filter_map(|(gas, is_checked)| if *is_checked { Some(*gas) } else { None })
            .collect();

        // 1. Build the base columns (everything except the raw flux columns and dropped internals)
        let mut final_columns: Vec<String> = column_names
            .iter()
            .filter(|c| {
                // keep the column if:
                //  - it's NOT being dropped
                //  - it's NOT one of the raw flux cols (lin_flux, roblin_flux, poly_flux)
                !drop_after_processing.contains(&c.as_str())
                    && *c != "lin_flux"
                    && *c != "roblin_flux"
                    && *c != "poly_flux"
                    && *c != "exp_flux"
            })
            .cloned()
            .collect();

        // Save index of "gas" column *before* adding derived flux columns.
        let gas_col_index = final_columns.iter().position(|c| c == "gas");

        // 2. Add per-(gas, unit) derived flux columns for each enabled model
        //
        // We'll construct names like:
        //   "<model_col>_<gasname>_<unit_suffix>"
        //
        // where:
        //   model_col   = "lin_flux" | "roblin_flux" | "poly_flux"
        //   gasname     = gas.column_name()  (e.g. "CO2", "CH4")
        //   unit_suffix = flux_unit.suffix() (e.g. "mg_m2_h")
        //
        // Note: unit can differ per gas.

        for &model_col in &enabled_models {
            for gas in &selected_gases {
                let unit = self.gas_unit_choice.get(gas).copied().unwrap_or(FluxUnit::UmolM2S);

                let col_name = format!(
                    "{}_{}_{}",
                    model_col,
                    gas.column_name(), // you provide this, e.g. "CO2"
                    unit.suffix()      // you provide this, e.g. "mg_m2_h"
                );

                final_columns.push(col_name);
            }
        }

        // We'll need tz for timestamp conversion in rows
        let tz: Tz = project.tz;

        // clone stuff we capture into the row closure
        let unit_choice = self.gas_unit_choice.clone();
        let gas_checked = self.gas_checked.clone();
        let enabled_models_closure = enabled_models.clone();
        let selected_gases_closure = selected_gases.clone();
        let final_columns_closure = final_columns.clone();
        let drop_after_processing_closure = drop_after_processing.clone();
        let column_names_closure = column_names.clone();

        // 3. Build rows iterator. Each row -> Vec<String> in final_columns order.
        let rows = stmt.query_map([&project.id.unwrap()], move |row| {
            let mut record: HashMap<String, String> = HashMap::new();

            // collect DB row into record as text
            for (i, col_name) in column_names_closure.iter().enumerate() {
                let val = match row.get_ref(i)? {
                    ValueRef::Null => "".to_string(),
                    ValueRef::Integer(ts) => ts.to_string(),
                    ValueRef::Real(f) => f.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).to_string(),
                    ValueRef::Blob(_) => "[BLOB]".to_string(),
                };
                record.insert(col_name.clone(), val);
            }

            // ---- timestamp + lag normalization --------------------------------

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
                if let (Ok(co), Ok(cl)) = (close_offset.parse::<i64>(), close_lag_s.parse::<i64>())
                {
                    record.insert("close_offset".to_string(), (co - cl).to_string());
                }
            }

            // open_offset -= open_lag_s
            if let (Some(open_offset), Some(open_lag_s)) =
                (record.get("open_offset"), record.get("open_lag_s"))
            {
                if let (Ok(oo), Ok(ol)) = (open_offset.parse::<i64>(), open_lag_s.parse::<i64>()) {
                    record.insert("open_offset".to_string(), (oo - ol).to_string());
                }
            }

            // end_offset -= end_lag_s
            if let (Some(end_offset), Some(end_lag_s)) =
                (record.get("end_offset"), record.get("end_lag_s"))
            {
                if let (Ok(eo), Ok(el)) = (end_offset.parse::<i64>(), end_lag_s.parse::<i64>()) {
                    record.insert("end_offset".to_string(), (eo - el).to_string());
                }
            }

            // ---- gas normalization --------------------------------------------

            // "gas" in DB is numeric; turn that into GasType string ("CO2", "CH4", etc)
            if let Some(i) = record.get("gas").and_then(|s| s.parse::<usize>().ok()) {
                if let Some(gas_enum) = GasType::from_int(i) {
                    record.insert("gas".to_string(), gas_enum.to_string());
                }
            }

            // "main_gas" too, if present
            if let Some(i) = record.get("main_gas").and_then(|s| s.parse::<usize>().ok()) {
                if let Some(gas_enum) = GasType::from_int(i) {
                    record.insert("main_gas".to_string(), gas_enum.to_string());
                }
            }

            if let Some(i) = record.get("temperature_source").and_then(|s| s.parse::<i32>().ok()) {
                if let Some(temp_s) = MeteoSource::from_int(i) {
                    record.insert("temperature_source".to_string(), temp_s.to_string());
                }
            }

            if let Some(i) = record.get("pressure_source").and_then(|s| s.parse::<i32>().ok()) {
                if let Some(press_s) = MeteoSource::from_int(i) {
                    record.insert("pressure_source".to_string(), press_s.to_string());
                }
            }

            // parse back gas_enum for this row
            let row_gas_opt = record.get("gas").and_then(|s| s.parse::<GasType>().ok());

            // ---- drop internal columns we don't want in output ----------------

            for col in &drop_after_processing_closure {
                record.remove(*col);
            }

            // ---- per-(gas,unit) flux fanout -----------------------------------

            // For each enabled model ("lin_flux", etc.) and for each selected gas,
            // create a dedicated column in `record`:
            //
            //   "<model_col>_<gas>_<unit_suffix>"
            //
            // If this row's gas == that gas, convert and fill; otherwise empty.
            //
            // Then remove the original generic model_col from record so only
            // per-gas/unit columns remain.

            for &model_col in &enabled_models_closure {
                for gas in &selected_gases_closure {
                    // which unit did user pick for THIS gas?
                    let unit = unit_choice.get(gas).copied().unwrap_or(FluxUnit::UmolM2S);

                    let header_key =
                        format!("{}_{}_{}", model_col, gas.column_name(), unit.suffix());

                    // default is empty
                    let mut cell_val = String::new();

                    if let Some(row_gas) = row_gas_opt {
                        if row_gas == *gas {
                            if let Some(raw_str) = record.get(model_col) {
                                if let Ok(raw_val) = raw_str.parse::<f64>() {
                                    // convert from µmol/m²/s to chosen unit for THIS gas
                                    let converted = unit.from_umol_m2_s(raw_val, row_gas);
                                    cell_val = converted.to_string();
                                }
                            }
                        }
                    }

                    record.insert(header_key, cell_val);
                }

                // We don't want the un-fanned source left around
                record.remove(model_col);
            }

            // ---- build row as Vec<String> matching final_columns order --------

            let row_values: Vec<String> = final_columns_closure
                .iter()
                .map(|name| record.get(name).cloned().unwrap_or_default())
                .collect();

            Ok(row_values)
        })?;

        // 4. Write CSV ----------------------------------------------------------

        let file = File::create(Path::new(csv_path))?;
        let mut wtr = Writer::from_writer(file);

        // header
        wtr.write_record(&final_columns)?;

        // rows (skip rows for gases that weren't selected, just in case)
        for row_result in rows {
            let row_values = row_result?;

            if let Some(gas_idx) = gas_col_index {
                if let Some(gas_value) = row_values.get(gas_idx) {
                    if let Ok(gas_enum) = gas_value.parse::<GasType>() {
                        if !gas_checked.get(&gas_enum).copied().unwrap_or(false) {
                            continue;
                        }
                    }
                }
            }

            wtr.write_record(&row_values)?;
        }

        wtr.flush()?;
        Ok(())
    }
}
