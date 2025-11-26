use fluxrs_core::project::Project;
use rusqlite::{params, types::ValueRef, Connection, Result, Row};

#[derive(Default)]
pub struct TableApp {
    project: Option<Project>,
    table_names: Vec<String>,
    selected_table: Option<String>,
    column_names: Vec<String>,
    data: Vec<Vec<String>>,
    current_page: usize,
}

impl TableApp {
    fn fetch_table_names(&mut self, conn: &Connection) {
        let mut stmt = match conn.prepare(
            "SELECT name
                    FROM sqlite_master
                    WHERE type='table'
                    AND name NOT LIKE 'sqlite_%'
                    ORDER BY name",
        ) {
            Ok(stmt) => stmt,
            Err(err) => {
                eprintln!("Error preparing statement: {}", err);
                self.table_names.clear();
                return;
            },
        };

        let tables = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .and_then(|rows| rows.collect::<Result<Vec<String>, _>>());

        match tables {
            Ok(names) => self.table_names = names,
            Err(err) => {
                eprintln!("Error fetching table names: {}", err);
                self.table_names.clear();
            },
        }
    }
    fn fetch_table_data(&mut self, table_name: &str) {
        self.column_names.clear();
        self.data.clear();
        self.current_page = 0; // Reset page when switching tables

        let conn = Connection::open("fluxrs.db").expect("Failed to open database");

        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name)).unwrap();
        self.column_names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<String>, _>>()
            .unwrap_or_default();

        let mut index = None;
        let mut project_col = None;

        if table_name != "projects" && self.project.is_some() {
            project_col = Some("project_link");
        }

        if matches!(table_name, "fluxes" | "flux_history" | "cycles") {
            index = Some("start_time");
        }

        if matches!(table_name, "measurements" | "meteo" | "height") {
            index = Some("datetime");
        }

        // Build query correctly: WHERE first, then ORDER BY
        let mut query = format!("SELECT * FROM {}", table_name);

        if let Some(col) = project_col {
            query.push_str(&format!(
                " WHERE {} = '{}'",
                col,
                self.project.as_ref().unwrap().id.unwrap()
            ));
        }

        if let Some(col) = index {
            query.push_str(&format!(" ORDER BY {}", col));
        }
        let mut stmt = conn.prepare(&query).unwrap();
        let column_count = stmt.column_count();

        let rows = stmt.query_map([], |row: &Row| {
            let mut values = Vec::new();
            for i in 0..column_count {
                let value = match row.get_ref(i) {
                    Ok(ValueRef::Null) => "NULL".to_string(),
                    Ok(ValueRef::Integer(i)) => i.to_string(),
                    Ok(ValueRef::Real(f)) => f.to_string(),
                    Ok(ValueRef::Text(s)) => String::from_utf8_lossy(s).to_string(),
                    Ok(ValueRef::Blob(_)) => "[BLOB]".to_string(), // Handle BLOBs gracefully
                    Err(_) => "[ERROR]".to_string(),               //   Handle row errors explicitly
                };
                values.push(value);
            }
            Ok(values)
        });

        self.data = rows.unwrap().filter_map(|res| res.ok()).collect(); //   Collect valid rows only
    }

    pub fn table_ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context, project: Option<Project>) {
        self.project = project;
        ui.heading("Database Table Viewer");
        if self.table_names.is_empty() {
            let conn = Connection::open("fluxrs.db").expect("Failed to open database");
            self.fetch_table_names(&conn);
        }
        if self.selected_table == Some("measurements".to_owned()) {
            ui.label("Viewing measurements is disabled for now, too much data.");
            ui.label(
                "Need to implement selecting a time range because of the massive amount of data.",
            );
        }
        if !self.table_names.is_empty() {
            egui::ComboBox::from_label("Select a table")
                .selected_text(
                    self.selected_table.clone().unwrap_or_else(|| "Choose a table".to_string()),
                )
                .show_ui(ui, |ui| {
                    for table in &self.table_names.clone() {
                        if ui
                            .selectable_label(self.selected_table.as_deref() == Some(table), table)
                            .clicked()
                        {
                            self.selected_table = Some(table.clone());
                            if table == "measurements" {
                                return;
                            }
                            self.fetch_table_data(table);
                        }
                    }
                });
        } else {
            ui.label("No tables found in the database.");
        }

        ui.separator();

        if let Some(_selected) = &self.selected_table {
            // Determine which rows to display for pagination
            let rows_per_page = 100;
            let start_idx = self.current_page * rows_per_page;
            let end_idx = (start_idx + rows_per_page).min(self.data.len());
            ui.horizontal(|ui| {
                // Previous Page Button
                if self.current_page > 0 && ui.button("⬅ Previous").clicked() {
                    self.current_page -= 1;
                }

                ui.label(format!(
                    "Page {}/{}",
                    self.current_page + 1,
                    self.data.len().div_ceil(rows_per_page)
                ));

                // Next Page Button
                if end_idx < self.data.len() && ui.button("Next ➡").clicked() {
                    self.current_page += 1;
                }
            });
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("data_table").striped(true).show(ui, |ui| {
                    for col in &self.column_names {
                        ui.label(col); // show headers as-is
                    }
                    ui.end_row();
                    for row in &self.data[start_idx..end_idx] {
                        for (i, value) in row.iter().enumerate() {
                            let col_name = &self.column_names[i];
                            let display = if col_name == "datetime" || col_name == "start_time" {
                                if let Ok(ts) = value.parse::<i64>() {
                                    if let Some(dt_utc) = chrono::DateTime::from_timestamp(ts, 0) {
                                        let dt_local = dt_utc
                                            .with_timezone(&self.project.as_ref().unwrap().tz); // or &self.project.tz / &tz
                                        dt_local.format("%Y-%m-%d %H:%M:%S").to_string()
                                    } else {
                                        format!("Invalid timestamp: {}", ts)
                                    }
                                } else {
                                    format!("Invalid value: {}", value)
                                }
                            } else {
                                value.to_string()
                            };

                            ui.label(display);
                        }
                        ui.end_row();
                    }
                });
            });
        }
    }
}
