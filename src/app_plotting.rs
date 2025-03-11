use crate::index::Index;
pub use crate::instruments::GasType;
use crate::query::{insert_fluxes_ignore_duplicates, update_fluxes};
use crate::structs::{Cycle, ErrorCode};
use crate::validation_app::ValidationApp;
use crate::validation_app::{
    create_polygon, handle_drag_polygon, is_inside_polygon, limit_to_bounds,
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use egui::Align2;
use egui_file::FileDialog;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use std::ops::RangeInclusive;

use eframe::egui::{
    Color32, Id, Layout, PointerButton, Pos2, Rect, RichText, Sense, Ui, Vec2, Vec2b,
};
use egui_plot::{
    CoordinatesFormatter, Corner, GridInput, GridMark, MarkerShape, Plot, PlotBounds, PlotPoint,
    PlotPoints, PlotTransform, Points, Text, VLine,
};

impl ValidationApp {
    pub fn is_gas_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_gases.contains(gas_type)
    }
    pub fn is_flux_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_fluxes.contains(gas_type)
    }
    pub fn is_calc_r_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_calc_rs.contains(gas_type)
    }
    pub fn is_measurement_r_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_measurement_rs.contains(gas_type)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_gas_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        gas_type: GasType,
        lag_s: f64,
        calc_area_color: Color32,
        calc_area_stroke_color: Color32,
        calc_area_adjust_color: Color32,
        main_id: Id,
        left_id: Id,
        right_id: Id,
    ) {
        let left_polygon = create_polygon(
            self.get_calc_start(gas_type),
            self.get_calc_start(gas_type) + self.drag_panel_width,
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_adjust_color,
            calc_area_stroke_color,
            "Extend left",
            left_id,
        );

        let right_polygon = create_polygon(
            self.get_calc_end(gas_type) - self.drag_panel_width,
            self.get_calc_end(gas_type),
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_adjust_color,
            calc_area_stroke_color,
            "Extend right",
            right_id,
        );
        let main_polygon = create_polygon(
            self.get_calc_start(gas_type) + self.drag_panel_width,
            self.get_calc_end(gas_type) - self.drag_panel_width,
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_color,
            calc_area_stroke_color,
            "Move",
            main_id,
        );

        let adj_x_open: f64 = self.start_time_idx + self.open_offset + lag_s;
        let adj_x_close = self.start_time_idx + self.close_offset + lag_s;
        let adj_open_line = VLine::new(adj_x_open).name("Lagtime").width(2.0).allow_hover(true);
        let adj_close_line =
            VLine::new(adj_x_close).name("Close time").width(2.0).allow_hover(true);

        let x_open: f64 = self.start_time_idx + self.open_offset;
        let x_close = self.start_time_idx + self.close_offset;
        let open_line = VLine::new(x_open).name("Unadjusted open").width(2.0).allow_hover(true);
        let close_line = VLine::new(x_close).name("Unadjusted close").width(2.0).allow_hover(true);

        // plot_ui.polygon(main_polygon);
        // plot_ui.polygon(left_polygon);
        // plot_ui.polygon(right_polygon);
        if self.cycles[*self.index].is_valid {
            plot_ui.polygon(main_polygon);
            plot_ui.polygon(left_polygon);
            plot_ui.polygon(right_polygon);
        } else {
            let errors = ErrorCode::from_mask(self.cycles[*self.index].error_code.0);
            let error_messages: Vec<String> =
                errors.iter().map(|error| error.to_string()).collect();

            let msg = format!("{}", error_messages.join("\n"));
            let has_errors = Id::new(format!("haserrors{}", gas_type));

            plot_ui.text(
                Text::new(
                    PlotPoint::new(self.start_time_idx, self.get_max_y(&gas_type)),
                    RichText::new(msg).size(20.),
                )
                .highlight(true)
                .anchor(Align2::LEFT_TOP)
                .color(Color32::from_rgba_unmultiplied(250, 128, 128, 255))
                .id(has_errors),
            );
        }
        if let Some(data) = self.cycles[self.index.count].gas_v.get(&gas_type) {
            let dt_v = self.cycles[self.index.count].dt_v_as_float();
            let diag_values = &self.cycles[self.index.count].diag_v; // Assuming `diag` is a Vec<f64>

            let mut normal_points = Vec::new();
            let mut highlighted_points = Vec::new();

            for ((x, y), &diag) in
                dt_v.iter().copied().zip(data.iter().copied()).zip(diag_values.iter())
            {
                if diag != 0 {
                    highlighted_points.push([x, y]); // Store points with diag != 0 separately
                } else {
                    normal_points.push([x, y]); // Store normal points
                }
            }

            // Plot normal points
            if !normal_points.is_empty() {
                plot_ui.points(
                    Points::new(PlotPoints::from(normal_points))
                        .name(format!("{}", gas_type))
                        .shape(MarkerShape::Circle)
                        .color(gas_type.color()) // Default color
                        .radius(2.),
                );
            }

            // Plot highlighted points (different color)
            if !highlighted_points.is_empty() {
                plot_ui.points(
                    Points::new(PlotPoints::from(highlighted_points))
                        .name(format!("{} (Error)", gas_type))
                        .shape(MarkerShape::Circle)
                        .color(egui::Color32::RED) // Use red for errors
                        .radius(3.), // Slightly bigger for visibility
                );
            }

            plot_ui.vline(adj_open_line);
            plot_ui.vline(adj_close_line);
            plot_ui.vline(open_line);
            plot_ui.vline(close_line);
        } else {
            let half_way_x = self.start_time_idx + ((self.end_time_idx - self.start_time_idx) / 2.);
            let bad_plot = Id::new(format!("bad_plot {}", gas_type));
            plot_ui.text(
                Text::new(PlotPoint::new(half_way_x, 0), RichText::new("No data points").size(20.))
                    .id(bad_plot),
            );
        }
    }

    pub fn get_min_y(&self, gas_type: &GasType) -> f64 {
        *self.cycles[*self.index].min_y.get(gas_type).unwrap_or(&0.0)
    }
    pub fn get_max_y(&self, gas_type: &GasType) -> f64 {
        *self.cycles[*self.index].max_y.get(gas_type).unwrap_or(&0.0)
    }

    pub fn get_measurement_min_y(&self, gas_type: &GasType) -> f64 {
        *self.measurement_min_y.get(gas_type).unwrap_or(&0.0)
    }
    pub fn get_measurement_max_y(&self, gas_type: &GasType) -> f64 {
        *self.measurement_max_y.get(gas_type).unwrap_or(&0.0)
    }
    pub fn update_current_cycle(&mut self) {
        let proj = self.selected_project.as_ref().unwrap().clone();
        self.cycles[*self.index].update_cycle(proj);
        self.cycles[*self.index].manual_adjusted = true;
        let mut conn = Connection::open("fluxrs.db").unwrap();
        // let proj = self.selected_project.as_ref().unwrap().clone();

        // update_fluxes(&mut conn, &[self.cycles[*self.index].clone()], proj);

        match update_fluxes(
            &mut conn,
            &[self.cycles[*self.index].clone()],
            self.selected_project.as_ref().unwrap().clone(),
        ) {
            Ok(_) => println!("Fluxes inserted successfully!"),
            Err(e) => eprintln!("Error inserting fluxes: {}", e),
        }
    }
    // pub fn calculate_min_y(&mut self) {
    //     let cycle = &self.cycles[self.index.count];
    //     self.min_y.clear(); // Clear previous data
    //
    //     for (gas_type, gas_v) in &cycle.gas_v {
    //         let min_value =
    //             gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min);
    //
    //         self.min_y.insert(*gas_type, min_value);
    //     }
    // }
    // pub fn calculate_max_y(&mut self) {
    //     let cycle = &self.cycles[self.index.count];
    //     self.max_y.clear(); // Clear previous data
    //
    //     for (gas_type, gas_v) in &cycle.gas_v {
    //         let min_value =
    //             gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max);
    //
    //         self.max_y.insert(*gas_type, min_value);
    //     }
    // }
    pub fn calculate_measurement_max_y(&mut self) {
        let cycle = &self.cycles[self.index.count];
        self.measurement_max_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &cycle.measurement_gas_v {
            let min_value =
                gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::NEG_INFINITY, f64::max);

            self.measurement_max_y.insert(*gas_type, min_value);
        }
    }
    pub fn calculate_measurement_min_y(&mut self) {
        let cycle = &self.cycles[self.index.count];
        self.measurement_min_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &cycle.measurement_gas_v {
            let min_value =
                gas_v.iter().copied().filter(|v| !v.is_nan()).fold(f64::INFINITY, f64::min);

            self.measurement_min_y.insert(*gas_type, min_value);
        }
    }

    pub fn update_plots(&mut self) {
        println!("Update cycle");
        let index = self.index.count;
        // let cycle = self.get_cycle();
        // let cycle = &self.cycles[index];
        self.calculate_measurement_min_y();
        self.calculate_measurement_max_y();
        self.all_traces = self.cycles.iter().map(|cycle| cycle.chamber_id.clone()).collect();
        for chamber_id in &self.all_traces {
            self.chamber_colors
                .entry(chamber_id.clone())
                .or_insert_with(|| generate_color(chamber_id));
        }
        self.get_visible_indexes();

        self.lag_idx = self.cycles[index].open_time.timestamp() as f64 + self.cycles[index].lag_s;
        self.close_idx =
            self.cycles[index].close_time.timestamp() as f64 + self.cycles[index].lag_s;
        self.open_idx = self.cycles[index].start_time.timestamp() as f64
            + self.cycles[index].open_offset as f64
            + self.cycles[index].lag_s;
        self.open_offset = self.cycles[index].open_offset as f64;
        self.close_offset = self.cycles[index].close_offset as f64;
        self.start_time_idx = self.cycles[index].start_time.timestamp() as f64;
        self.end_time_idx = self.cycles[index].end_time.timestamp() as f64;
        self.calc_range_end = self.cycles[index].calc_range_end.clone();
        self.calc_range_start = self.cycles[index].calc_range_start.clone();
        self.calc_r = self.cycles[index].calc_r.clone();
        self.measurement_r = self.cycles[index].measurement_r.clone();
        self.flux = self.cycles[index].flux.clone();
        self.gases = self.cycles[index].gases.clone();
        self.cycles[index].check_errors();
        // self.min_y = cycle
        //     .gas_v
        //     .iter()
        //     .copied()
        //     .filter(|v| !v.is_nan())
        //     .fold(f64::INFINITY, f64::min);
        // self.max_y = cycle
        //     .gas_v
        //     .iter()
        //     .copied()
        //     .filter(|v| !v.is_nan())
        //     .fold(f64::NEG_INFINITY, f64::max);
        // self.lag_vec = self.cycles.iter().map(|x| x.lag_s).collect();
        // self.start_vec = self
        //     .cycles
        //     .iter()
        //     .map(|x| x.start_time.timestamp() as f64)
        //     .collect();
        // self.lag_plot = self
        //     .start_vec
        //     .iter()
        //     .copied() // Copy each f64 from the iterator
        //     .zip(self.lag_vec.iter().copied()) // Iterate and copy gas_v
        //     .map(|(x, y)| [x, y]) // Convert each tuple into an array
        //     .collect();
        // self.flux_plot = self
        //     .start_vec
        //     .iter()
        //     .copied() // Copy each f64 from the iterator
        //     .zip(self.lag_vec.iter().copied()) // Iterate and copy gas_v
        //     .map(|(x, y)| [x, y]) // Convert each tuple into an array
        //     .collect();
        // self.flux_plot = for (timestamp, flux_map) in self.start_vec.iter().zip(self.flux.iter()) {
        //     for (&gas, &value) in flux_map.iter() {
        //         self.flux_plot
        //             .entry(gas)
        //             .or_insert_with(Vec::new)
        //             .push([*timestamp, value]); // Store [timestamp, flux_value]
        //     }
        // }
    }

    pub fn get_visible_indexes(&mut self) {
        self.visible_cycles.clear(); // Reset previous indexes

        for (index, cycle) in self.cycles.iter().enumerate() {
            let chamber_id = &cycle.chamber_id;

            if !self.show_valids && cycle.is_valid {
                continue;
            }
            if !self.show_invalids && !cycle.is_valid {
                continue;
            }
            // Check if chamber is visible
            if self.visible_traces.get(chamber_id).copied().unwrap_or(true) {
                self.visible_cycles.push(index); // Store index
            }
        }
    }
    // pub fn get_visible_indexes(&mut self) {
    //     self.cycles
    //         .iter()
    //         .enumerate()
    //         .filter_map(|(index, cycle)| {
    //             let chamber_id = &cycle.chamber_id;
    //
    //             // Check if chamber is visible
    //             if let visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true) {
    //                 Some(index) // Keep the index
    //             } else {
    //                 None // Exclude this index
    //             }
    //         })
    //         .collect();
    // }

    pub fn create_traces<F>(
        &mut self,
        gas_type: &GasType,
        selector: F,
    ) -> (HashMap<String, Vec<[f64; 2]>>, HashMap<String, Vec<[f64; 2]>>)
    where
        F: Fn(&Cycle, &GasType) -> f64, // Selector function with gas_type
    {
        let mut valid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
        let mut invalid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        for &index in &self.visible_cycles {
            let cycle = &self.cycles[index]; // Get cycle by precomputed index
            let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
            let value = selector(cycle, gas_type); // Extract value using the selector
            let start_time = cycle.start_time.timestamp() as f64; // Get timestamp

            if cycle.is_valid {
                valid_traces.entry(chamber_id).or_insert_with(Vec::new).push([start_time, value]);
            } else {
                invalid_traces.entry(chamber_id).or_insert_with(Vec::new).push([start_time, value]);
            }
        }

        (valid_traces, invalid_traces)
    }
    // pub fn create_traces<F>(
    //     &mut self,
    //     gas_type: &GasType,
    //     selector: F,
    // ) -> HashMap<String, Vec<[f64; 2]>>
    // where
    //     F: Fn(&Cycle, &GasType) -> f64, // Selector function with gas_type
    // {
    //     let mut trace_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
    //
    //     for &index in &self.visible_cycles {
    //         let cycle = &self.cycles[index]; // Get cycle by precomputed index
    //         let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
    //         let value = selector(cycle, gas_type); // Use the selector function with gas_type
    //         let start_time = cycle.start_time.timestamp() as f64; // Get timestamp
    //
    //         trace_map.entry(chamber_id).or_insert_with(Vec::new).push([start_time, value]);
    //         // Append the point
    //     }
    //
    //     trace_map
    // }
    // pub fn create_traces<F>(&mut self, selector: F) -> HashMap<String, Vec<[f64; 2]>>
    // where
    //     F: Fn(&Cycle) -> f64, // Selector function extracts the desired float value
    // {
    //     let mut trace_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
    //
    //     for &index in &self.visible_cycles {
    //         let cycle = &self.cycles[index]; // Get cycle by precomputed index
    //         let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
    //         let value = selector(cycle, gas_type); // Use the selector function to get the desired value
    //         let start_time = cycle.start_time.timestamp() as f64; // Get timestamp
    //
    //         trace_map
    //             .entry(chamber_id)
    //             .or_insert_with(Vec::new)
    //             .push([start_time, value]); // Append the point
    //     }
    //
    //     trace_map
    // }

    pub fn create_lag_traces(
        &mut self,
    ) -> (HashMap<String, Vec<[f64; 2]>>, HashMap<String, Vec<[f64; 2]>>) {
        let mut valid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
        let mut invalid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        for &index in &self.visible_cycles {
            let cycle = &self.cycles[index]; // Get cycle by precomputed index
            let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
            let is_valid = cycle.is_valid; // Validity per point
            let lag_value = cycle.lag_s; // Get lag value
            let start_time = cycle.start_time.timestamp() as f64; // Get timestamp

            // ✅ Store valid and invalid points separately
            if is_valid {
                valid_traces
                    .entry(chamber_id.clone())
                    .or_insert_with(Vec::new)
                    .push([start_time, lag_value]);
            } else {
                invalid_traces
                    .entry(chamber_id.clone())
                    .or_insert_with(Vec::new)
                    .push([start_time, lag_value]);
            }
        }

        (valid_traces, invalid_traces)
    }
    // pub fn create_lag_traces(&mut self) -> HashMap<String, Vec<[f64; 2]>> {
    //     let mut flux_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
    //
    //     for &index in &self.visible_cycles {
    //         let cycle = &self.cycles[index]; // Get cycle by precomputed index
    //         let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
    //
    //         // Apply additional filters based on validity settings
    //
    //         let lag_value = cycle.lag_s; // Get lag value
    //         let start_time = cycle.start_time.timestamp() as f64; // Get timestamp
    //
    //         flux_map
    //             .entry(chamber_id)
    //             .or_insert_with(Vec::new)
    //             .push([start_time, lag_value]);
    //     }
    //
    //     flux_map
    // }
    // pub fn create_lag_traces(&mut self) -> HashMap<String, Vec<[f64; 2]>> {
    //     let mut flux_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
    //
    //     for cycle in &self.cycles {
    //         let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
    //
    //         if self
    //             .visible_traces
    //             .get(&chamber_id)
    //             .cloned()
    //             .unwrap_or(true)
    //         {
    //             if !self.show_valids && cycle.is_valid {
    //                 continue;
    //             }
    //             if !self.show_invalids && !cycle.is_valid {
    //                 continue;
    //             }
    //             let lag_value = cycle.lag_s; // Get flux value
    //             let start_time = cycle.start_time.timestamp() as f64; // Get timestamp
    //
    //             flux_map
    //                 .entry(chamber_id)
    //                 .or_insert_with(Vec::new)
    //                 .push([start_time, lag_value]);
    //         }
    //     }
    //
    //     flux_map
    // }

    pub fn get_calc_end(&self, gas_type: GasType) -> f64 {
        *self.cycles[self.index.count].calc_range_end.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn get_calc_start(&self, gas_type: GasType) -> f64 {
        *self.cycles[self.index.count].calc_range_start.get(&gas_type).unwrap_or(&0.0)
    }
    pub fn set_calc_start(&mut self, gas_type: GasType, x: f64) {
        self.cycles[self.index.count].calc_range_start.insert(gas_type, x);
    }
    pub fn set_calc_end(&mut self, gas_type: GasType, x: f64) {
        self.cycles[self.index.count].calc_range_end.insert(gas_type, x);
    }
    pub fn decrement_calc_start(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count].calc_range_start.get(&gas_type).unwrap_or(&0.0);
        let x = s - x;
        self.cycles[self.index.count].calc_range_start.insert(gas_type, x);
    }
    pub fn increment_calc_start(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count].calc_range_start.get(&gas_type).unwrap_or(&0.0);
        let x = s + x;
        self.cycles[self.index.count].calc_range_start.insert(gas_type, x);
    }
    pub fn increment_calc_end(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count].calc_range_end.get(&gas_type).unwrap_or(&0.0);
        let x = s + x;
        self.cycles[self.index.count].calc_range_end.insert(gas_type, x);
    }
    pub fn decrement_calc_end(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count].calc_range_end.get(&gas_type).unwrap_or(&0.0);
        let x = s - x;
        self.cycles[self.index.count].calc_range_end.insert(gas_type, x);
    }
    // pub fn new() -> Self {
    //     Self {
    //         cycles: Vec::new(),
    //         gases: Vec::new(),
    //         end_date: NaiveDate::from_ymd_opt(2025, 1, 14)
    //             .unwrap()
    //             .and_hms_opt(0, 0, 0)
    //             .unwrap()
    //             .and_utc(),
    //         // end_date: Utc::now(),
    //         start_date: Utc::now() - chrono::TimeDelta::weeks(1),
    //         flux_traces: HashMap::new(),
    //         lag_traces: HashMap::new(),
    //         chamber_ids: Vec::new(),
    //         lag_plot_w: 600.,
    //         lag_plot_h: 350.,
    //         gas_plot_w: 600.,
    //         gas_plot_h: 350.,
    //         flux_plot_w: 600.,
    //         flux_plot_h: 350.,
    //         all_traces: HashSet::new(),
    //         visible_traces: HashMap::new(),
    //         visible_cycles: Vec::new(),
    //         selected_point: None,
    //         dragged_point: None,
    //         r_lim: 1.,
    //         chamber_colors: HashMap::new(),
    //         enabled_gases: HashSet::from([GasType::CH4, GasType::CO2]),
    //         enabled_fluxes: HashSet::from([GasType::CH4, GasType::CO2]),
    //         calc_area_color: Color32::default(),
    //         calc_area_adjust_color: Color32::default(),
    //         calc_area_stroke_color: Color32::default(),
    //         drag_panel_width: 40.,
    //         lag_idx: 0.,
    //         close_idx: 0.,
    //         open_idx: 0.,
    //         open_offset: 0.,
    //         close_offset: 0.,
    //         start_time_idx: 0.,
    //         end_time_idx: 0.,
    //         calc_range_end: HashMap::new(),
    //         calc_range_start: HashMap::new(),
    //         max_y: HashMap::new(),
    //         min_y: HashMap::new(),
    //         calc_r: HashMap::new(),
    //         measurement_r: HashMap::new(),
    //         main_gas: GasType::CH4,
    //         chamber_id: String::new(),
    //         is_valid: true,
    //         manual_valid: false,
    //         override_valid: None,
    //         flux: HashMap::new(),
    //         measurement_max_y: HashMap::new(),
    //         measurement_min_y: HashMap::new(),
    //         min_calc_area_range: 240.,
    //         index: Index::default(),
    //         lag_vec: Vec::new(),
    //         start_vec: Vec::new(),
    //         lag_plot: Vec::new(),
    //         opened_files: None,
    //         open_file_dialog: None,
    //         initial_path: Some(PathBuf::from(".")),
    //         selected_data_type: None,
    //         log_messages: Vec::new(),
    //         show_valids: true,
    //         show_invalids: true,
    //         zoom_to_measurement: false,
    //     }
    // }
    // pub fn new(mut cycles: Vec<Cycle>) -> Self {
    //     let cycle = &mut cycles[0];
    //     let lag_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
    //     let close_idx = cycle.close_time.timestamp() as f64 + cycle.lag_s;
    //     let open_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
    //     let open_offset = cycle.open_offset as f64;
    //     let close_offset = cycle.close_offset as f64;
    //     let start_time_idx = cycle.start_time.timestamp() as f64;
    //     let end_time_idx = cycle.end_time.timestamp() as f64;
    //     let calc_range_end = cycle.calc_range_end.clone();
    //     let calc_range_start = cycle.calc_range_start.clone();
    //     let min_y = cycle.min_y.clone();
    //     let max_y = cycle.max_y.clone();
    //     let gas_plot = cycle.gas_plot.clone();
    //     let min_calc_area_range = 240.;
    //     let lag_vec: Vec<f64> = cycles.iter().map(|x| x.lag_s).collect();
    //     let start_vec: Vec<f64> = cycles
    //         .iter()
    //         .map(|x| x.start_time.timestamp() as f64)
    //         .collect();
    //     let lag_plot: Vec<[f64; 2]> = start_vec
    //         .iter()
    //         .copied() // Copy each f64 from the iterator
    //         .zip(lag_vec.iter().copied()) // Iterate and copy gas_v
    //         .map(|(x, y)| [x, y]) // Convert each tuple into an array
    //         .collect();
    //     let index = Index::default();
    //     let drag_panel_width = 40.0;
    //     let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
    //     let calc_area_adjust_color = Color32::from_rgba_unmultiplied(64, 242, 106, 50);
    //     let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
    //     let selected_point = None;
    //     let dragged_point = None;
    //     let gases = Vec::new();
    //     Self {
    //         cycles,
    //         gases,
    //         start_date: String::new(),
    //         end_date: String::new(),
    //         visible_traces: HashMap::new(),
    //         selected_point,
    //         dragged_point,
    //         r_lim: 1.,
    //         chamber_colors: HashMap::new(),
    //         enabled_gases: HashSet::from([GasType::CH4, GasType::CO2]),
    //         enabled_fluxes: HashSet::from([GasType::CH4, GasType::CO2]),
    //         gas_plot,
    //         calc_area_color,
    //         calc_area_adjust_color,
    //         calc_area_stroke_color,
    //         drag_panel_width,
    //         lag_idx,
    //         close_idx,
    //         open_idx,
    //         open_offset,
    //         close_offset,
    //         start_time_idx,
    //         end_time_idx,
    //         calc_range_end,
    //         calc_range_start,
    //         max_y,
    //         min_y,
    //         min_calc_area_range,
    //         index,
    //         lag_vec,
    //         start_vec,
    //         lag_plot,
    //     }
    // }

    pub fn render_attribute_plot<F>(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        gas_type: &GasType,
        selector: F,
        plot_name: &str,
    ) where
        F: Fn(&Cycle, &GasType) -> f64, // Selector function for extracting data
    {
        let (valid_traces, invalid_traces) = self.create_traces(gas_type, selector);
        let mut hovered_point: Option<[f64; 2]> = None;

        // Sort chamber IDs for consistent rendering
        let mut chamber_ids: Vec<&String> =
            valid_traces.keys().chain(invalid_traces.keys()).collect();
        chamber_ids.sort();

        for chamber_id in chamber_ids {
            let color = self
                .chamber_colors
                .entry(chamber_id.clone())
                .or_insert_with(|| generate_color(chamber_id));

            // **Render Valid Traces**
            if let Some(points) = valid_traces.get(chamber_id) {
                let plot_points = PlotPoints::from(points.clone());

                plot_ui.points(
                    Points::new(plot_points)
                        .name(format!("{} {}", plot_name, chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
                        .color(*color), // Normal color for valid points
                );
            }

            // **Render Invalid Traces (e.g., different color or shape)**
            if let Some(points) = invalid_traces.get(chamber_id) {
                let plot_points = PlotPoints::from(points.clone());

                plot_ui.points(
                    Points::new(plot_points)
                    .name(format!("{} {} (Invalid)", plot_name, chamber_id))
                    .shape(MarkerShape::Cross) // Different shape for invalid points
                    .radius(3.)
                    .color(*color), // Highlight invalid points in red
                );
            }
        }

        // **Handle hovering logic (consider both valid & invalid traces)**
        let all_traces = self.merge_traces(valid_traces.clone(), invalid_traces.clone());
        // let all_traces: HashMap<String, Vec<[f64; 2]>> =
        // valid_traces.into_iter().chain(invalid_traces).collect();

        let transform = plot_ui.transform();
        if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
            hovered_point = find_closest_point_screen_space(
                transform,
                Some(cursor_screen_pos),
                &all_traces,
                80.0,
            );
        }

        if plot_ui.response().clicked() {
            if let Some(closest) = hovered_point {
                let x_coord = closest[0];

                // Find the newest y-coordinate for this x
                if let Some(new_y) =
                    all_traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
                {
                    self.selected_point = Some([x_coord, new_y]);
                }

                // **Update index when clicking on a new measurement**
                for (i, c) in self.cycles.iter().enumerate() {
                    if c.start_time.timestamp() as f64 == x_coord {
                        self.index.set(i);
                    }
                }
                self.update_plots();
            }
        }

        // **Force `selected_point` to update whenever `index` changes**
        if let Some(current_cycle) = self.cycles.get(self.index.count) {
            let x_coord = current_cycle.start_time.timestamp() as f64;

            if let Some(new_y) =
                all_traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
            {
                self.selected_point = Some([x_coord, new_y]); // Keep x, update y
            }
        }

        // Draw updated selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new(PlotPoints::from(vec![selected]))
                    .name("Current")
                    .shape(MarkerShape::Circle)
                    .radius(5.0)
                    .filled(false)
                    .color(egui::Color32::RED),
            );
        }

        // Draw hovered point (if not the selected point)
        if let Some(hovered) = hovered_point {
            if Some(hovered) != self.selected_point {
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![hovered]))
                        .name("Closest")
                        .shape(MarkerShape::Circle)
                        .radius(5.0)
                        .filled(false)
                        .color(egui::Color32::GREEN),
                );
            }
        }
    }
    // pub fn render_attribute_plot<F>(
    //     &mut self,
    //     plot_ui: &mut egui_plot::PlotUi,
    //     gas_type: &GasType,
    //     selector: F,
    //     plot_name: &str,
    // ) where
    //     F: Fn(&Cycle, &GasType) -> f64, // Selector function for extracting data
    // {
    //     let traces = self.create_traces(gas_type, selector);
    //     let mut hovered_point: Option<[f64; 2]> = None;
    //
    //     // Sort chamber IDs for consistent rendering
    //     let mut chamber_ids: Vec<&String> = traces.keys().collect();
    //     chamber_ids.sort();
    //
    //     for chamber_id in chamber_ids {
    //         if let Some(points) = traces.get(chamber_id) {
    //             let color = self
    //                 .chamber_colors
    //                 .entry(chamber_id.clone())
    //                 .or_insert_with(|| generate_color(chamber_id));
    //
    //             let plot_points = PlotPoints::from(points.clone());
    //
    //             plot_ui.points(
    //                 Points::new(plot_points)
    //                     .name(format!("{} {}", plot_name, chamber_id))
    //                     .shape(MarkerShape::Circle)
    //                     .radius(2.)
    //                     .color(*color),
    //             );
    //         }
    //     }
    //
    //     // **Handle hovering logic**
    //     if let transform = plot_ui.transform() {
    //         if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
    //             hovered_point = find_closest_point_screen_space(
    //                 &transform,
    //                 Some(cursor_screen_pos),
    //                 &traces,
    //                 80.0,
    //             );
    //         }
    //     }
    //
    //     if plot_ui.response().clicked() {
    //         if let Some(closest) = hovered_point {
    //             let x_coord = closest[0];
    //
    //             // Find the newest y-coordinate (flux value) for this x
    //             if let Some(new_y) =
    //                 traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
    //             {
    //                 self.selected_point = Some([x_coord, new_y]);
    //             }
    //
    //             // **Update index when clicking on a new measurement**
    //             for (i, c) in self.cycles.iter().enumerate() {
    //                 if c.start_time.timestamp() as f64 == x_coord {
    //                     self.index.set(i);
    //                 }
    //             }
    //             self.update_cycle();
    //         }
    //     }
    //
    //     // **Force `selected_point` to update whenever `index` changes**
    //     if let Some(current_cycle) = self.cycles.get(self.index.count) {
    //         let x_coord = current_cycle.start_time.timestamp() as f64;
    //
    //         if let Some(new_y) =
    //             traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
    //         {
    //             self.selected_point = Some([x_coord, new_y]); // Keep x, update y
    //         }
    //     }
    //
    //     // Draw updated selected point
    //     if let Some(selected) = self.selected_point {
    //         plot_ui.points(
    //             Points::new(PlotPoints::from(vec![selected]))
    //                 .name("Current")
    //                 .shape(MarkerShape::Circle)
    //                 .radius(5.0)
    //                 .filled(false)
    //                 .color(egui::Color32::RED),
    //         );
    //     }
    //
    //     // Draw hovered point (if not the selected point)
    //     if let Some(hovered) = hovered_point {
    //         if Some(hovered) != self.selected_point {
    //             plot_ui.points(
    //                 Points::new(PlotPoints::from(vec![hovered]))
    //                     .name("Closest")
    //                     .shape(MarkerShape::Circle)
    //                     .radius(5.0)
    //                     .filled(false)
    //                     .color(egui::Color32::GREEN),
    //             );
    //         }
    //     }
    // }
    // pub fn render_flux_plot(&mut self, plot_ui: &mut egui_plot::PlotUi, gas_type: GasType) {
    //     // let flux_traces = self.create_traces(&gas_type,|self.cycles[*self.index], &gas_type| *cycle.flux.get(gas_type).unwrap_or(&0.0));
    //     let flux_traces = self.create_traces(&gas_type, |cycle, gas_type| {
    //         *cycle.flux.get(gas_type).unwrap_or(&0.0)
    //     });
    //     let mut hovered_point: Option<[f64; 2]> = None;
    //
    //     // Sort chamber IDs for consistent rendering
    //     let mut chamber_ids: Vec<&String> = flux_traces.keys().collect();
    //     chamber_ids.sort();
    //
    //     for chamber_id in chamber_ids {
    //         if let Some(lag_points) = flux_traces.get(chamber_id) {
    //             let color = self
    //                 .chamber_colors
    //                 .entry(chamber_id.clone())
    //                 .or_insert_with(|| generate_color(chamber_id));
    //
    //             let plot_points = PlotPoints::from(lag_points.clone());
    //
    //             plot_ui.points(
    //                 Points::new(plot_points)
    //                     .name(format!("ID {}", chamber_id))
    //                     .shape(MarkerShape::Circle)
    //                     .radius(2.)
    //                     .color(*color),
    //             );
    //         }
    //     }
    //
    //     // **Ensure selected_point updates properly when clicking OR changing index**
    //     if let transform = plot_ui.transform() {
    //         if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
    //             hovered_point = find_closest_point_screen_space(
    //                 &transform,
    //                 Some(cursor_screen_pos),
    //                 &flux_traces,
    //                 80.0,
    //             );
    //         }
    //     }
    //
    //     if plot_ui.response().clicked() {
    //         if let Some(closest) = hovered_point {
    //             let x_coord = closest[0];
    //
    //             // Find the newest y-coordinate (flux value) for this x
    //             if let Some(new_y) = flux_traces
    //                 .values()
    //                 .flatten()
    //                 .filter(|p| p[0] == x_coord)
    //                 .map(|p| p[1])
    //                 .last()
    //             {
    //                 self.selected_point = Some([x_coord, new_y]);
    //             }
    //
    //             // **Update index when clicking on a new measurement**
    //             for (i, c) in self.cycles.iter().enumerate() {
    //                 if c.start_time.timestamp() as f64 == x_coord {
    //                     self.index.set(i);
    //                 }
    //             }
    //             self.update_cycle();
    //         }
    //     }
    //
    //     // **Force `selected_point` to update whenever `index` changes**
    //     if let Some(current_cycle) = self.cycles.get(self.index.count) {
    //         let x_coord = current_cycle.start_time.timestamp() as f64;
    //
    //         if let Some(new_y) = flux_traces
    //             .values()
    //             .flatten()
    //             .filter(|p| p[0] == x_coord)
    //             .map(|p| p[1])
    //             .last()
    //         {
    //             self.selected_point = Some([x_coord, new_y]); // Keep x, update y
    //         }
    //     }
    //
    //     // Draw updated selected point
    //     if let Some(selected) = self.selected_point {
    //         plot_ui.points(
    //             Points::new(PlotPoints::from(vec![selected]))
    //                 .name("Current")
    //                 .shape(MarkerShape::Circle)
    //                 .radius(5.0)
    //                 .filled(false)
    //                 .color(egui::Color32::RED),
    //         );
    //     }
    //
    //     // Draw hovered point (if not the selected point)
    //     if let Some(hovered) = hovered_point {
    //         if Some(hovered) != self.selected_point {
    //             plot_ui.points(
    //                 Points::new(PlotPoints::from(vec![hovered]))
    //                     .name("Closest")
    //                     .shape(MarkerShape::Circle)
    //                     .radius(5.0)
    //                     .filled(false)
    //                     .color(egui::Color32::GREEN),
    //             );
    //         }
    //     }
    // }
    fn merge_traces(
        &self,
        valid_traces: HashMap<String, Vec<[f64; 2]>>,
        invalid_traces: HashMap<String, Vec<[f64; 2]>>,
    ) -> HashMap<String, Vec<[f64; 2]>> {
        let mut merged_traces = HashMap::new();

        // ✅ Insert all valid traces
        for (key, points) in valid_traces {
            merged_traces.entry(key).or_insert_with(Vec::new).extend(points);
        }

        // ✅ Insert all invalid traces (merge if key already exists)
        for (key, points) in invalid_traces {
            merged_traces.entry(key).or_insert_with(Vec::new).extend(points);
        }

        merged_traces
    }
    pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
        let main_gas = self.main_gas.unwrap();
        let (valid_traces, invalid_traces) =
            self.create_traces(&main_gas, |cycle, gas_type| cycle.lag_s);
        let mut hovered_point: Option<[f64; 2]> = None;
        let lag_traces = self.merge_traces(valid_traces.clone(), invalid_traces.clone());

        let mut chamber_ids: Vec<&String> = lag_traces.keys().collect();
        chamber_ids.sort();

        fn generate_color(seed: &str) -> Color32 {
            let hash = fxhash::hash(seed) as u32;
            let r = ((hash >> 24) & 255) as u8;
            let g = ((hash >> 8) & 255) as u8;
            let b = (hash & 255) as u8;
            Color32::from_rgb(r, g, b)
        }

        for chamber_id in chamber_ids {
            // if let Some(lag_points) = lag_traces.clone().get_mut(chamber_id) {
            let color = self
                .chamber_colors
                .entry(chamber_id.clone())
                .or_insert_with(|| generate_color(chamber_id));
            if let Some(valid_points) = valid_traces.get(chamber_id) {
                plot_ui.points(
                    Points::new(PlotPoints::from(valid_points.clone()))
                        .name(format!("Flux Chamber {} (Valid)", chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
                        .color(*color),
                );
            }
            if let Some(invalid_points) = invalid_traces.get(chamber_id) {
                plot_ui.points(
                    Points::new(PlotPoints::from(invalid_points.clone()))
                        .name(format!("Flux Chamber {} (Invalid)", chamber_id))
                        .shape(MarkerShape::Cross) // Different shape for invalid points
                        .radius(3.) // Slightly larger to highlight
                        .color(*color),
                );
            }
        }

        if let transform = plot_ui.transform() {
            if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
                // Only find hovered point if not dragging
                if self.dragged_point.is_none() {
                    hovered_point = find_closest_point_screen_space(
                        &transform,
                        Some(cursor_screen_pos),
                        &lag_traces,
                        80.0,
                    );
                }
            }
        }

        let response = plot_ui.response();

        // **Dragging Mechanism**
        if response.drag_started_by(egui::PointerButton::Primary) {
            // Lock onto the hovered point for dragging
            if let Some(hovered) = hovered_point {
                self.dragged_point = Some(hovered);
                // self.selected_point = self.dragged_point;
                self.update_plots();
            }
        }

        // if let Some(dragged) = self.dragged_point {
        //     if response.dragged_by(egui::PointerButton::Primary) {
        //         let delta = response.drag_delta(); // Mouse movement in screen space
        //
        //         if let transform = plot_ui.transform() {
        //             // ✅ Convert screen-space delta to plot-space delta
        //             let plot_delta = transform.inverse().transform_vec2(delta.into());
        //
        //             let new_y = dragged[1] - plot_delta.y as f64; // ✅ Adjust using converted delta
        //
        //             self.dragged_point = Some([dragged[0], new_y]);
        //
        //             for cycle in &mut self.cycles {
        //                 if cycle.start_time.timestamp() as f64 == dragged[0] {
        //                     cycle.lag_s = new_y;
        //                     self.close_idx = self.start_time_idx + self.close_offset + new_y;
        //                     self.open_idx = self.start_time_idx + self.open_offset + new_y;
        //                     cycle.recalc_r();
        //                     cycle.change_measurement_range();
        //                 }
        //             }
        //         }
        //     }
        // }

        if let Some(dragged) = self.dragged_point {
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta(); // Mouse movement in screen space

                if let transform = plot_ui.transform() {
                    let scale_factors = transform.dvalue_dpos(); // ✅ Get scale factor for UI → Plot conversion

                    let plot_dy = delta.y as f64 * scale_factors[1]; // ✅ Correct scaling for Y-axis

                    let new_y = dragged[1] + plot_dy; // ✅ Apply correct scaled movement

                    self.dragged_point = Some([dragged[0], new_y]);

                    for cycle in &mut self.cycles {
                        if cycle.start_time.timestamp() as f64 == dragged[0] {
                            cycle.lag_s = new_y;
                            self.close_idx = self.start_time_idx + self.close_offset + new_y;
                            self.open_idx = self.start_time_idx + self.open_offset + new_y;
                            // cycle.update_cycle();

                            // cycle.recalc_r();
                            // cycle.change_measurement_range();
                        }
                    }
                    self.update_plots();
                    self.update_current_cycle();
                }
            }
        }
        // if let Some(dragged) = self.dragged_point {
        //     if response.dragged_by(egui::PointerButton::Primary) {
        //         let delta = response.drag_delta();
        //
        //         if let transform = plot_ui.transform() {
        //             let plot_delta = transform.dvalue_dpos();
        //             println!("{:?}", transform.dvalue_dpos());
        //             println!("{:?}", transform.dpos_dvalue());
        //             // transform.position_from_point(PlotPoint::new())
        //
        //             // transform.position_from_point(&PlotPoint::new(point[0], point[1]));
        //
        //             // NOT THIS
        //             // let new_y = dragged[1] + transform.dpos_dvalue_y(); // ✅ Corrected scaling
        //
        //             // transform.value_from_position(Pos2::new(dragged[0])[dragged[0], new_y]);
        //             self.dragged_point = Some([dragged[0], new_y]);
        //
        //             for cycle in &mut self.cycles {
        //                 if cycle.start_time.timestamp() as f64 == dragged[0] {
        //                     cycle.lag_s = new_y;
        //                     self.close_idx = self.start_time_idx + self.close_offset + new_y;
        //                     self.open_idx = self.start_time_idx + self.open_offset + new_y;
        //                     cycle.recalc_r();
        //                     cycle.change_measurement_range();
        //                 }
        //             }
        //         }
        //     }
        // }
        // if let Some(dragged) = self.dragged_point {
        //     if response.dragged_by(egui::PointerButton::Primary) {
        //         let delta = response.drag_delta();
        //         let new_y = self.dragged_point.unwrap()[1] - delta.y as f64;
        //
        //         self.dragged_point = Some([dragged[0], new_y]);
        //
        //         for (i, cycle) in &mut self.cycles.iter_mut().enumerate() {
        //             if cycle.start_time.timestamp() as f64 == dragged[0] {
        //                 cycle.lag_s = new_y;
        //                 self.close_idx = self.start_time_idx + self.close_offset + new_y;
        //                 self.open_idx = self.start_time_idx + self.open_offset + new_y;
        //                 cycle.recalc_r();
        //                 cycle.change_measurement_range();
        //             }
        //         }
        //     }
        // }
        if response.drag_stopped() {
            self.dragged_point = None;
            // self.cycles[*self.index].update_cycle();
            self.update_current_cycle();
            // self.update_cycle();
        }

        if let Some(hovered) = hovered_point {
            if response.clicked() || response.dragged_by(egui::PointerButton::Primary) {
                let x_coord = hovered[0];

                if let Some(new_y) =
                    lag_traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
                {
                    self.selected_point = Some([x_coord, new_y]);
                }

                for (i, c) in self.cycles.iter().enumerate() {
                    if c.start_time.timestamp() as f64 == x_coord {
                        self.index.set(i);
                    }
                }
                self.update_plots();
            }
        }

        // **Force selected_point to update whenever index changes**
        if let Some(current_cycle) = self.cycles.get(self.index.count) {
            let x_coord = current_cycle.start_time.timestamp() as f64;

            if let Some(new_y) =
                lag_traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
            {
                self.selected_point = Some([x_coord, new_y]); // Keep x, update y
            }
        }

        // Draw updated selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new(PlotPoints::from(vec![selected]))
                    .name("Selected Point")
                    .shape(MarkerShape::Circle)
                    .radius(5.0)
                    .filled(false)
                    .color(egui::Color32::RED),
            );
        }

        // Draw hovered point
        if let Some(hovered) = hovered_point {
            if Some(hovered) != self.selected_point {
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![hovered]))
                        .name("Hovered Point")
                        .shape(MarkerShape::Circle)
                        .radius(5.0)
                        .filled(false)
                        .color(egui::Color32::GREEN),
                );
            }
        }
    }
    pub fn find_closest_point(
        &self,
        pointer: PlotPoint,
        traces: &HashMap<String, Vec<[f64; 2]>>, // All chamber traces
    ) -> Option<[f64; 2]> {
        // Flatten all points from all traces into a single vector
        let all_points: Vec<[f64; 2]> = traces.values().flatten().copied().collect();

        // Early exit if no points exist
        if all_points.is_empty() {
            return None;
        }

        // Efficiently compute min/max values in a single pass
        let (x_min, x_max, y_min, y_max) = all_points.iter().fold(
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY),
            |(x_min, x_max, y_min, y_max), p| {
                (
                    x_min.min(p[0]), // Track min X
                    x_max.max(p[0]), // Track max X
                    y_min.min(p[1]), // Track min Y
                    y_max.max(p[1]), // Track max Y
                )
            },
        );

        // Avoid division by zero (fallback to direct distance if range is too small)
        let x_range = (x_max - x_min).max(1e-10);
        let y_range = (y_max - y_min).max(1e-10);

        // Normalization functions
        let norm_x = |x: f64| (x - x_min) / x_range;
        let norm_y = |y: f64| (y - y_min) / y_range;

        // Find the closest point across all traces
        all_points
            .iter()
            .min_by(|a, b| {
                let dist_a = ((norm_x(a[0]) - norm_x(pointer.x)).powi(2)
                    + (norm_y(a[1]) - norm_y(pointer.y)).powi(2))
                .sqrt();
                let dist_b = ((norm_x(b[0]) - norm_x(pointer.x)).powi(2)
                    + (norm_y(b[1]) - norm_y(pointer.y)).powi(2))
                .sqrt();
                dist_a.partial_cmp(&dist_b).unwrap_or(std::cmp::Ordering::Equal)
                // Handle NaN case safely
            })
            .copied()
    }

    pub fn create_lag_plot(&self) -> Plot {
        Plot::new("Lag plot")
            // .x_grid_spacer(self.x_grid_spacer_lag())
            // .x_axis_formatter(self.x_axis_formatter_lag())
            .allow_drag(true)
            .width(600.)
            .height(350.)
            .y_axis_label("Lag (s)")
        // .legend(Legend::default().text_style(egui::TextStyle::Body))
        // Disables built-in legend
        // .legend(Legend::default().show(false))
    }

    pub fn find_bad_measurement(&mut self, gas_type: GasType) {
        let mut idx = self.index.count + 1;
        while idx < self.cycles.len() - 1
            && *self.cycles[idx].measurement_r.get(&gas_type).unwrap_or(&0.0) > 0.995
        {
            idx += 1;
        }
        // self.index = idx.min(self.cycles.len() - 1);
        self.index.set(idx.min(self.cycles.len() - 1));
        self.update_plots();
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_gas_plot_ui(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        gas_type: GasType,
        lag_s: f64,
        drag_panel_width: f64,
        calc_area_color: Color32,
        calc_area_stroke_color: Color32,
        calc_area_adjust_color: Color32,
        main_id: Id,
        left_id: Id,
        right_id: Id,
    ) {
        self.drag_panel_width = 40.;
        self.render_gas_plot(
            plot_ui,
            gas_type,
            lag_s,
            calc_area_color,
            calc_area_stroke_color,
            calc_area_adjust_color,
            main_id,
            left_id,
            right_id,
        );

        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            let drag_delta = plot_ui.pointer_coordinate_drag_delta();

            let inside_left = is_inside_polygon(
                pointer_pos,
                self.get_calc_start(gas_type),
                self.get_calc_start(gas_type) + drag_panel_width,
                self.get_min_y(&gas_type),
                self.get_max_y(&gas_type),
            );
            let inside_right = is_inside_polygon(
                pointer_pos,
                self.get_calc_end(gas_type) - drag_panel_width,
                self.get_calc_end(gas_type),
                self.get_min_y(&gas_type),
                self.get_max_y(&gas_type),
            );
            let inside_main = is_inside_polygon(
                pointer_pos,
                self.get_calc_start(gas_type) + drag_panel_width,
                self.get_calc_end(gas_type) - drag_panel_width,
                self.get_min_y(&gas_type),
                self.get_max_y(&gas_type),
            );
            let x_open: f64 = self.start_time_idx + self.open_offset + lag_s;
            let x_close = self.start_time_idx + self.close_offset + lag_s;
            let inside_lag = is_inside_polygon(
                pointer_pos,
                x_open - 20.,
                x_open + 20.,
                f64::NEG_INFINITY,
                f64::INFINITY,
            );

            let after_close = self.get_calc_start(gas_type) >= self.close_idx;
            let before_open = self.get_calc_end(gas_type) <= self.open_idx;
            let in_bounds = after_close && before_open;
            let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
            let at_start = self.get_calc_start(gas_type) == self.close_idx;
            let at_end = self.get_calc_end(gas_type) == self.open_idx;
            let range_len = self.get_calc_end(gas_type) - self.get_calc_start(gas_type);
            let cycle_len = self.open_idx - self.close_idx;

            if range_len > cycle_len {
                self.set_calc_start(gas_type, self.close_idx);
                self.set_calc_end(gas_type, self.open_idx);
            }
            if inside_left {
                handle_drag_polygon(plot_ui, self, true, &gas_type);
                self.cycles[self.index.count].get_calc_data(gas_type);
                self.cycles[self.index.count].calculate_calc_r(gas_type);
                self.cycles[self.index.count].calculate_flux(gas_type);
            }
            if inside_right {
                handle_drag_polygon(plot_ui, self, false, &gas_type);
                self.cycles[self.index.count].get_calc_data(gas_type);
                self.cycles[self.index.count].calculate_calc_r(gas_type);
                self.cycles[self.index.count].calculate_flux(gas_type);
            }
            if inside_main && in_bounds && dragged && !at_start && !at_end {
                self.increment_calc_start(gas_type, drag_delta.x as f64);
                self.increment_calc_end(gas_type, drag_delta.x as f64);
                self.cycles[self.index.count].get_calc_data(gas_type);
                self.cycles[self.index.count].calculate_calc_r(gas_type);
                self.cycles[self.index.count].calculate_flux(gas_type);
            }
            if inside_lag && dragged && !inside_right {
                self.lag_idx += drag_delta.x as f64;
                let new_lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                self.lag_idx = new_lag_s.round();
                self.close_idx = self.start_time_idx + self.close_offset + self.lag_idx;
                self.open_idx = self.start_time_idx + self.open_offset + self.lag_idx;
                self.cycles[self.index.count].lag_s = self.lag_idx;

                // self.cycles[*self.index].update_cycle();
                // self.cycles[self.index.count].get_calc_datas();
                // self.cycles[self.index.count].get_measurement_datas();
                // self.cycles[self.index.count].calculate_measurement_rs();
                // self.cycles[self.index.count].find_highest_r_windows();
                self.update_current_cycle();
                self.update_plots();
                // self.cycles[self.index.count].calculate_fluxes();
            }
            limit_to_bounds(plot_ui, self, &gas_type);
            let x_range = (self.end_time_idx - self.start_time_idx) * 0.05;
            let y_range = (self.get_max_y(&gas_type) - self.get_min_y(&gas_type)) * 0.05;
            let x_min = self.start_time_idx - x_range;
            let x_max = self.end_time_idx + x_range;
            // let x_min = x_open - 90.;
            // let x_max = x_open + 90.;
            // let mut y_min = self.get_min_y(&gas_type) - 50.;
            // let mut y_max = self.get_max_y(&gas_type) + 50.;
            let mut y_min = self.get_min_y(&gas_type) - y_range;
            let mut y_max = self.get_max_y(&gas_type) + y_range;

            if self.zoom_to_measurement {
                let x_min = x_open - 60.;
                let x_max = x_open + 60.;
                plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
            } else {
                // BUG: breaks gas plot zooming
                plot_ui.set_auto_bounds(true);
            }
        }
    }

    pub fn render_legend(&mut self, ui: &mut Ui, traces: &HashMap<String, Color32>) {
        let legend_width = 150.0;
        let color_box_size = Vec2::new(16.0, 16.0);

        let mut sorted_traces: Vec<String> = self.all_traces.iter().cloned().collect();

        // Sort numerically
        sorted_traces.sort_by(|a, b| {
            let num_a = a.parse::<f64>().ok();
            let num_b = b.parse::<f64>().ok();
            match (num_a, num_b) {
                (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.cmp(b),
            }
        });

        ui.allocate_ui_with_layout(
            Vec2::new(legend_width, ui.available_height()),
            Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.label("Legend");

                if self.visible_traces.is_empty() {
                    self.visible_traces =
                        sorted_traces.clone().into_iter().map(|s| (s, true)).collect();
                }

                for chamber_id in &sorted_traces {
                    let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);

                    ui.horizontal(|ui| {
                        let color = self.chamber_colors.get(chamber_id).unwrap().clone();

                        let response = ui.checkbox(&mut visible, "");

                        // **Single Click: Toggle Visibility Normally**
                        if response.clicked() {
                            self.toggle_visibility(chamber_id);
                            self.update_plots();
                        }

                        // **Double Click: Enable Only This Trace, Disable Others**
                        if response.double_clicked() {
                            self.visible_traces.iter_mut().for_each(|(_, v)| *v = false); // Disable all
                            self.visible_traces.insert(chamber_id.clone(), true); // Enable only this one
                            self.update_plots();
                        }

                        let (rect, _response) =
                            ui.allocate_at_least(color_box_size, egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                        ui.label(chamber_id);
                    });
                }
            },
        );
    }
    // pub fn render_legend(&mut self, ui: &mut Ui, traces: &HashMap<String, Color32>) {
    //     let legend_width = 150.0;
    //     let color_box_size = Vec2::new(16.0, 16.0);
    //
    //     // let mut sorted_traces: Vec<(&String, &Color32)> = traces.iter().collect();
    //     let mut sorted_traces: Vec<String> = self.all_traces.iter().cloned().collect();
    //
    //     // Sort numerically
    //     sorted_traces.sort_by(|(a), (b)| {
    //         let num_a = a.parse::<f64>().ok();
    //         let num_b = b.parse::<f64>().ok();
    //         match (num_a, num_b) {
    //             (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
    //             (Some(_), None) => std::cmp::Ordering::Less,
    //             (None, Some(_)) => std::cmp::Ordering::Greater,
    //             (None, None) => a.cmp(b),
    //         }
    //     });
    //
    //     ui.allocate_ui_with_layout(
    //         Vec2::new(legend_width, ui.available_height()),
    //         Layout::top_down(egui::Align::LEFT),
    //         |ui| {
    //             ui.label("Legend");
    //
    //             if self.visible_traces.is_empty() {
    //                 self.visible_traces =
    //                     sorted_traces.clone().into_iter().map(|s| (s, true)).collect();
    //                 // self.visible_traces =
    //                 // self.all_traces.into_iter().map(|s| (s, true)).collect();
    //             }
    //             for chamber_id in &sorted_traces {
    //                 // if self.visible_traces.is_empty() {
    //                 //     self.visible_traces = sorted_traces
    //                 //         .clone()
    //                 //         .into_iter()
    //                 //         .map(|s| (s, true))
    //                 //         .collect();
    //                 //     // self.visible_traces =
    //                 //     // self.all_traces.into_iter().map(|s| (s, true)).collect();
    //                 // }
    //                 let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);
    //
    //                 ui.horizontal(|ui| {
    //                     let color = self.chamber_colors.get(chamber_id).unwrap().clone();
    //                     // let mut checkbox_clicked = false;
    //
    //                     // Only allow disabling if more than one trace is visible
    //                     if ui.checkbox(&mut visible, "").clicked() {
    //                         // checkbox_clicked = true;
    //                         // println!("{:?}", self.visible_traces);
    //                         self.toggle_visibility(chamber_id);
    //                         self.update_cycle();
    //                     }
    //
    //                     // self.visible_traces.insert(chamber_id.clone(), visible);
    //
    //                     // Count how many traces are currently visible
    //                     // let visible_count = self.visible_traces.values().filter(|&&v| v).count();
    //
    //                     // if checkbox_clicked && !visible && visible_count == 1 {
    //                     //     visible = true; // Prevent disabling the last visible trace
    //                     //     self.update_cycle();
    //                     // }
    //                     let (rect, _response) =
    //                         ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //                     ui.painter().rect_filled(rect, 2.0, color);
    //                     ui.label(chamber_id);
    //                     // self.update_cycle();
    //                 });
    //             }
    //         },
    //     );
    // }
    pub fn toggle_visibility(&mut self, chamber_id: &String) {
        // Count currently visible traces
        let visible_count = self.visible_traces.values().filter(|&&v| v).count();

        // Get the current visibility state
        let is_visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);

        if is_visible && visible_count == 1 {
            // Prevent disabling the last visible trace
            return;
        }

        // Toggle visibility
        self.visible_traces.insert(chamber_id.clone(), !is_visible);
    }
    // pub fn render_legend(&mut self, ui: &mut Ui, traces: &HashMap<String, Color32>) {
    //     let legend_width = 150.0; // Fixed width for the legend
    //     let color_box_size = Vec2::new(16.0, 16.0); // Size of color indicator
    //
    //     // Ensure all traces are always available in legend
    //     let mut sorted_traces: Vec<(&String, &Color32)> = if traces.is_empty() {
    //         // If all traces are hidden, use `all_traces` to show legend items
    //         self.all_traces
    //             .iter()
    //             .map(|id| (id, self.chamber_colors.get(id).unwrap_or(&Color32::GRAY)))
    //             .collect()
    //     } else {
    //         traces.iter().collect()
    //     };
    //
    //     // Sort traces numerically when possible
    //     sorted_traces.sort_by(|(a, _), (b, _)| {
    //         let num_a = a.parse::<f64>().ok();
    //         let num_b = b.parse::<f64>().ok();
    //
    //         match (num_a, num_b) {
    //             (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal), // Numeric sorting
    //             (Some(_), None) => std::cmp::Ordering::Less, // Numbers come first
    //             (None, Some(_)) => std::cmp::Ordering::Greater, // Strings come after numbers
    //             (None, None) => a.cmp(b),                    // Default string sorting
    //         }
    //     });
    //
    //     // Always allocate UI for the legend, even if all traces are hidden
    //     ui.allocate_ui_with_layout(
    //         Vec2::new(legend_width, ui.available_height()),
    //         Layout::top_down(egui::Align::LEFT),
    //         |ui| {
    //             ui.label("Legend");
    //
    //             for (chamber_id, color) in sorted_traces {
    //                 let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);
    //
    //                 ui.horizontal(|ui| {
    //                     if ui.checkbox(&mut visible, "").changed() {
    //                         self.visible_traces.insert(chamber_id.clone(), visible);
    //                     }
    //
    //                     let (rect, _response) =
    //                         ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //                     ui.painter().rect_filled(rect, 2.0, *color);
    //                     ui.label(chamber_id);
    //                 });
    //             }
    //         },
    //     );
    // }

    // pub fn render_legend(&mut self, ui: &mut Ui, traces: &HashMap<String, Color32>) {
    //     let legend_width = 150.0; // Fixed width for the legend
    //     let color_box_size = Vec2::new(16.0, 16.0); // Size of color indicator
    //
    //     // 🔹 Convert trace names into (numeric value, original name)
    //     let mut sorted_traces: Vec<(&String, &Color32)> = traces.iter().collect();
    //
    //     // sorted traces as numbers
    //     sorted_traces.sort_by(|(a, _), (b, _)| {
    //         let num_a = a.parse::<f64>().ok();
    //         let num_b = b.parse::<f64>().ok();
    //
    //         match (num_a, num_b) {
    //             (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal), // Numeric sorting
    //             (Some(_), None) => std::cmp::Ordering::Less, // Numbers come first
    //             (None, Some(_)) => std::cmp::Ordering::Greater, // Strings come after numbers
    //             (None, None) => a.cmp(b),                    // Default string sorting
    //         }
    //     });
    //
    //     let mut sorted_traces: Vec<_> = self.all_traces.iter().collect();
    //
    //     // Sort numerically
    //     sorted_traces.sort_by(|a, b| {
    //         let num_a = a.parse::<i64>().unwrap_or(i64::MAX); // Convert string to number, or max if invalid
    //         let num_b = b.parse::<i64>().unwrap_or(i64::MAX);
    //         num_a.cmp(&num_b)
    //     });
    //
    //     ui.allocate_ui_with_layout(
    //         Vec2::new(legend_width, ui.available_height()),
    //         Layout::top_down(egui::Align::LEFT),
    //         |ui| {
    //             ui.label("Legend");
    //
    //             for chamber_id in sorted_traces {
    //                 // Sorted numerically
    //                 let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);
    //
    //                 ui.horizontal(|ui| {
    //                     if ui.checkbox(&mut visible, "").changed() {
    //                         self.visible_traces.insert(chamber_id.clone(), visible);
    //                     }
    //
    //                     let (rect, _response) =
    //                         ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //
    //                     let color = self
    //                         .chamber_colors
    //                         .get(chamber_id)
    //                         .unwrap_or(&egui::Color32::GRAY);
    //                     ui.painter().rect_filled(rect, 2.0, *color);
    //
    //                     ui.label(chamber_id);
    //                 });
    //             }
    //         },
    //     );
    //     // ui.allocate_ui_with_layout(
    //     //     Vec2::new(legend_width, ui.available_height()),
    //     //     Layout::top_down(egui::Align::LEFT),
    //     //     |ui| {
    //     //         ui.label("Legend");
    //     //
    //     //         for chamber_id in sorted_traces {
    //     //             // Sorted numerically
    //     //             let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);
    //     //
    //     //             ui.horizontal(|ui| {
    //     //                 if ui.checkbox(&mut visible, "").changed() {
    //     //                     self.visible_traces.insert(chamber_id.clone(), visible);
    //     //                 }
    //     //
    //     //                 let (rect, _response) =
    //     //                     ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //     //
    //     //                 let color = self
    //     //                     .chamber_colors
    //     //                     .get(chamber_id)
    //     //                     .unwrap_or(&egui::Color32::GRAY);
    //     //                 ui.painter().rect_filled(rect, 2.0, *color);
    //     //
    //     //                 ui.label(chamber_id);
    //     //             });
    //     //         }
    //     //     },
    //     // );
    //     // ui.allocate_ui_with_layout(
    //     //     Vec2::new(legend_width, ui.available_height()), // Fixed width
    //     //     Layout::top_down(egui::Align::LEFT),
    //     //     |ui| {
    //     //         ui.label("Legend");
    //     //
    //     //         for chamber_id in &self.all_traces {
    //     //             // Always iterate over all traces
    //     //             let mut visible = self.visible_traces.get(chamber_id).copied().unwrap_or(true);
    //     //
    //     //             ui.horizontal(|ui| {
    //     //                 // 🔹 Checkbox for toggling visibility
    //     //                 if ui.checkbox(&mut visible, "").changed() {
    //     //                     self.visible_traces.insert(chamber_id.clone(), visible);
    //     //                 }
    //     //
    //     //                 // 🔹 Reserve space for the color box
    //     //                 let (rect, _response) =
    //     //                     ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //     //
    //     //                 // 🔹 Draw the color box
    //     //                 let color = self
    //     //                     .chamber_colors
    //     //                     .get(chamber_id)
    //     //                     .unwrap_or(&egui::Color32::GRAY);
    //     //                 ui.painter().rect_filled(rect, 2.0, *color);
    //     //
    //     //                 // 🔹 Show trace name
    //     //                 ui.label(chamber_id);
    //     //             });
    //     //         }
    //     //     },
    //     // );
    //     // ui.allocate_ui_with_layout(
    //     //     Vec2::new(legend_width, ui.available_height()), // Fixed width
    //     //     Layout::top_down(egui::Align::LEFT),
    //     //     |ui| {
    //     //         ui.label("Legend");
    //     //
    //     //         for (chamber_id, color) in sorted_traces.iter() {
    //     //             ui.horizontal(|ui| {
    //     //                 // 🔹 Checkbox for toggling visibility
    //     //                 let mut visible = self
    //     //                     .visible_traces
    //     //                     .get(*chamber_id)
    //     //                     .cloned()
    //     //                     .unwrap_or(true);
    //     //                 if ui.checkbox(&mut visible, "").changed() {
    //     //                     // No label on checkbox to avoid overlap
    //     //                     self.visible_traces.insert((*chamber_id).clone(), visible);
    //     //                 }
    //     //
    //     //                 // 🔹 Reserve space for the color box
    //     //                 let (rect, _response) =
    //     //                     ui.allocate_at_least(color_box_size, egui::Sense::hover());
    //     //
    //     //                 // 🔹 Draw the color box inside the allocated rect
    //     //                 ui.painter().rect_filled(rect, 2.0, **color);
    //     //                 // 🔹 Show trace name separately for spacing
    //     //                 ui.label(*chamber_id);
    //     //             });
    //     //         }
    //     //     },
    //     // );
    // }
}

pub fn create_gas_plot<'a>(
    gas_type: &'a GasType,
    start: f64,
    end: f64,
    w: f32,
    h: f32,
) -> egui_plot::Plot<'a> {
    let x_axis_formatter = |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
        let timestamp = mark.value as i64;

        // Round to the nearest 5-minute interval (300 seconds)
        let rounded_timestamp = (timestamp / 300) * 300;

        DateTime::from_timestamp(rounded_timestamp, 0)
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_else(|| "Invalid".to_string())
    };
    Plot::new(format!("{}gas_plot", gas_type))
        .coordinates_formatter(
            Corner::RightBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = DateTime::from_timestamp(timestamp, 0)
                    .map(|dt| {
                        // DateTime::<Utc>::from_utc(dt, Utc)
                        dt.format("%Y-%m-%d %H:%M:%S").to_string()
                    })
                    .unwrap_or_else(|| format!("{:.1}", value.x));

                format!(
                    "Time: {}\n{} Concentration: {:.3} {}",
                    datetime,
                    gas_type,
                    value.y,
                    gas_type.unit()
                )
            }),
        )
        .label_formatter(|_, value| {
            let timestamp = value.x as i64;
            let datetime = DateTime::from_timestamp(timestamp, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| format!("{:.1}", value.x));

            format!("Time: {}\nConc: {:.3} ppm", datetime, value.y)
        })
        .x_axis_formatter(x_axis_formatter)
        .allow_drag(false)
        .width(w)
        .height(h)
        .include_x(start)
        .include_x(end)
        .y_axis_label(format!("{}", gas_type))
    // .legend(Legend::default().position(Corner::LeftTop))
}
pub fn init_calc_r_plot<'a>(gas_type: &'a GasType, w: f32, h: f32) -> egui_plot::Plot<'a> {
    Plot::new(format!("{}calc_r_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                    .map(|dt| {
                        DateTime::<Utc>::from_utc(dt, Utc).format("%Y-%m-%d %H:%M:%S").to_string()
                    })
                    .unwrap_or_else(|| format!("{:.1}", value.x));

                format!("Time: {}\n{} r: {:.5}", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} calc r", gas_type))
}
pub fn init_measurement_r_plot<'a>(gas_type: &'a GasType, w: f32, h: f32) -> egui_plot::Plot<'a> {
    Plot::new(format!("{}measurement_r_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                    .map(|dt| {
                        DateTime::<Utc>::from_utc(dt, Utc).format("%Y-%m-%d %H:%M:%S").to_string()
                    })
                    .unwrap_or_else(|| format!("{:.1}", value.x));

                format!("Time: {}\n{} r: {:.5}", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} measurement r", gas_type))
}
pub fn init_flux_plot<'a>(gas_type: &'a GasType, w: f32, h: f32) -> egui_plot::Plot<'a> {
    Plot::new(format!("{}flux_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                    .map(|dt| {
                        DateTime::<Utc>::from_utc(dt, Utc).format("%Y-%m-%d %H:%M:%S").to_string()
                    })
                    .unwrap_or_else(|| format!("{:.1}", value.x));

                format!("Time: {}\n{} flux: {:.3} mg/m²/h", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} flux", gas_type))
}
pub fn init_lag_plot(gas_type: &GasType, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}lag_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                    .map(|dt| {
                        DateTime::<Utc>::from_utc(dt, Utc)
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string()
                    })
                    .unwrap_or_else(|| format!("{:.1}", value.x));

                format!("Time: {}\n{} lag: {:.0} sec", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        // .label_formatter(|_, value| {
        //     let timestamp = value.x as i64;
        //     let datetime = DateTime::from_timestamp(timestamp, 0)
        //         .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
        //         .unwrap_or_else(|| format!("{:.1}", value.x));
        //
        //     format!("Time: {}\nLag: {}s ", datetime, value.y)
        // })
        .allow_drag(false)
        .width(w)
        .height(h)
        .y_axis_label(format!("{} lag", gas_type))
        .x_axis_formatter(format_x_axis)
    // .legend(Legend::default().position(Corner::LeftTop))
}
fn generate_grid_marks(range: GridInput) -> Vec<GridMark> {
    let (min, max) = range.bounds;
    let week = 604800.0; // 1 week in seconds
    let day = 86400.0; // 1 day in seconds
    let mut bigs = Vec::new();
    let mut smalls = Vec::new();
    let mut current = min - (min - week);

    // Generate weekly grid marks
    while current <= max {
        bigs.push(GridMark { value: current, step_size: week });
        current += week;
    }

    // Generate daily grid marks
    current = min - (min - day);
    while current <= max {
        smalls.push(GridMark { value: current, step_size: day });
        current += day;
    }

    // Combine both sets of grid marks
    bigs.append(&mut smalls);
    bigs
}

fn format_x_axis(mark: GridMark, _range: &RangeInclusive<f64>) -> String {
    let timestamp = mark.value as i64; // Extract timestamp

    DateTime::from_timestamp(timestamp, 0) // Use `_opt` to avoid panics
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string()) // Format as HH:MM
        .unwrap_or_else(|| "Invalid".to_string()) // Handle invalid timestamps
}

pub fn find_closest_point_screen_space(
    plot_transform: &PlotTransform,          // Required for conversion
    cursor_pos: Option<Pos2>,                // Cursor position in screen coordinates
    traces: &HashMap<String, Vec<[f64; 2]>>, // All data traces
    max_screen_distance: f32,                // Max allowed distance in screen pixels
) -> Option<[f64; 2]> {
    let cursor_screen = cursor_pos?;

    let mut closest_point: Option<[f64; 2]> = None;
    let mut min_dist = f32::INFINITY;

    for trace in traces.values() {
        for &point in trace {
            // Convert data point to screen space
            let screen_pos =
                plot_transform.position_from_point(&PlotPoint::new(point[0], point[1]));

            // Compute screen-space Euclidean distance
            let screen_dist = ((screen_pos.x - cursor_screen.x).powi(2)
                + (screen_pos.y - cursor_screen.y).powi(2))
            .sqrt();

            // Update closest point if it's within range
            if screen_dist < min_dist && screen_dist <= max_screen_distance {
                min_dist = screen_dist;
                closest_point = Some(point);
            }
        }
    }

    closest_point
}
fn generate_color(seed: &str) -> Color32 {
    let hash = fxhash::hash(seed) as u32;
    let r = ((hash >> 16) & 255) as u8;
    let g = ((hash >> 8) & 255) as u8;
    let b = (hash & 255) as u8;
    Color32::from_rgb(r, g, b)
}
