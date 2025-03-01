use crate::index::Index;
use crate::instruments::parse_secnsec_to_dt;
pub use crate::instruments::GasType;
use crate::myapp::MyApp;
use crate::myapp::{create_polygon, handle_drag_polygon, is_inside_polygon, limit_to_bounds};
use crate::structs;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Add;

use std::ops::RangeInclusive;

use crate::structs::{Cycle, CycleBuilder};
use eframe::egui::{
    show_tooltip_at, Button, Color32, Id, PointerButton, Pos2, Rect, RichText, Sense, Stroke, Ui,
};
use egui_plot::{
    AxisHints, ClosestElem, CoordinatesFormatter, Corner, GridInput, GridMark, HLine, Legend, Line,
    MarkerShape, Plot, PlotItem, PlotPoint, PlotPoints, PlotTransform, PlotUi, Points, Polygon,
    Text, VLine,
};

// pub struct Index {
//     pub count: usize,
// }
// impl Index {
//     pub fn increment(&mut self) {
//         self.count += 1;
//     }
//     pub fn decrement(&mut self) {
//         self.count -= 1;
//     }
//     pub fn reset(&mut self) {
//         self.count = 0;
//     }
//     pub fn set(&mut self, val: usize) {
//         self.count = val;
//     }
// }
// impl Default for Index {
//     fn default() -> Self {
//         Self { count: 0 }
//     }
// }
//
// impl Add for Index {
//     type Output = Self;
//
//     fn add(self, other: Self) -> Self {
//         Self {
//             count: self.count + other.count,
//         }
//     }
// }
impl MyApp {
    pub fn is_gas_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_gases.contains(gas_type)
    }
    pub fn is_flux_enabled(&self, gas_type: &GasType) -> bool {
        self.enabled_fluxes.contains(gas_type)
    }
    pub fn render_gas_plot(&self, plot_ui: &mut egui_plot::PlotUi, gas_type: GasType, lag_s: f64) {
        // let x_range = (self.end_time_idx - self.start_time_idx) * 0.05;
        // let y_range = (self.get_max_y(&gas_type) - self.get_min_y(&gas_type)) * 0.05;
        //
        // let x_min = self.start_time_idx - x_range;
        // let x_max = self.end_time_idx + x_range;
        // let y_min = self.get_min_y(&gas_type) - y_range;
        // let y_max = self.get_max_y(&gas_type) + y_range;
        // plot_ui.plot().include_x(x_min).include_x(x_max);
        // plot_ui.plot().include_y(y_min).include_y(y_max);
        // plot_ui.set_plot_bounds(egui_plot::PlotBounds::from_min_max(
        //     [x_min, y_min],
        //     [x_max, y_max],
        // ));
        if let Some(data) = self.cycles[self.index.count].gas_v.get(&gas_type) {
            let points: Vec<[f64; 2]> = self.cycles[self.index.count]
                .dt_v_as_float()
                .iter()
                .copied()
                .zip(data.iter().copied())
                .map(|(x, y)| [x, y])
                .collect();

            plot_ui.points(
                Points::new(PlotPoints::from(points))
                    .name(format!("{}", gas_type))
                    .shape(MarkerShape::Circle)
                    .color(gas_type.color())
                    .radius(2.),
            );
        } else {
            let half_way_x = self.start_time_idx + ((self.end_time_idx - self.start_time_idx) / 2.);
            let bad_plot = Id::new(format!("bad_plot {}", gas_type));
            plot_ui.text(
                Text::new(
                    PlotPoint::new(half_way_x, 0),
                    RichText::new("No data points").size(20.),
                )
                .id(bad_plot),
            );
        }
    }
    pub fn prepare_plot_data(&mut self) {
        let cycle = &self.cycles[self.index.count];
        self.gas_plot.clear(); // Clear existing data before recalculating

        for (gas_type, gas_v) in &cycle.gas_v {
            let data: Vec<[f64; 2]> = cycle
                .dt_v_as_float()
                .iter()
                .copied()
                .zip(gas_v.iter().copied())
                .map(|(x, y)| [x, y])
                .collect();

            self.gas_plot.insert(*gas_type, data);
        }
    }
    pub fn get_cycle(&mut self) -> &structs::Cycle {
        &self.cycles[self.index.count]
    }
    // pub fn mk_main(&mut self, gas_type: GasType) {
    //     let main_polygon = create_polygon(
    //         self.get_calc_start(gas_type) + self.drag_panel_width,
    //         self.get_calc_end(gas_type) - self.drag_panel_width,
    //         self.get_min_y(&gas_type),
    //         self.get_max_y(&gas_type),
    //         self.calc_area_color,
    //         self.calc_area_stroke_color,
    //         "Move",
    //     );
    // }

    pub fn calculate_min_y(&mut self) {
        let cycle = &self.cycles[self.index.count];
        self.min_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &cycle.gas_v {
            let min_value = gas_v
                .iter()
                .copied()
                .filter(|v| !v.is_nan())
                .fold(f64::INFINITY, f64::min);

            self.min_y.insert(*gas_type, min_value);
        }
    }
    pub fn get_min_y(&self, gas_type: &GasType) -> f64 {
        *self.min_y.get(gas_type).unwrap_or(&0.0)
    }
    pub fn get_max_y(&self, gas_type: &GasType) -> f64 {
        *self.max_y.get(gas_type).unwrap_or(&0.0)
    }
    pub fn calculate_max_y(&mut self) {
        let cycle = &self.cycles[self.index.count];
        self.max_y.clear(); // Clear previous data

        for (gas_type, gas_v) in &cycle.gas_v {
            let min_value = gas_v
                .iter()
                .copied()
                .filter(|v| !v.is_nan())
                .fold(f64::NEG_INFINITY, f64::max);

            self.max_y.insert(*gas_type, min_value);
        }
    }

    // fn calculate_min_y(&mut self, cycle: &structs::Cycle) {
    //     self.min_y.clear();
    //     for (gas_type, gas_v) in &cycle.gas_v {
    //         let data: Vec<f64> = cycle
    //             .dt_v_as_float()
    //             .iter()
    //             .copied()
    //             .zip(gas_v.iter().copied())
    //             .map(|(x, y)| [x, y])
    //             .collect();
    //
    //         self.gas_plot.insert(*gas_type, data);
    //     }

    // cycle
    //     .gas_v
    //     .get(&gas_type)
    //     .map(|gas_v| {
    //         gas_v
    //             .iter()
    //             .copied()
    //             .filter(|v| !v.is_nan())
    //             .fold(f64::INFINITY, f64::min)
    //     })
    //     .unwrap_or(f64::INFINITY)
    // }

    // pub fn prepare_plot_data(cycle: &structs::Cycle, gas_type: GasType) -> Vec<[f64; 2]> {
    //     if let Some(gas_v) = cycle.gas_v.get(&gas_type) {
    //         cycle
    //             .dt_v_as_float()
    //             .iter()
    //             .copied()
    //             .zip(gas_v.iter().copied())
    //             .map(|(x, y)| [x, y])
    //             .collect()
    //     } else {
    //         Vec::new()
    //     }
    // }
    pub fn update_cycle(&mut self) {
        let index = self.index.count;
        // let cycle = self.get_cycle();
        // let cycle = &self.cycles[index];
        self.calculate_min_y();
        self.calculate_max_y();
        // self.prepare_plot_data();
        // let gas_type = GasType::CH4;
        // self.gas_plot = cycle
        //     .dt_v_as_float()
        //     .iter()
        //     .copied()
        //     .zip(cycle.gas_v.iter().copied())
        //     .map(|(x, y)| [x, y])
        //     .collect();

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
        self.gases = self.cycles[index].gases.clone();
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
    // pub fn create_flux_plot(&mut self, gas_type: &GasType) -> Vec<[f64; 2]> {
    pub fn create_lag_traces(&mut self) -> HashMap<String, Vec<[f64; 2]>> {
        let mut flux_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        for cycle in &self.cycles {
            let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
            let lag_value = cycle.lag_s; // Get flux value
            let start_time = cycle.start_time.timestamp() as f64; // Get timestamp

            flux_map
                .entry(chamber_id)
                .or_insert_with(Vec::new) // Ensure entry exists
                .push([start_time, lag_value]); // Append the point
        }

        flux_map
    }
    pub fn create_flux_traces(&mut self, gas_type: &GasType) -> HashMap<String, Vec<[f64; 2]>> {
        let mut flux_map: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        for cycle in &self.cycles {
            let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
            let flux_value = *cycle.flux.get(gas_type).unwrap_or(&0.0); // Get flux value
            let start_time = cycle.start_time.timestamp() as f64; // Get timestamp

            flux_map
                .entry(chamber_id)
                .or_insert_with(Vec::new) // Ensure entry exists
                .push([start_time, flux_value]); // Append the point
        }

        flux_map
    }

    pub fn get_calc_end(&self, gas_type: GasType) -> f64 {
        *self.cycles[self.index.count]
            .calc_range_end
            .get(&gas_type)
            .unwrap_or(&0.0)
    }
    pub fn get_calc_start(&self, gas_type: GasType) -> f64 {
        *self.cycles[self.index.count]
            .calc_range_start
            .get(&gas_type)
            .unwrap_or(&0.0)
    }
    pub fn set_calc_start(&mut self, gas_type: GasType, x: f64) {
        self.cycles[self.index.count]
            .calc_range_start
            .insert(gas_type, x);
    }
    pub fn set_calc_end(&mut self, gas_type: GasType, x: f64) {
        self.cycles[self.index.count]
            .calc_range_end
            .insert(gas_type, x);
    }
    pub fn decrement_calc_start(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count]
            .calc_range_start
            .get(&gas_type)
            .unwrap_or(&0.0);
        let x = s - x;
        self.cycles[self.index.count]
            .calc_range_start
            .insert(gas_type, x);
    }
    pub fn increment_calc_start(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count]
            .calc_range_start
            .get(&gas_type)
            .unwrap_or(&0.0);
        let x = s + x;
        self.cycles[self.index.count]
            .calc_range_start
            .insert(gas_type, x);
    }
    pub fn increment_calc_end(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count]
            .calc_range_end
            .get(&gas_type)
            .unwrap_or(&0.0);
        let x = s + x;
        self.cycles[self.index.count]
            .calc_range_end
            .insert(gas_type, x);
    }
    pub fn decrement_calc_end(&mut self, gas_type: GasType, x: f64) {
        let s = self.cycles[self.index.count]
            .calc_range_end
            .get(&gas_type)
            .unwrap_or(&0.0);
        let x = s - x;
        self.cycles[self.index.count]
            .calc_range_end
            .insert(gas_type, x);
    }

    pub fn new(mut cycles: Vec<Cycle>) -> Self {
        let cycle = &mut cycles[0];
        let gas_type = GasType::CH4;
        // cycle.prepare_plot_data();
        cycle.calculate_min_y();
        cycle.calculate_max_y();
        // let gas_plot: Vec<[f64; 2]> = cycle
        //     .dt_v_as_float()
        //     .iter()
        //     .copied() // Copy each f64 from the iterator
        //     .zip(cycle.gas_v.iter().copied()) // Iterate and copy gas_v
        //     .map(|(x, y)| [x, y]) // Convert each tuple into an array
        //     .collect();
        let lag_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
        let close_idx = cycle.close_time.timestamp() as f64 + cycle.lag_s;
        let open_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
        let open_offset = cycle.open_offset as f64;
        let close_offset = cycle.close_offset as f64;
        let start_time_idx = cycle.start_time.timestamp() as f64;
        let end_time_idx = cycle.end_time.timestamp() as f64;
        let calc_range_end = cycle.calc_range_end.clone();
        let calc_range_start = cycle.calc_range_start.clone();
        let min_y = cycle.min_y.clone();
        let max_y = cycle.max_y.clone();
        let gas_plot = cycle.gas_plot.clone();
        // let min_y = cycle
        //     .gas_v
        //     .iter()
        //     .cloned()
        //     .filter(|v| !v.is_nan())
        //     .fold(f64::INFINITY, f64::min);
        // let max_y = cycle
        //     .gas_v
        //     .iter()
        //     .cloned()
        //     // .rev()
        //     // .take(120)
        //     .filter(|v| !v.is_nan())
        //     .fold(f64::NEG_INFINITY, f64::max);

        // let max_y;
        // let min_y;
        let min_calc_area_range = 240.;
        let lag_vec: Vec<f64> = cycles.iter().map(|x| x.lag_s).collect();
        let start_vec: Vec<f64> = cycles
            .iter()
            .map(|x| x.start_time.timestamp() as f64)
            .collect();
        let lag_plot: Vec<[f64; 2]> = start_vec
            .iter()
            .copied() // Copy each f64 from the iterator
            .zip(lag_vec.iter().copied()) // Iterate and copy gas_v
            .map(|(x, y)| [x, y]) // Convert each tuple into an array
            .collect();
        let index = Index::default();
        let drag_panel_width = 40.0;
        let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
        let calc_area_adjust_color = Color32::from_rgba_unmultiplied(64, 242, 106, 50);
        let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
        let selected_point = None;
        let dragged_point = None;
        let gases = Vec::new();
        // let enabled_gases = HashSet::from([GasType::CH4, GasType::CO2])
        Self {
            cycles,
            gases,
            selected_point,
            dragged_point,
            r_lim: 1.,
            chamber_colors: HashMap::new(),
            enabled_gases: HashSet::from([GasType::CH4, GasType::CO2]),
            enabled_fluxes: HashSet::from([GasType::CH4, GasType::CO2]),
            gas_plot,
            calc_area_color,
            calc_area_adjust_color,
            calc_area_stroke_color,
            drag_panel_width,
            lag_idx,
            close_idx,
            open_idx,
            open_offset,
            close_offset,
            start_time_idx,
            end_time_idx,
            calc_range_end,
            calc_range_start,
            max_y,
            min_y,
            min_calc_area_range,
            index,
            lag_vec,
            start_vec,
            lag_plot,
        }
    }
    pub fn render_flux_plot(&mut self, plot_ui: &mut egui_plot::PlotUi, gas_type: GasType) {
        let flux_traces = self.create_flux_traces(&gas_type);
        let mut hovered_point: Option<[f64; 2]> = None;

        // Sort chamber IDs for consistent rendering
        let mut chamber_ids: Vec<&String> = flux_traces.keys().collect();
        chamber_ids.sort();

        fn generate_color(seed: &str) -> Color32 {
            let hash = fxhash::hash(seed) as u32;
            let r = ((hash >> 16) & 255) as u8;
            let g = ((hash >> 8) & 255) as u8;
            let b = (hash & 255) as u8;
            Color32::from_rgb(r, g, b)
        }

        for chamber_id in chamber_ids {
            if let Some(lag_points) = flux_traces.get(chamber_id) {
                let color = self
                    .chamber_colors
                    .entry(chamber_id.clone())
                    .or_insert_with(|| generate_color(chamber_id));

                let plot_points = PlotPoints::from(lag_points.clone());

                plot_ui.points(
                    Points::new(plot_points)
                        .name(format!("ID {}", chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
                        .color(*color),
                );
            }
        }

        // **Ensure selected_point updates properly when clicking OR changing index**
        if let transform = plot_ui.transform() {
            if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
                hovered_point = find_closest_point_screen_space(
                    &transform,
                    Some(cursor_screen_pos),
                    &flux_traces,
                    80.0,
                );
            }
        }

        if plot_ui.response().clicked() {
            if let Some(closest) = hovered_point {
                let x_coord = closest[0];

                // Find the newest y-coordinate (flux value) for this x
                if let Some(new_y) = flux_traces
                    .values()
                    .flatten()
                    .filter(|p| p[0] == x_coord)
                    .map(|p| p[1])
                    .last()
                {
                    self.selected_point = Some([x_coord, new_y]);
                }

                // **Update index when clicking on a new measurement**
                for (i, c) in self.cycles.iter().enumerate() {
                    if c.start_time.timestamp() as f64 == x_coord {
                        self.index.set(i);
                    }
                }
                self.update_cycle();
            }
        }

        // **Force `selected_point` to update whenever `index` changes**
        if let Some(current_cycle) = self.cycles.get(self.index.count) {
            let x_coord = current_cycle.start_time.timestamp() as f64;

            if let Some(new_y) = flux_traces
                .values()
                .flatten()
                .filter(|p| p[0] == x_coord)
                .map(|p| p[1])
                .last()
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

    pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
        let mut lag_traces = self.create_lag_traces();
        let mut hovered_point: Option<[f64; 2]> = None;

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
            if let Some(lag_points) = lag_traces.clone().get_mut(chamber_id) {
                let color = self
                    .chamber_colors
                    .entry(chamber_id.clone())
                    .or_insert_with(|| generate_color(chamber_id));

                let plot_points = PlotPoints::from(lag_points.clone());

                plot_ui.points(
                    Points::new(plot_points)
                        .name(format!("Flux Chamber {}", chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
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
                self.selected_point = self.dragged_point;
                self.dragged_point = Some(hovered);
            }
        }

        self.update_cycle();
        if let Some(dragged) = self.dragged_point {
            if response.dragged_by(egui::PointerButton::Primary) {
                if let delta = response.drag_delta() {
                    let new_y = self.dragged_point.unwrap()[1] - delta.y as f64 * 0.3; // ✅ Continuously update dragged Y

                    // ✅ Update `self.dragged_point` dynamically
                    self.dragged_point = Some([dragged[0], new_y]);

                    // ✅ Update `lag_s` in cycles for the dragged point
                    for (i, cycle) in &mut self.cycles.iter_mut().enumerate() {
                        if cycle.start_time.timestamp() as f64 == dragged[0] {
                            cycle.lag_s = new_y; // Only update the locked point
                                                 // self.lag_idx += drag_delta.x as f64;
                                                 // let new_lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                                                 // self.lag_idx = new_lag_s.round();
                            self.close_idx = self.start_time_idx + self.close_offset + new_y;
                            self.open_idx = self.start_time_idx + self.open_offset + new_y;
                            cycle.recalc_r();
                            cycle.change_measurement_range();
                            self.index.set(i);
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            self.dragged_point = None; // Stop dragging when released
        }

        // **Update selected point when clicking**
        if let Some(hovered) = hovered_point {
            if response.clicked() {
                let x_coord = hovered[0];

                if let Some(new_y) = lag_traces
                    .values()
                    .flatten()
                    .filter(|p| p[0] == x_coord)
                    .map(|p| p[1])
                    .last()
                {
                    self.selected_point = Some([x_coord, new_y]);
                }

                for (i, c) in self.cycles.iter().enumerate() {
                    if c.start_time.timestamp() as f64 == x_coord {
                        self.index.set(i);
                    }
                }
                self.update_cycle();
            }
        }

        // **Force selected_point to update whenever index changes**
        if let Some(current_cycle) = self.cycles.get(self.index.count) {
            let x_coord = current_cycle.start_time.timestamp() as f64;

            if let Some(new_y) = lag_traces
                .values()
                .flatten()
                .filter(|p| p[0] == x_coord)
                .map(|p| p[1])
                .last()
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
    // pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
    //     let mut lag_traces = self.create_lag_traces();
    //     let mut hovered_point: Option<[f64; 2]> = None;
    //     let mut dragged_point: Option<[f64; 2]> = None; // Store the currently dragged point
    //
    //     let mut chamber_ids: Vec<&String> = lag_traces.keys().collect();
    //     // chamber_ids.sort();
    //
    //     fn generate_color(seed: &str) -> Color32 {
    //         let hash = fxhash::hash(seed) as u32;
    //         let r = ((hash >> 24) & 255) as u8;
    //         let g = ((hash >> 8) & 255) as u8;
    //         let b = (hash & 255) as u8;
    //         Color32::from_rgb(r, g, b)
    //     }
    //
    //     for chamber_id in chamber_ids {
    //         if let Some(lag_points) = lag_traces.clone().get_mut(chamber_id) {
    //             let color = self
    //                 .chamber_colors
    //                 .entry(chamber_id.clone())
    //                 .or_insert_with(|| generate_color(chamber_id));
    //
    //             let plot_points = PlotPoints::from(lag_points.clone());
    //
    //             plot_ui.points(
    //                 Points::new(plot_points)
    //                     .name(format!("Flux Chamber {}", chamber_id))
    //                     .shape(MarkerShape::Circle)
    //                     .radius(2.)
    //                     .color(*color),
    //             );
    //         }
    //     }
    //
    //     if let transform = plot_ui.transform() {
    //         if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
    //             hovered_point = find_closest_point_screen_space(
    //                 &transform,
    //                 Some(cursor_screen_pos),
    //                 &lag_traces,
    //                 80.0,
    //             );
    //         }
    //     }
    //
    //     // **Dragging Mechanism**
    //     if let Some(hovered) = hovered_point {
    //         let response = plot_ui.response();
    //
    //         if response.dragged_by(egui::PointerButton::Primary) {
    //             dragged_point = Some(hovered); // Store the dragged point
    //         }
    //
    //         if let Some(dragged) = dragged_point {
    //             if let delta = response.drag_delta() {
    //                 let new_y = dragged[1] - delta.y as f64; // Scale delta for precision
    //
    //                 // Update lag_s value in `cycles`
    //                 for cycle in &mut self.cycles {
    //                     if cycle.start_time.timestamp() as f64 == dragged[0] {
    //                         cycle.lag_s = new_y; // Update lag_s in real-time
    //                         cycle.recalc_r();
    //                     }
    //                 }
    //                 self.update_cycle();
    //
    //                 self.selected_point = Some([dragged[0], new_y]);
    //             }
    //         }
    //
    //         // if response.drag_released() {
    //         //     dragged_point = None; // Stop dragging when released
    //         // }
    //     }
    //
    //     // **Update selected point when clicking**
    //     if let Some(hovered) = hovered_point {
    //         if plot_ui.response().clicked() {
    //             let x_coord = hovered[0];
    //
    //             if let Some(new_y) = lag_traces
    //                 .values()
    //                 .flatten()
    //                 .filter(|p| p[0] == x_coord)
    //                 .map(|p| p[1])
    //                 .last()
    //             {
    //                 self.selected_point = Some([x_coord, new_y]);
    //             }
    //
    //             for (i, c) in self.cycles.iter().enumerate() {
    //                 if c.start_time.timestamp() as f64 == x_coord {
    //                     self.index.set(i);
    //                 }
    //             }
    //             self.update_cycle();
    //         }
    //     }
    //
    //     // **Force selected_point to update whenever index changes**
    //     if let Some(current_cycle) = self.cycles.get(self.index.count) {
    //         let x_coord = current_cycle.start_time.timestamp() as f64;
    //
    //         if let Some(new_y) = lag_traces
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
    //                 .name("Selected Point")
    //                 .shape(MarkerShape::Circle)
    //                 .radius(5.0)
    //                 .filled(false)
    //                 .color(egui::Color32::RED),
    //         );
    //     }
    //
    //     // Draw hovered point
    //     if let Some(hovered) = hovered_point {
    //         if Some(hovered) != self.selected_point {
    //             plot_ui.points(
    //                 Points::new(PlotPoints::from(vec![hovered]))
    //                     .name("Hovered Point")
    //                     .shape(MarkerShape::Circle)
    //                     .radius(5.0)
    //                     .filled(false)
    //                     .color(egui::Color32::GREEN),
    //             );
    //         }
    //     }
    // }
    // pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
    //     let lag_traces = self.create_lag_traces();
    //     let mut hovered_point: Option<[f64; 2]> = None;
    //
    //     // Sort chamber IDs for consistent rendering
    //     let mut chamber_ids: Vec<&String> = lag_traces.keys().collect();
    //     chamber_ids.sort();
    //
    //     fn generate_color(seed: &str) -> Color32 {
    //         let hash = fxhash::hash(seed) as u32;
    //         let r = ((hash >> 24) & 255) as u8;
    //         let g = ((hash >> 8) & 255) as u8;
    //         let b = (hash & 255) as u8;
    //         Color32::from_rgb(r, g, b)
    //     }
    //
    //     for chamber_id in chamber_ids {
    //         if let Some(lag_points) = lag_traces.get(chamber_id) {
    //             let color = self
    //                 .chamber_colors
    //                 .entry(chamber_id.clone())
    //                 .or_insert_with(|| generate_color(chamber_id));
    //
    //             let plot_points = PlotPoints::from(lag_points.clone());
    //
    //             plot_ui.points(
    //                 Points::new(plot_points)
    //                     .name(format!("Flux Chamber {}", chamber_id))
    //                     .shape(MarkerShape::Circle)
    //                     .radius(2.)
    //                     .color(*color),
    //             );
    //         }
    //     }
    //
    //     // **Find closest point based on screen coordinates**
    //     if let transform = plot_ui.transform() {
    //         if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
    //             hovered_point = find_closest_point_screen_space(
    //                 &transform,
    //                 Some(cursor_screen_pos),
    //                 &lag_traces,
    //                 80.0,
    //             );
    //         }
    //     }
    //
    //     // **Ensure selected_point updates properly when clicking OR changing index**
    //     if let Some(hovered) = hovered_point {
    //         if plot_ui.response().clicked() {
    //             let x_coord = hovered[0];
    //
    //             // Find the newest y-coordinate (flux value) for this x
    //             if let Some(new_y) = lag_traces
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
    //         if let Some(new_y) = lag_traces
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
    //                 .name("Selected Point")
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
    //                     .name("Hovered Point")
    //                     .shape(MarkerShape::Circle)
    //                     .radius(5.0)
    //                     .filled(false)
    //                     .color(egui::Color32::GREEN),
    //             );
    //         }
    //     }
    // }
    // pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
    //     let lag_traces = self.create_lag_traces();
    //     let mut hovered_point: Option<[f64; 2]> = None;
    //
    //     // Sort chamber IDs for consistent rendering
    //     let mut chamber_ids: Vec<&String> = lag_traces.keys().collect();
    //     chamber_ids.sort();
    //
    //     fn generate_color(seed: &str) -> Color32 {
    //         let hash = fxhash::hash(seed) as u32;
    //         let r = ((hash >> 16) & 255) as u8;
    //         let g = ((hash >> 8) & 255) as u8;
    //         let b = (hash & 255) as u8;
    //         Color32::from_rgb(r, g, b)
    //     }
    //
    //     for chamber_id in chamber_ids {
    //         if let Some(lag_points) = lag_traces.get(chamber_id) {
    //             let color = self
    //                 .chamber_colors
    //                 .entry(chamber_id.clone())
    //                 .or_insert_with(|| generate_color(chamber_id));
    //
    //             let plot_points = PlotPoints::from(lag_points.clone());
    //
    //             plot_ui.points(
    //                 Points::new(plot_points)
    //                     .name(format!("Flux Chamber {}", chamber_id))
    //                     .shape(MarkerShape::Circle)
    //                     .radius(2.)
    //                     .color(*color),
    //             );
    //         }
    //     }
    //
    //     // **Ensure selected_point updates properly when clicking OR changing index**
    //
    //     if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
    //         hovered_point = self.find_closest_point(pointer_pos, &lag_traces);
    //
    //         if plot_ui.response().clicked() {
    //             if let Some(closest) = hovered_point {
    //                 let x_coord = closest[0];
    //
    //                 // Find the newest y-coordinate (flux value) for this x
    //                 if let Some(new_y) = lag_traces
    //                     .values()
    //                     .flatten()
    //                     .filter(|p| p[0] == x_coord)
    //                     .map(|p| p[1])
    //                     .last()
    //                 {
    //                     self.selected_point = Some([x_coord, new_y]);
    //                 }
    //
    //                 // **Update index when clicking on a new measurement**
    //                 for (i, c) in self.cycles.iter().enumerate() {
    //                     if c.start_time.timestamp() as f64 == x_coord {
    //                         self.index.set(i);
    //                     }
    //                 }
    //                 self.update_cycle();
    //             }
    //         }
    //     }
    //
    //     // **Force `selected_point` to update whenever `index` changes**
    //     if let Some(current_cycle) = self.cycles.get(self.index.count) {
    //         let x_coord = current_cycle.start_time.timestamp() as f64;
    //
    //         if let Some(new_y) = lag_traces
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
    //                 .name("Selected Point")
    //                 .shape(MarkerShape::Diamond)
    //                 .radius(5.0)
    //                 .color(egui::Color32::YELLOW),
    //         );
    //     }
    //
    //     // Draw hovered point (if not the selected point)
    //     if let Some(hovered) = hovered_point {
    //         if Some(hovered) != self.selected_point {
    //             plot_ui.points(
    //                 Points::new(PlotPoints::from(vec![hovered]))
    //                     .name("Hovered Point")
    //                     .shape(MarkerShape::Diamond)
    //                     .radius(5.0)
    //                     .color(egui::Color32::GREEN),
    //             );
    //         }
    //     }
    // }

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
            (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ),
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
                dist_a
                    .partial_cmp(&dist_b)
                    .unwrap_or(std::cmp::Ordering::Equal) // Handle NaN case safely
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
            .legend(Legend::default().position(Corner::LeftTop))
    }

    pub fn find_bad_measurement(&mut self, gas_type: GasType) {
        let mut idx = self.index.count + 1;
        while idx < self.cycles.len() - 1
            && *self.cycles[idx]
                .measurement_r
                .get(&gas_type)
                .unwrap_or(&0.0)
                > 0.995
        {
            idx += 1;
        }
        // self.index = idx.min(self.cycles.len() - 1);
        self.index.set(idx.min(self.cycles.len() - 1));
        self.update_cycle();
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
        self.render_gas_plot(plot_ui, gas_type, lag_s);

        let main_polygon = create_polygon(
            self.get_calc_start(gas_type) + drag_panel_width,
            self.get_calc_end(gas_type) - drag_panel_width,
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_color,
            calc_area_stroke_color,
            "Move",
            main_id,
        );

        let left_polygon = create_polygon(
            self.get_calc_start(gas_type),
            self.get_calc_start(gas_type) + drag_panel_width,
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_adjust_color,
            calc_area_stroke_color,
            "Extend left",
            left_id,
        );

        let right_polygon = create_polygon(
            self.get_calc_end(gas_type) - drag_panel_width,
            self.get_calc_end(gas_type),
            self.get_min_y(&gas_type),
            self.get_max_y(&gas_type),
            calc_area_adjust_color,
            calc_area_stroke_color,
            "Extend right",
            right_id,
        );

        let x_open: f64 = self.start_time_idx + self.open_offset + lag_s;
        // let x_open: f64 = self.cycles[*self.index].start_time.timestamp() as f64
        //     + self.open_offset
        //     + self.cycles[*self.index].lag_s;
        // let x_close: f64 = self.cycles[*self.index].start_time.timestamp() as f64
        //     + self.close_offset
        //     + self.cycles[*self.index].lag_s;
        let x_close = self.start_time_idx + self.close_offset + lag_s;
        let max_vl = VLine::new(x_open)
            .name("Lagtime")
            .width(2.0)
            .allow_hover(true);

        let close_vl = VLine::new(x_close)
            .name("Close time")
            .width(2.0)
            .allow_hover(true);

        plot_ui.polygon(main_polygon);
        plot_ui.polygon(left_polygon);
        plot_ui.polygon(right_polygon);
        plot_ui.vline(max_vl);
        plot_ui.vline(close_vl);

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

                self.cycles[self.index.count].get_calc_datas();
                self.cycles[self.index.count].get_measurement_datas();
                self.cycles[self.index.count].calculate_measurement_rs();
                self.cycles[self.index.count].find_highest_r_windows();
                self.update_cycle();
                self.cycles[self.index.count].calculate_fluxes();
            }
            limit_to_bounds(plot_ui, self, &gas_type)
        }
    }
}

pub fn create_gas_plot<'a>(gas_type: &'a GasType, start: f64, end: f64) -> egui_plot::Plot<'a> {
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
        .width(600.)
        .height(350.)
        .include_x(start)
        .include_x(end)
        .y_axis_label("CH4")
        .legend(Legend::default().position(Corner::LeftTop))
}
pub fn init_flux_plot<'a>(gas_type: &'a GasType) -> egui_plot::Plot<'a> {
    Plot::new(format!("{}flux_plot", gas_type))
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

                format!(
                    "Time: {}\n{} flux: {:.3} mg/m²/h",
                    datetime, gas_type, value.y
                )
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(600.)
        .height(350.)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} flux", gas_type))
        .legend(Legend::default().position(Corner::LeftTop))
}
pub fn init_lag_plot(gas_type: &GasType) -> egui_plot::Plot {
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
        .width(600.)
        .height(350.)
        .y_axis_label(format!("{} lag", gas_type))
        .x_axis_formatter(format_x_axis)
        .legend(Legend::default().position(Corner::LeftTop))
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
        bigs.push(GridMark {
            value: current,
            step_size: week,
        });
        current += week;
    }

    // Generate daily grid marks
    current = min - (min - day);
    while current <= max {
        smalls.push(GridMark {
            value: current,
            step_size: day,
        });
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
