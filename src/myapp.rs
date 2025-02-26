use crate::{instruments::GasType, prepare_plot_data};
use std::collections::HashMap;
use std::ops::Add;
// use crate::{instruments::GasType, prepare_plot_data};
use crate::structs;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::any::Any;

use crate::structs::{Cycle, CycleBuilder};
use eframe::egui::{
    show_tooltip_at, Button, Color32, Id, PointerButton, Pos2, Rect, Sense, Stroke, Ui,
};
use egui_plot::{
    AxisHints, ClosestElem, Corner, GridInput, GridMark, HLine, Legend, Line, MarkerShape, Plot,
    PlotItem, PlotPoint, PlotPoints, PlotUi, Points, Polygon, VLine,
};
#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Index {
    count: usize,
}
impl Index {
    pub fn increment(&mut self) {
        self.count += 1;
    }
    pub fn decrement(&mut self) {
        self.count -= 1;
    }
    pub fn reset(&mut self) {
        self.count = 0;
    }
    pub fn set(&mut self, val: usize) {
        self.count = val;
    }
}
impl Default for Index {
    fn default() -> Self {
        Self { count: 0 }
    }
}

impl Add for Index {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            count: self.count + other.count,
        }
    }
}

#[derive(Default)]
pub struct MyApp {
    cycles: Vec<Cycle>,
    gas_plot: HashMap<GasType, Vec<[f64; 2]>>,
    // gas_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
    lag_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
    // flux_plot: HashMap<GasType, Vec<[f64; 2]>>,
    lag_idx: f64, // Add a vecxy tor of values to your struct
    close_idx: f64,
    open_offset: f64,
    close_offset: f64,
    open_idx: f64,
    start_time_idx: f64,
    calc_range_start: HashMap<GasType, f64>,
    calc_range_end: HashMap<GasType, f64>,
    max_y: HashMap<GasType, f64>,
    min_y: HashMap<GasType, f64>,
    drag_panel_width: f64,
    calc_area_color: Color32,
    calc_area_adjust_color: Color32,
    calc_area_stroke_color: Color32,
    // max_y: f64,
    // min_y: f64,
    // max_y: f64,
    // min_y: f64,
    min_calc_area_range: f64,
    index: Index,
    lag_vec: Vec<f64>,
    start_vec: Vec<f64>,
}
// fn calculate_min_y(cycle: &structs::Cycle, gas_type: GasType) -> f64 {
//     cycle
//         .gas_v
//         .get(&gas_type)
//         .map(|gas_v| {
//             gas_v
//                 .iter()
//                 .copied()
//                 .filter(|v| !v.is_nan())
//                 .fold(f64::INFINITY, f64::min)
//         })
//         .unwrap_or(f64::INFINITY)
// }

fn calculate_max_y(cycle: &structs::Cycle, gas_type: GasType) -> f64 {
    cycle
        .gas_v
        .get(&gas_type)
        .map(|gas_v| {
            gas_v
                .iter()
                .copied()
                .filter(|v| !v.is_nan())
                .fold(f64::NEG_INFINITY, f64::max)
        })
        .unwrap_or(f64::NEG_INFINITY)
}
// pub fn calculate_min_y(cycle: &structs::Cycle) {
//     // let cycle = &self.cycles[self.index.count];
//     cycle.min_y.clear(); // Clear previous data
//
//     for (gas_type, gas_v) in &cycle.gas_v {
//         let min_value = gas_v
//             .iter()
//             .copied()
//             .filter(|v| !v.is_nan())
//             .fold(f64::INFINITY, f64::min);
//
//         cycle.min_y.insert(*gas_type, min_value);
//     }
// }

impl MyApp {
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
    pub fn update_cycle(&mut self, index: usize) {
        // let cycle = self.get_cycle();
        // let cycle = &self.cycles[index];
        self.calculate_min_y();
        self.calculate_max_y();
        self.prepare_plot_data();
        // let gas_type = GasType::CH4;
        // self.gas_plot = cycle
        //     .dt_v_as_float()
        //     .iter()
        //     .copied()
        //     .zip(cycle.gas_v.iter().copied())
        //     .map(|(x, y)| [x, y])
        //     .collect();

        self.lag_idx = self.cycles[self.index.count].open_time.timestamp() as f64
            + self.cycles[self.index.count].lag_s;
        self.close_idx = self.cycles[self.index.count].close_time.timestamp() as f64
            + self.cycles[self.index.count].lag_s;
        self.open_idx = self.cycles[self.index.count].start_time.timestamp() as f64
            + self.cycles[self.index.count].open_offset as f64
            + self.cycles[self.index.count].lag_s;
        self.open_offset = self.cycles[self.index.count].open_offset as f64;
        self.close_offset = self.cycles[self.index.count].close_offset as f64;
        self.start_time_idx = self.cycles[self.index.count].start_time.timestamp() as f64;
        self.calc_range_end = self.cycles[self.index.count].calc_range_end.clone();
        self.calc_range_start = self.cycles[self.index.count].calc_range_start.clone();
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
        self.lag_vec = self.cycles.iter().map(|x| x.lag_s).collect();
        self.start_vec = self
            .cycles
            .iter()
            .map(|x| x.start_time.timestamp() as f64)
            .collect();
        self.lag_plot = self
            .start_vec
            .iter()
            .copied() // Copy each f64 from the iterator
            .zip(self.lag_vec.iter().copied()) // Iterate and copy gas_v
            .map(|(x, y)| [x, y]) // Convert each tuple into an array
            .collect();
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
    pub fn create_flux_plot(&mut self, gas_type: &GasType) -> Vec<[f64; 2]> {
        let flux_vec: Vec<f64> = self
            .cycles
            .iter()
            .map(|x| *x.flux.get(gas_type).unwrap_or(&0.0))
            .collect();
        self.start_vec = self
            .cycles
            .iter()
            .map(|x| x.start_time.timestamp() as f64)
            .collect();
        // gas_plot: HashMap<GasType, Vec<[f64; 2]>>,
        let flux_plot: Vec<[f64; 2]> = self
            .start_vec
            .iter()
            .copied() // Copy each f64 from the iterator
            .zip(flux_vec.iter().copied()) // Iterate and copy gas_v
            .map(|(x, y)| [x, y]) // Convert each tuple into an array
            .collect();
        flux_plot
    }

    pub fn get_calc_end(&mut self, gas_type: GasType) -> f64 {
        *self.cycles[self.index.count]
            .calc_range_end
            .get(&gas_type)
            .unwrap_or(&0.0)
    }
    pub fn get_calc_start(&mut self, gas_type: GasType) -> f64 {
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
        cycle.prepare_plot_data();
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
        Self {
            cycles,
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
}

#[allow(clippy::too_many_arguments)]
fn create_polygon(
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
    color: Color32,
    stroke: Color32,
    id: &str,
    idd: Id,
) -> Polygon {
    Polygon::new(PlotPoints::from(vec![
        [start_x, min_y],
        [start_x, max_y],
        [end_x, max_y],
        [end_x, min_y],
        [start_x, min_y], // Close the polygon
    ]))
    .name(id)
    .fill_color(color)
    .stroke(Stroke::new(2.0, stroke))
    .allow_hover(true)
}
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        println!(
            "calc: {}, meas: {} ,i: {}",
            self.cycles[self.index.count].calc_dt_v.len(),
            self.cycles[self.index.count].measurement_dt_v.len(),
            self.index.count,
            // self.index.count
        );
        println!("{}", self.calc_range_end.len());
        let mut gas_type = GasType::CH4;
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                egui::widgets::global_theme_preference_buttons(ui);
                ui.add_space(16.0);
                use egui::special_emojis::GITHUB;
                ui.hyperlink_to(
                    format!("{GITHUB} egui_plot on GitHub"),
                    "https://github.com/emilk/egui_plot",
                );
            });
        });
        egui::SidePanel::left("my_left_panel").show(ctx, |ui| {
            let lag = format!("lag s: {}", self.cycles[self.index.count].lag_s);
            ui.label(lag);
            let total_r = match self.cycles[self.index.count].calc_r.get(&gas_type) {
                Some(r) => format!("calc r: {:.6}", r),
                None => "calc r: N/A".to_string(), // Handle missing data
            };

            ui.label(total_r);

            let measurement_r = match self.cycles[self.index.count].measurement_r.get(&gas_type) {
                Some(r) => format!("measurement r: {:.6}", r),
                None => "calc r: N/A".to_string(), // Handle missing data
            };
            // );
            ui.label(measurement_r);

            let flux = match self.cycles[self.index.count].flux.get(&gas_type) {
                Some(r) => format!("flux : {:.6}", r),
                None => "calc r: N/A".to_string(), // Handle missing data
            };
            // let flux = format!("flux: {:.6}", self.cycles[self.index.count].flux);
            ui.label(flux);

            let datetime = format!("datetime: {}", self.cycles[self.index.count].start_time);
            ui.label(datetime);
        });
        // egui::SidePanel::left("my_left_panel").show(ctx, |ui| {});

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.style_mut().text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(14.0, eframe::epaint::FontFamily::Proportional),
            );

            // let gas_plot = self.create_gas_plot();
            // let lag_plot = self.create_lag_plot();

            let mut prev_clicked = false;
            let mut next_clicked = false;
            let mut highest_r = false;
            let mut find_lag = false;
            let mut find_bad = false;

            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                prev_clicked = ui.add(egui::Button::new("Prev measurement")).clicked();
                next_clicked = ui.add(egui::Button::new("Next measurement")).clicked();
            });

            ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                highest_r = ui.add(egui::Button::new("Find r")).clicked();
                find_lag = ui.add(egui::Button::new("Find lag")).clicked();
                find_bad = ui.add(egui::Button::new("Find bad")).clicked();
            });

            ui.add_space(10.);

            if find_bad {
                self.find_bad_measurement(gas_type);
            }

            if find_lag {
                self.cycles[self.index.count].get_peak_datetime(gas_type);
                self.update_cycle(self.index.count);
            }

            if highest_r {
                self.cycles[self.index.count].find_highest_r_window_disp(gas_type);
            }

            if prev_clicked && self.index.count > 0 {
                self.index.decrement();
                self.update_cycle(self.index.count);
            }

            if next_clicked && self.index.count + 1 < self.cycles.len() {
                self.index.increment();
                self.update_cycle(self.index.count);
            }

            let mut lag_s = self.cycles[self.index.count].lag_s;

            let drag_panel_width = 40.;
            let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
            let calc_area_adjust_color = Color32::from_rgba_unmultiplied(64, 242, 106, 50);
            let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
            let close_line_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
            let left_id = Id::new("left_test");
            let main_id = Id::new("main_area");
            let right_id = Id::new("right_area");
            let left_id2 = Id::new("left_test2");
            let main_id2 = Id::new("main_area2");
            let right_id2 = Id::new("right_area2");

            // let main_polygon = create_polygon(
            //     self.get_calc_start(gas_type) + drag_panel_width,
            //     self.get_calc_end(gas_type) - drag_panel_width,
            //     self.get_min_y(&gas_type),
            //     self.get_max_y(&gas_type),
            //     calc_area_color,
            //     calc_area_stroke_color,
            //     "Move",
            //     main_id,
            // );

            // let left_polygon = create_polygon(
            //     self.get_calc_start(gas_type),
            //     self.get_calc_end(gas_type) + drag_panel_width,
            //     self.get_min_y(&gas_type),
            //     self.get_max_y(&gas_type),
            //     calc_area_adjust_color,
            //     calc_area_stroke_color,
            //     "Extend left",
            //     left_id,
            // );

            // let right_polygon = create_polygon(
            //     self.get_calc_end(gas_type) - drag_panel_width,
            //     self.get_calc_end(gas_type),
            //     self.get_min_y(&gas_type),
            //     self.get_max_y(&gas_type),
            //     calc_area_adjust_color,
            //     calc_area_stroke_color,
            //     "Extend right",
            //     right_id,
            // );
            let x_axis_formatter_gas =
                |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
                    // let timestamp = x as i64;
                    let timestamp = mark.value as i64; // Extract value from GridMark
                    DateTime::from_timestamp(timestamp, 0)
                        .map(|dt| dt.format("%H:%M").to_string())
                        .unwrap_or_else(|| "Invalid".to_string())
                };
            // let gas_plot = self.create_gas_plot();
            // let lag_plot = self.create_lag_plot();
            let gas_plot = Plot::new("Data plot")
                // .x_grid_spacer(self.x_grid_spacer_gas())
                .x_axis_formatter(x_axis_formatter_gas)
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .y_axis_label("CH4")
                .legend(Legend::default().position(Corner::LeftTop));

            let x_max: f64 = self.start_time_idx + self.open_offset + lag_s;
            let x_close = self.start_time_idx + self.close_offset + lag_s;
            let dragged = ui.response().dragged_by(PointerButton::Primary);

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
            let max_vl = VLine::new(x_max)
                .name("Lagtime")
                .width(2.0)
                .allow_hover(true);

            let close_vl = VLine::new(x_close)
                .name("Close time")
                .width(2.0)
                .allow_hover(true);

            ui.horizontal(|ui| {
                gas_plot.show(ui, |plot_ui| {
                    self.render_gas_plot(plot_ui, gas_type, lag_s);
                    plot_ui.polygon(main_polygon);
                    plot_ui.polygon(left_polygon);
                    plot_ui.polygon(right_polygon);
                    plot_ui.vline(max_vl);
                    plot_ui.vline(close_vl);
                    if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                        let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                        // Handle dragging
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
                            // self.cycles[self.index.count]
                            //     .calc_range_end
                            //     .get(&gas_type)
                            //     .unwrap(),
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
                            x_max - 20.,
                            x_max + 20.,
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
                            // self.cycles[self.index.count]
                            //     .calc_range_start
                            //     .get(&gas_type)
                            //     .unwrap() = self.close_idx;
                            // self.cycles[self.index.count]
                            //     .calc_range_end
                            //     .get(&gas_type)
                            //     .unwrap() = self.open_idx;
                        }
                        if inside_left {
                            handle_drag_polygon(plot_ui, self, true, &gas_type);
                            // self.cycles[self.index].get_calc_data();
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
                            lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                            lag_s = lag_s.round();
                            self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                            self.open_idx = self.start_time_idx + self.open_offset + lag_s;
                            self.cycles[self.index.count].lag_s = lag_s;
                            self.cycles[self.index.count].get_measurement_data(gas_type);
                            self.cycles[self.index.count].calculate_measurement_r(gas_type);
                            self.cycles[self.index.count].find_highest_r_window_disp(gas_type);
                            self.cycles[self.index.count].calculate_flux(gas_type);
                            self.update_cycle(self.index.count);
                            // println!("{:?}", self.cycles[self.index].calc_range_start.get(&gas_type).unwrap())
                            // self.update_cycle(self.index);
                            // if self.open_idx == self.cycles[self.index].calc_range_end.get(&gas_type).unwrap() {
                            //     self.cycles[self.index].calc_range_start.get(&gas_type).unwrap() -= drag_delta.x as f64;
                            // }
                        }
                        limit_to_bounds(plot_ui, self, &gas_type)
                    }
                });
            });

            gas_type = GasType::CO2;
            let co2_plot = Plot::new("CO2 plot")
                // .x_grid_spacer(self.x_grid_spacer_gas())
                .x_axis_formatter(x_axis_formatter_gas)
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .y_axis_label("CO2")
                .legend(Legend::default().position(Corner::LeftTop));

            co2_plot.show(ui, |plot_ui| {
                // plot_ui.points(
                //     Points::new(PlotPoints::from(points))
                //         .name(format!("{}", gas_type))
                //         .shape(MarkerShape::Circle)
                //         .radius(2.),
                // );
                let x_max: f64 = self.start_time_idx + self.open_offset + lag_s;
                let main_polygon = create_polygon(
                    self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        + drag_panel_width,
                    self.cycles[self.index.count]
                        .calc_range_end
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        - drag_panel_width,
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_color,
                    calc_area_stroke_color,
                    "Move",
                    main_id2,
                );

                let left_polygon = create_polygon(
                    *self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0),
                    self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        + drag_panel_width,
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend left",
                    left_id2,
                );

                let right_polygon = create_polygon(
                    self.get_calc_end(gas_type) - drag_panel_width,
                    self.get_calc_end(gas_type),
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend right",
                    right_id2,
                );
                let x_axis_formatter_gas =
                    |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
                        // let timestamp = x as i64;
                        let timestamp = mark.value as i64; // Extract value from GridMark
                        DateTime::from_timestamp(timestamp, 0)
                            .map(|dt| dt.format("%H:%M").to_string())
                            .unwrap_or_else(|| "Invalid".to_string())
                    };

                let x_close = self.start_time_idx + self.close_offset + lag_s;
                let max_vl = VLine::new(x_max)
                    .name("Lagtime")
                    .width(2.0)
                    .allow_hover(true);

                let close_vl = VLine::new(x_close)
                    .name("Close time")
                    .width(2.0)
                    .allow_hover(true);

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
                            .radius(2.),
                    );
                }
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);
                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                    // Handle dragging
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
                        self.cycles[self.index.count]
                            .calc_range_start
                            .get(&gas_type)
                            .unwrap_or(&0.0)
                            + drag_panel_width,
                        self.cycles[self.index.count]
                            .calc_range_end
                            .get(&gas_type)
                            .unwrap_or(&0.0)
                            - drag_panel_width,
                        self.get_min_y(&gas_type),
                        self.get_max_y(&gas_type),
                    );
                    let inside_lag = is_inside_polygon(
                        pointer_pos,
                        x_max - 20.,
                        x_max + 20.,
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
                        // self.cycles[self.index].get_calc_data();
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
                        lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                        lag_s = lag_s.round();
                        self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                        self.open_idx = self.start_time_idx + self.open_offset + lag_s;
                        self.cycles[self.index.count].lag_s = lag_s;
                        self.cycles[self.index.count].get_measurement_data(gas_type);
                        self.cycles[self.index.count].calculate_measurement_r(gas_type);
                        self.cycles[self.index.count].find_highest_r_window_disp(gas_type);
                        self.cycles[self.index.count].calculate_flux(gas_type);
                        self.update_cycle(self.index.count);
                        // println!("{:?}", self.cycles[self.index].calc_range_start.get(&gas_type).unwrap())
                        // self.update_cycle(self.index);
                        // if self.open_idx == self.cycles[self.index].calc_range_end.get(&gas_type).unwrap() {
                        //     self.cycles[self.index].calc_range_start.get(&gas_type).unwrap() -= drag_delta.x as f64;
                        // }
                    }
                    limit_to_bounds(plot_ui, self, &gas_type)
                }
            });
            gas_type = GasType::H2O;
            let h2o_plot = Plot::new("H2O plot")
                // .x_grid_spacer(self.x_grid_spacer_gas())
                .x_axis_formatter(x_axis_formatter_gas)
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .y_axis_label("H2O")
                .legend(Legend::default().position(Corner::LeftTop));

            h2o_plot.show(ui, |plot_ui| {
                // plot_ui.points(
                //     Points::new(PlotPoints::from(points))
                //         .name(format!("{}", gas_type))
                //         .shape(MarkerShape::Circle)
                //         .radius(2.),
                // );
                let x_max: f64 = self.start_time_idx + self.open_offset + lag_s;
                let main_polygon = create_polygon(
                    self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        + drag_panel_width,
                    self.cycles[self.index.count]
                        .calc_range_end
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        - drag_panel_width,
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_color,
                    calc_area_stroke_color,
                    "Move",
                    main_id2,
                );

                let left_polygon = create_polygon(
                    *self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0),
                    self.cycles[self.index.count]
                        .calc_range_start
                        .get(&gas_type)
                        .unwrap_or(&0.0)
                        + drag_panel_width,
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend left",
                    left_id2,
                );

                let right_polygon = create_polygon(
                    self.get_calc_end(gas_type) - drag_panel_width,
                    self.get_calc_end(gas_type),
                    self.get_min_y(&gas_type),
                    self.get_max_y(&gas_type),
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend right",
                    right_id2,
                );
                let x_axis_formatter_gas =
                    |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
                        // let timestamp = x as i64;
                        let timestamp = mark.value as i64; // Extract value from GridMark
                        DateTime::from_timestamp(timestamp, 0)
                            .map(|dt| dt.format("%H:%M").to_string())
                            .unwrap_or_else(|| "Invalid".to_string())
                    };

                let x_close = self.start_time_idx + self.close_offset + lag_s;
                let max_vl = VLine::new(x_max)
                    .name("Lagtime")
                    .width(2.0)
                    .allow_hover(true);

                let close_vl = VLine::new(x_close)
                    .name("Close time")
                    .width(2.0)
                    .allow_hover(true);

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
                            .radius(2.),
                    );
                }
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);
                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                    // Handle dragging
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
                        self.cycles[self.index.count]
                            .calc_range_start
                            .get(&gas_type)
                            .unwrap_or(&0.0)
                            + drag_panel_width,
                        self.cycles[self.index.count]
                            .calc_range_end
                            .get(&gas_type)
                            .unwrap_or(&0.0)
                            - drag_panel_width,
                        self.get_min_y(&gas_type),
                        self.get_max_y(&gas_type),
                    );
                    let inside_lag = is_inside_polygon(
                        pointer_pos,
                        x_max - 20.,
                        x_max + 20.,
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
                        // self.cycles[self.index].get_calc_data();
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
                        lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                        lag_s = lag_s.round();
                        self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                        self.open_idx = self.start_time_idx + self.open_offset + lag_s;
                        self.cycles[self.index.count].lag_s = lag_s;
                        self.cycles[self.index.count].get_measurement_data(gas_type);
                        self.cycles[self.index.count].calculate_measurement_r(gas_type);
                        self.cycles[self.index.count].find_highest_r_window_disp(gas_type);
                        self.cycles[self.index.count].calculate_flux(gas_type);
                        self.update_cycle(self.index.count);
                        // println!("{:?}", self.cycles[self.index].calc_range_start.get(&gas_type).unwrap())
                        // self.update_cycle(self.index);
                        // if self.open_idx == self.cycles[self.index].calc_range_end.get(&gas_type).unwrap() {
                        //     self.cycles[self.index].calc_range_start.get(&gas_type).unwrap() -= drag_delta.x as f64;
                        // }
                    }
                    limit_to_bounds(plot_ui, self, &gas_type)
                }
            });
            let lag_plot = Plot::new("Lag plot")
                // .x_grid_spacer(self.x_grid_spacer_lag())
                // .x_axis_formatter(self.x_axis_formatter_lag())
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .y_axis_label("Lag (s)")
                .legend(Legend::default().position(Corner::LeftTop));

            // ui.horizontal(|ui| {
            // });
            let flux_plot = Plot::new("Flux plot")
                // .x_grid_spacer(self.x_grid_spacer_lag())
                // .x_axis_formatter(self.x_axis_formatter_lag())
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .y_axis_label("Lag (s)")
                .legend(Legend::default().position(Corner::LeftTop));
            ui.horizontal(|ui| {
                flux_plot.show(ui, |plot_ui| {
                    self.render_flux_plot(plot_ui, GasType::CH4);
                });
                lag_plot.show(ui, |plot_ui| {
                    self.render_lag_plot(plot_ui);
                });
            });
        });
    }
}

impl MyApp {
    // fn render_lag_plot(&self, plot_ui: &mut egui_plot::PlotUi) {
    //     let points: Vec<[f64; 2]> = self.lag_plot.clone();
    //
    //     plot_ui.points(
    //         Points::new(PlotPoints::from(points))
    //             .name("Lag")
    //             .shape(MarkerShape::Circle)
    //             .radius(2.),
    //     );
    // }
    fn render_gas_plot(&self, plot_ui: &mut egui_plot::PlotUi, gas_type: GasType, lag_s: f64) {
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
                    .radius(2.),
            );
        }
    }
    fn render_flux_plot(&mut self, plot_ui: &mut egui_plot::PlotUi, gas_type: GasType) {
        let lag_points = self.create_flux_plot(&gas_type); // Clone to avoid borrowing issues
        let mut selected_point: Option<[f64; 2]> = Some(lag_points[self.index.count]); // Store the selected point
        let plot_points = PlotPoints::from(lag_points.clone());

        plot_ui.points(
            Points::new(plot_points)
                .name("Flux")
                .shape(MarkerShape::Circle)
                .radius(2.),
        );

        // First, get pointer position
        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            if let Some(closest) = self.find_closest_point(pointer_pos, &lag_points) {
                // Check if the user clicked
                if plot_ui.response().clicked() {
                    for (i, c) in self.cycles.iter().enumerate() {
                        if c.start_time.timestamp() as f64 == closest[0] {
                            self.index.set(i);
                        }
                    }
                    self.update_cycle(self.index.count);
                }

                // Highlight the selected point
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![closest]))
                        .name("Selected Point")
                        .shape(MarkerShape::Diamond)
                        .radius(5.0) // Larger marker for highlight
                        .color(egui::Color32::GREEN), // Highlighted color
                );
                if plot_ui.response().clicked() {
                    for (i, c) in self.cycles.iter().enumerate() {
                        if c.start_time.timestamp() as f64 == closest[0] {
                            self.index.set(i);
                        }
                    }
                    selected_point = Some(closest);
                    self.update_cycle(self.index.count);
                }
            }
            if let Some(selected) = selected_point {
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![selected]))
                        .name("Selected Point")
                        .shape(MarkerShape::Diamond)
                        .radius(5.0) // Larger marker for highlight
                        .color(egui::Color32::YELLOW), // Highlighted color
                );
            }
        }
    }
    fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
        let mut selected_point: Option<[f64; 2]> = Some(self.lag_plot[self.index.count]); // Store the selected point
        let lag_points = self.lag_plot.clone(); // Clone to avoid borrowing issues
        let plot_points = PlotPoints::from(lag_points.clone());

        plot_ui.points(
            Points::new(plot_points)
                .name("Lag")
                .shape(MarkerShape::Circle)
                .radius(2.),
        );

        // First, get pointer position
        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            if let Some(closest) = self.find_closest_point(pointer_pos, &lag_points) {
                // Check if the user clicked
                if plot_ui.response().clicked() {
                    for (i, c) in self.cycles.iter().enumerate() {
                        if c.start_time.timestamp() as f64 == closest[0] {
                            self.index.set(i);
                        }
                    }
                    self.update_cycle(self.index.count);
                }

                // Highlight the selected point
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![closest]))
                        .name("Selected Point")
                        .shape(MarkerShape::Diamond)
                        .radius(5.0) // Larger marker for highlight
                        .color(egui::Color32::GREEN), // Highlighted color
                );
                if plot_ui.response().clicked() {
                    for (i, c) in self.cycles.iter().enumerate() {
                        if c.start_time.timestamp() as f64 == closest[0] {
                            self.index.set(i);
                        }
                    }
                    selected_point = Some(closest);
                    self.update_cycle(self.index.count);
                }
            }
            if let Some(selected) = selected_point {
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![selected]))
                        .name("Selected Point")
                        .shape(MarkerShape::Diamond)
                        .radius(5.0) // Larger marker for highlight
                        .color(egui::Color32::YELLOW), // Highlighted color
                );
            }
        }
    }

    // Helper function to find the closest point to the cursor
    fn find_closest_point(
        &self,
        pointer: egui_plot::PlotPoint,
        points: &Vec<[f64; 2]>,
    ) -> Option<[f64; 2]> {
        let x_range = self
            .lag_plot
            .iter()
            .map(|p| p[0])
            .fold(f64::INFINITY, f64::min)
            ..self
                .lag_plot
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
        let y_range = self
            .lag_plot
            .iter()
            .map(|p| p[1])
            .fold(f64::INFINITY, f64::min)
            ..self
                .lag_plot
                .iter()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max);
        // if let Some(pointer) = pointer_pos {
        let norm_x = |x: f64| (x - x_range.start) / (x_range.end - x_range.start);
        let norm_y = |y: f64| (y - y_range.start) / (y_range.end - y_range.start);
        points
            .iter()
            .min_by(|a, b| {
                let dist_a = ((norm_x(a[0]) - norm_x(pointer.x)).powi(2)
                    + (norm_y(a[1]) - norm_y(pointer.y)).powi(2))
                .sqrt();
                let dist_b = ((norm_x(b[0]) - norm_x(pointer.x)).powi(2)
                    + (norm_y(b[1]) - norm_y(pointer.y)).powi(2))
                .sqrt();
                // let dist_a =
                //     ((a[0] - pointer.x).powi(2) + (a[1] - pointer.y).powi(2)).sqrt();
                // let dist_b =
                //     ((b[0] - pointer.x).powi(2) + (b[1] - pointer.y).powi(2)).sqrt();
                dist_a.partial_cmp(&dist_b).unwrap()
            })
            .copied()

        // points
        //     .iter()
        //     .min_by(|a, b| {
        //         let dist_a = ((a[0] - pointer.x).powi(2) + (a[1] - pointer.y).powi(2)).sqrt();
        //         let dist_b = ((b[0] - pointer.x).powi(2) + (b[1] - pointer.y).powi(2)).sqrt();
        //         dist_a.partial_cmp(&dist_b).unwrap()
        //     })
        //     .copied()
    }

    fn create_gas_plot(&self) -> Plot {
        Plot::new("Data plot")
            // .x_grid_spacer(self.x_grid_spacer_gas())
            // .x_axis_formatter(self.x_axis_formatter_gas())
            .allow_drag(false)
            .width(600.)
            .height(350.)
            .y_axis_label("CH4")
            .legend(Legend::default().position(Corner::LeftTop))
    }

    fn create_lag_plot(&self) -> Plot {
        Plot::new("Lag plot")
            // .x_grid_spacer(self.x_grid_spacer_lag())
            // .x_axis_formatter(self.x_axis_formatter_lag())
            .allow_drag(false)
            .width(600.)
            .height(350.)
            .y_axis_label("Lag (s)")
            .legend(Legend::default().position(Corner::LeftTop))
    }

    fn find_bad_measurement(&mut self, gas_type: GasType) {
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
        self.update_cycle(self.index.count);
    }
}

fn is_inside_polygon(
    point: egui_plot::PlotPoint,
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
) -> bool {
    point.x >= start_x && point.x <= end_x && point.y >= min_y && point.y <= max_y
}
fn limit_to_bounds(plot_ui: &mut PlotUi, app: &mut MyApp, gas_type: &GasType) {
    let calc_area_range = app.get_calc_end(*gas_type) - app.get_calc_start(*gas_type);
    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
    let at_min_area = calc_area_range as i64 == app.min_calc_area_range as i64;
    // let after_close = app.cycles[app.index.count].calc_range_start.get(&gas_type).unwrap() >= app.close_idx;
    // let before_open = app.cycles[app.index.count].calc_range_end.get(&gas_type).unwrap() <= app.open_idx;
    // let in_bounds = after_close && before_open;
    // let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let at_start = app.get_calc_start(*gas_type) <= app.close_idx;
    let at_end = app.get_calc_end(*gas_type) >= app.open_idx;
    let positive_drag = drag_delta.x > 0.;
    let negative_drag = drag_delta.x < 0.;

    // println!("{}", drag_delta);
    if at_start && positive_drag && !at_min_area {
        app.increment_calc_start(*gas_type, drag_delta.x as f64);
        return;
    }

    if at_end && negative_drag && !at_min_area {
        app.increment_calc_end(*gas_type, drag_delta.x as f64);
        return;
    }

    if app.get_calc_start(*gas_type) < app.close_idx {
        let diff = (app.get_calc_start(*gas_type) - app.close_idx).abs();
        app.set_calc_start(*gas_type, app.close_idx);
        if app.get_calc_end(*gas_type) < app.open_idx {
            app.increment_calc_end(*gas_type, diff);
            // app.cycles[app.index.count]
            //     .calc_range_end
            //     .get(&gas_type)
            //     .unwrap() += diff;
        }
        return;
    }
    if app.get_calc_end(*gas_type) > app.open_idx {
        let diff = (app.cycles[app.index.count]
            .calc_range_end
            .get(&gas_type)
            .unwrap_or(&0.0)
            - app.open_idx)
            .abs();
        app.set_calc_end(*gas_type, app.open_idx);
        if app.get_calc_start(*gas_type) > app.close_idx {
            app.decrement_calc_start(*gas_type, diff);
            // app.cycles[app.index.count]
            //     .calc_range_start
            //     .get(&gas_type)
            //     .unwrap() -= diff;
        }
        return;
    }
}
fn handle_drag_polygon(plot_ui: &mut PlotUi, app: &mut MyApp, is_left: bool, gas_type: &GasType) {
    let delta = plot_ui.pointer_coordinate_drag_delta();
    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let calc_area_range = app.cycles[app.index.count]
        .calc_range_end
        .get(&gas_type)
        .unwrap_or(&0.0)
        - app.cycles[app.index.count]
            .calc_range_start
            .get(&gas_type)
            .unwrap_or(&0.0);
    // println!("Dragging.");
    // println!("{}", delta);

    if is_left && app.get_calc_start(*gas_type) > app.close_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range <= app.min_calc_area_range && delta.x > 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.decrement_calc_start(*gas_type, diff);
            return;
        }
        app.increment_calc_start(*gas_type, delta.x as f64);
        // app.cycles[app.index.count]
        //     .calc_range_start
        //     .get(&gas_type)
        //     .unwrap() += delta.x as f64; // Adjust left boundary
    } else if !is_left && app.get_calc_end(*gas_type) < app.open_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range < app.min_calc_area_range && delta.x < 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.increment_calc_end(*gas_type, diff);
            // app.cycles[app.index.count]
            //     .calc_range_end
            //     .get(&gas_type)
            //     .unwrap() += diff;
            return;
        }
        app.increment_calc_end(*gas_type, delta.x as f64);
        // app.cycles[app.index.count]
        //     .calc_range_end
        //     .get(&gas_type)
        //     .unwrap() += delta.x as f64; // Adjust right boundary
    }
}
// fn create_plot(&cycle) ->
