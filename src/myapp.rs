use chrono::{DateTime, Utc};
use std::any::Any;

use crate::structs::{Cycle, CycleBuilder};
use eframe::egui::{
    show_tooltip_at, Button, Color32, Id, PointerButton, Pos2, Rect, Sense, Stroke, Ui,
};
use egui_plot::{
    AxisHints, ClosestElem, Corner, GridInput, GridMark, HLine, Legend, Line, MarkerShape, Plot,
    PlotItem, PlotPoint, PlotPoints, PlotUi, Points, Polygon, VLine,
};

#[derive(Default)]
pub struct MyApp {
    cycles: Vec<Cycle>,
    gas_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
    lag_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
    lag_idx: f64,            // Add a vecxy tor of values to your struct
    close_idx: f64,
    open_offset: f64,
    close_offset: f64,
    open_idx: f64,
    start_time_idx: f64,
    calc_range_start: f64,
    calc_range_end: f64,
    max_y: f64,
    min_y: f64,
    min_calc_area_range: f64,
    index: usize,
    lag_vec: Vec<f64>,
    start_vec: Vec<f64>,
}

impl MyApp {
    pub fn update_cycle(&mut self, index: usize) {
        let cycle = &self.cycles[index];
        self.gas_plot = cycle
            .dt_v_as_float()
            .iter()
            .copied()
            .zip(cycle.gas_v.iter().copied())
            .map(|(x, y)| [x, y])
            .collect();

        self.lag_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
        self.close_idx = cycle.close_time.timestamp() as f64 + cycle.lag_s;
        self.open_idx =
            cycle.start_time.timestamp() as f64 + cycle.open_offset as f64 + cycle.lag_s;
        self.open_offset = cycle.open_offset as f64;
        self.close_offset = cycle.close_offset as f64;
        self.start_time_idx = cycle.start_time.timestamp() as f64;
        self.calc_range_end = cycle.calc_range_end;
        self.calc_range_start = cycle.calc_range_start;
        self.min_y = cycle
            .gas_v
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);
        self.max_y = cycle
            .gas_v
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);
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
    }

    pub fn new(cycles: Vec<Cycle>) -> Self {
        let cycle = &cycles[0];
        let gas_plot: Vec<[f64; 2]> = cycle
            .dt_v_as_float()
            .iter()
            .copied() // Copy each f64 from the iterator
            .zip(cycle.gas_v.iter().copied()) // Iterate and copy gas_v
            .map(|(x, y)| [x, y]) // Convert each tuple into an array
            .collect();
        let lag_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
        let close_idx = cycle.close_time.timestamp() as f64 + cycle.lag_s;
        let open_idx = cycle.open_time.timestamp() as f64 + cycle.lag_s;
        let open_offset = cycle.open_offset as f64;
        let close_offset = cycle.close_offset as f64;
        let start_time_idx = cycle.start_time.timestamp() as f64;
        let calc_range_end = cycle.calc_range_end;
        let calc_range_start = cycle.calc_range_start;
        let min_y = cycle
            .gas_v
            .iter()
            .cloned()
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);
        let max_y = cycle
            .gas_v
            .iter()
            .cloned()
            // .rev()
            // .take(120)
            .filter(|v| !v.is_nan())
            .fold(f64::NEG_INFINITY, f64::max);

        // let max_y;
        // let min_y;
        let min_calc_area_range = 120.;
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
        Self {
            cycles,
            gas_plot,
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
            index: 0,
            lag_vec,
            start_vec,
            lag_plot,
        }
    }
}

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
    .id(idd)
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut plot_rect = None;
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            // The top panel is often a good place for a menu bar:

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
        // for (_text_style, font_id) in style.text_styles.iter_mut() {
        //     font_id.size = 24 // whatever size you want here
        // }
        egui::SidePanel::left("my_left_panel").show(ctx, |ui| {
            let lag = format!("lag s: {}", self.cycles[self.index].lag_s);
            ui.label(lag);
            let total_r = format!("calc r: {:.6}", self.cycles[self.index].calc_r);
            ui.label(total_r);
            let measurement_r = format!(
                "measurement_r: {:.6}",
                self.cycles[self.index].measurement_r
            );
            ui.label(measurement_r);
            let flux = format!("flux: {:.6}", self.cycles[self.index].flux);
            ui.label(flux);
            let datetime = format!("datetime: {}", self.cycles[self.index].start_time);
            ui.label(datetime);
        });
        let mut threshold = 20.;
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.style_mut().text_styles.insert(
                egui::TextStyle::Button,
                egui::FontId::new(14.0, eframe::epaint::FontFamily::Proportional),
            );
            let x_axis_formatter_gas =
                |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
                    // let timestamp = x as i64;
                    let timestamp = mark.value as i64; // Extract value from GridMark
                    DateTime::from_timestamp(timestamp, 0)
                        .map(|dt| dt.format("%H:%M").to_string())
                        .unwrap_or_else(|| "Invalid".to_string())
                };
            let x_axis_formatter_lag =
                |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
                    // let timestamp = x as i64;
                    let timestamp = mark.value as i64; // Extract value from GridMark
                    DateTime::from_timestamp(timestamp, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "Invalid".to_string())
                };
            // let x_axis_spacer =
            //     |mark: GridInput, _range: &std::ops::RangeInclusive<f64>| -> String {
            //         // let timestamp = x as i64;
            //         // let timestamp = mark.value as i64; // Extract value from GridMark
            //         DateTime::from_timestamp(timestamp, 0)
            //             .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            //             .unwrap_or_else(|| "Invalid".to_string())
            //     };
            // ctx.set_pixels_per_point(1.1);

            let x_grid_spacer_gas = |range: GridInput| -> Vec<GridMark> {
                let (min, max) = range.bounds;
                let step = 300.0;
                let mut grid_marks = Vec::new();
                let mut current = min;

                // Generate grid marks at intervals of `step`
                while current <= max {
                    grid_marks.push(GridMark {
                        value: current,  // Set the full range
                        step_size: step, // Keep step size consistent
                    });

                    current += step; // Move to next tick position
                }
                grid_marks
            };
            let x_grid_spacer_lag = |range: GridInput| -> Vec<GridMark> {
                let (min, max) = range.bounds;
                let step = 691200.0;
                let mut grid_marks = Vec::new();
                let mut current = min;

                // Generate grid marks at intervals of `step`
                while current <= max {
                    grid_marks.push(GridMark {
                        value: current,  // Set the full range
                        step_size: step, // Keep step size consistent
                    });

                    current += step; // Move to next tick position
                }
                println!("gridmark count: {}", grid_marks.len());
                grid_marks
            };

            let gas_plot = Plot::new("Data plot")
                // .x_grid_spacer(x_grid_spacer_gas)
                .x_axis_formatter(x_axis_formatter_gas) // Custom date formatting
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .legend(Legend::default().position(Corner::LeftTop));

            let lag_plot = Plot::new("Lag plot")
                // .x_grid_spacer(x_grid_spacer_lag)
                .x_axis_formatter(x_axis_formatter_lag) // Custom date formatting
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .legend(Legend::default().position(Corner::LeftTop));

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
                let mut idx = self.index + 1;

                while idx < self.cycles.len() - 1 && self.cycles[idx].measurement_r > 0.98 {
                    idx += 1;
                }

                self.index = idx;

                // Prevent overflow by clamping index
                if self.index >= self.cycles.len() {
                    self.index = self.cycles.len() - 1;
                }

                self.update_cycle(self.index);
            }

            if find_lag {
                self.cycles[self.index].get_peak_datetime();
            }

            if highest_r {
                self.cycles[self.index].find_highest_r_window_disp();
            }

            if prev_clicked && self.index > 0 {
                // Prevent underflow
                self.index -= 1;
                self.update_cycle(self.index);
            }

            if next_clicked && self.index + 1 < self.cycles.len() {
                // Ensure valid index
                self.index += 1;
                self.update_cycle(self.index);
            }

            // let mut lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
            let mut lag_s = self.cycles[self.index].lag_s;

            let calc_area_range =
                self.cycles[self.index].calc_range_end - self.cycles[self.index].calc_range_start;

            // let right_polygon_rect = Rect::from_min_max(
            //     Pos2::new(self.cycles[self.index].calc_range_start as f32, self.min_y as f32),
            //     Pos2::new((self.cycles[self.index].calc_range_start + 30.0) as f32, self.max_y as f32),
            // );
            let left_id = Id::new("left_test");
            let main_id = Id::new("main_area");
            let right_id = Id::new("right_area");

            let inner = gas_plot.show(ui, |plot_ui| {
                plot_ui.points(
                    Points::new(PlotPoints::from(self.gas_plot.clone()))
                        .name("CH4")
                        .shape(MarkerShape::Circle)
                        .radius(2.),
                );
                // let x_max = self..start_time
                //     + chrono::TimeDelta::seconds(self.open_offset + self.lag_s as i64);
                let x_max: f64 = self.start_time_idx + self.open_offset + lag_s;

                let x_close = self.start_time_idx + self.close_offset + lag_s;

                let max_vl = VLine::new(x_max)
                    .name("Lagtime")
                    .width(2.0)
                    .allow_hover(true);

                let close_vl = VLine::new(x_close)
                    .name("Close time")
                    .width(2.0)
                    .allow_hover(true);

                let drag_panel_width = 40.;
                let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
                let calc_area_adjust_color = Color32::from_rgba_unmultiplied(64, 242, 106, 50);
                let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
                let close_line_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);

                // let close_line = create_polygon(
                //     x_max - 5.,
                //     x_max + 5.,
                //     self.min_y,
                //     self.max_y,
                //     close_line_color,
                //     close_line_color,
                //     "Close_time",
                //     left_id,
                // );
                let main_polygon = create_polygon(
                    self.cycles[self.index].calc_range_start + drag_panel_width,
                    self.cycles[self.index].calc_range_end - drag_panel_width,
                    self.min_y,
                    self.max_y,
                    calc_area_color,
                    calc_area_stroke_color,
                    "Move",
                    main_id,
                );

                let left_polygon = create_polygon(
                    self.cycles[self.index].calc_range_start,
                    self.cycles[self.index].calc_range_start + drag_panel_width,
                    self.min_y,
                    self.max_y,
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend left",
                    left_id,
                );

                let right_polygon = create_polygon(
                    self.cycles[self.index].calc_range_end - drag_panel_width,
                    self.cycles[self.index].calc_range_end,
                    self.min_y,
                    self.max_y,
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend right",
                    right_id,
                );

                // Draw polygons
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);
                // plot_ui.polygon(close_line);
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);

                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                    // Handle dragging
                    let inside_left = is_inside_polygon(
                        pointer_pos,
                        self.cycles[self.index].calc_range_start,
                        self.cycles[self.index].calc_range_start + drag_panel_width,
                        self.min_y,
                        self.max_y,
                    );
                    let inside_right = is_inside_polygon(
                        pointer_pos,
                        self.cycles[self.index].calc_range_end - drag_panel_width,
                        self.cycles[self.index].calc_range_end,
                        self.min_y,
                        self.max_y,
                    );
                    let inside_main = is_inside_polygon(
                        pointer_pos,
                        self.cycles[self.index].calc_range_start + drag_panel_width,
                        self.cycles[self.index].calc_range_end - drag_panel_width,
                        self.min_y,
                        self.max_y,
                    );
                    let inside_lag = is_inside_polygon(
                        pointer_pos,
                        x_max - 20.,
                        x_max + 20.,
                        f64::NEG_INFINITY,
                        f64::INFINITY,
                    );

                    let after_close = self.cycles[self.index].calc_range_start >= self.close_idx;
                    let before_open = self.cycles[self.index].calc_range_end <= self.open_idx;
                    let in_bounds = after_close && before_open;
                    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
                    let at_start = self.cycles[self.index].calc_range_start == self.close_idx;
                    let at_end = self.cycles[self.index].calc_range_end == self.open_idx;
                    let range_len = self.cycles[self.index].calc_range_end
                        - self.cycles[self.index].calc_range_start;
                    let cycle_len = self.open_idx - self.close_idx;

                    if range_len > cycle_len {
                        self.cycles[self.index].calc_range_start = self.close_idx;
                        self.cycles[self.index].calc_range_end = self.open_idx;
                    }
                    if inside_left {
                        handle_drag_polygon(plot_ui, self, true);
                        // self.cycles[self.index].get_calc_data();
                        self.cycles[self.index].get_calc_data();
                        self.cycles[self.index].calculate_calc_r();
                        self.cycles[self.index].calculate_flux();
                    }
                    if inside_right {
                        handle_drag_polygon(plot_ui, self, false);
                        self.cycles[self.index].get_calc_data();
                        self.cycles[self.index].calculate_calc_r();
                        self.cycles[self.index].calculate_flux();
                    }

                    if inside_main && in_bounds && dragged && !at_start && !at_end {
                        self.cycles[self.index].calc_range_start += drag_delta.x as f64;
                        self.cycles[self.index].calc_range_end += drag_delta.x as f64;
                        self.cycles[self.index].get_calc_data();
                        self.cycles[self.index].calculate_calc_r();
                        self.cycles[self.index].calculate_flux();
                    }

                    if inside_lag && dragged && !inside_right {
                        println!("New range!");
                        self.lag_idx += drag_delta.x as f64;
                        lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                        lag_s = lag_s.round();
                        self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                        self.open_idx = self.start_time_idx + self.open_offset + lag_s;
                        self.cycles[self.index].lag_s = lag_s;
                        self.cycles[self.index].get_measurement_data();
                        self.cycles[self.index].calculate_measurement_r();
                        println!("New window");
                        self.cycles[self.index].find_highest_r_window_disp();
                        self.cycles[self.index].calculate_flux();
                        self.update_cycle(self.index);
                        // println!("{:?}", self.cycles[self.index].calc_range_start)
                        // self.update_cycle(self.index);
                        // if self.open_idx == self.cycles[self.index].calc_range_end {
                        //     self.cycles[self.index].calc_range_start -= drag_delta.x as f64;
                        // }
                    }
                    limit_to_bounds(plot_ui, self)
                }
            });
            let mut selected_point: Option<[f64; 2]> = Some(self.lag_plot[self.index]); // Store the selected point
            let lags = lag_plot.show(ui, |plot_ui| {
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

                let points = self.lag_plot.clone(); // Clone to avoid borrowing issues
                let plot_points = PlotPoints::from(points.clone());

                // First, get pointer position
                let pointer_pos = plot_ui.pointer_coordinate();

                // Render the points
                plot_ui.points(
                    Points::new(plot_points)
                        .name("Lag")
                        .shape(MarkerShape::Circle)
                        .radius(2.),
                );

                // Find and display the nearest point
                if let Some(pointer) = pointer_pos {
                    let norm_x = |x: f64| (x - x_range.start) / (x_range.end - x_range.start);
                    let norm_y = |y: f64| (y - y_range.start) / (y_range.end - y_range.start);
                    if let Some(closest) = points.iter().min_by(|a, b| {
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
                    }) {
                        // if let Some(closest) = points.iter().min_by(|a, b| {
                        //     let dist_a =
                        //         ((a[0] - pointer.x).powi(2) + (a[1] - pointer.y).powi(2)).sqrt();
                        //     let dist_b =
                        //         ((b[0] - pointer.x).powi(2) + (b[1] - pointer.y).powi(2)).sqrt();
                        //     dist_a.partial_cmp(&dist_b).unwrap()
                        // }) {
                        // Check if the user clicked
                        if plot_ui.response().clicked() {
                            for (i, c) in self.cycles.iter().enumerate() {
                                if c.start_time.timestamp() as f64 == closest[0] {
                                    self.index = i;
                                }
                            }
                            selected_point = Some(*closest);
                            self.update_cycle(self.index);
                        }
                    }
                    if let Some(selected) = selected_point {
                        plot_ui.points(
                            Points::new(PlotPoints::from(vec![selected]))
                                .name("Selected Point")
                                .shape(MarkerShape::Diamond)
                                .radius(5.0) // Larger marker for highlight
                                .color(egui::Color32::GREEN), // Highlighted color
                        );
                    }
                }
            });

            // let lags = lag_plot.show(ui, |plot_ui| {
            //     let pts = self.lag_plot.clone();
            //     let plot_points = PlotPoints::from(pts.clone());
            //     let response = plot_ui.points(
            //         Points::new(PlotPoints::from(plot_points))
            //             .name("Lag")
            //             .shape(MarkerShape::Circle)
            //             .radius(2.),
            //     );
            //     if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            //         println!("{}", pointer_pos.x);
            //     }
            //     if let Some(pointer) = plot_ui.pointer_coordinate() {
            //         let closest_point = pts.iter().min_by(|a, b| {
            //             let dist_a =
            //                 ((a[0] - pointer.x).powi(2) + (a[1] - pointer.y).powi(2)).sqrt();
            //             let dist_b =
            //                 ((b[0] - pointer.x).powi(2) + (b[1] - pointer.y).powi(2)).sqrt();
            //             dist_a.partial_cmp(&dist_b).unwrap()
            //         });
            //
            //         if let Some(closest) = closest_point {
            //             ui.label(format!(
            //                 "Nearest Point: ({:.2}, {:.2})",
            //                 closest[0], closest[1]
            //             ));
            //         }
            //     }
            // });
            // println!("{}", calc_area_range);
            plot_rect = Some(inner.response.rect);
        });
    }
}
// WARN: BETTER FUNCTION FOR FINDING NEAREST
// when zoomed the elements get activated pretty far away
fn _find_nearest_point(
    points: &Vec<[f64; 2]>,
    pos: egui_plot::PlotPoint,
    threshold: f64,
) -> Option<(f64, f64, f64)> {
    points
        .iter()
        .map(|&p| {
            let dist = (p[0] - pos.x).powi(2) + (p[1] - pos.y).powi(2);
            (p[0], p[1], dist)
        })
        .filter(|(_, _, dist)| *dist <= threshold.powi(2)) // Filter points within threshold
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap()) // Find the closest
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
fn limit_to_bounds(plot_ui: &mut PlotUi, app: &mut MyApp) {
    let calc_area_range =
        (app.cycles[app.index].calc_range_end - app.cycles[app.index].calc_range_start);
    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
    let at_min_area = calc_area_range as i64 == app.min_calc_area_range as i64;
    // let after_close = app.cycles[app.index].calc_range_start >= app.close_idx;
    // let before_open = app.cycles[app.index].calc_range_end <= app.open_idx;
    // let in_bounds = after_close && before_open;
    // let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let at_start = app.cycles[app.index].calc_range_start <= app.close_idx;
    let at_end = app.cycles[app.index].calc_range_end >= app.open_idx;
    let positive_drag = drag_delta.x > 0.;
    let negative_drag = drag_delta.x < 0.;
    let max_len = app.open_idx - app.close_idx;

    // println!("{}", drag_delta);
    if at_start && positive_drag && !at_min_area {
        app.cycles[app.index].calc_range_start += drag_delta.x as f64;
        return;
    }

    if at_end && negative_drag && !at_min_area {
        app.cycles[app.index].calc_range_end += drag_delta.x as f64;
        return;
    }

    if app.cycles[app.index].calc_range_start < app.close_idx {
        println!("4");
        let diff = (app.cycles[app.index].calc_range_start - app.close_idx).abs();
        app.cycles[app.index].calc_range_start = app.close_idx;
        if app.cycles[app.index].calc_range_end < app.open_idx {
            app.cycles[app.index].calc_range_end += diff;
        }
        return;
    }
    if app.cycles[app.index].calc_range_end > app.open_idx {
        let diff = (app.cycles[app.index].calc_range_end - app.open_idx).abs();
        app.cycles[app.index].calc_range_end = app.open_idx;
        if app.cycles[app.index].calc_range_start > app.close_idx {
            app.cycles[app.index].calc_range_start -= diff;
        }
        return;
    }
}
fn handle_drag_polygon(plot_ui: &mut PlotUi, app: &mut MyApp, is_left: bool) {
    let delta = plot_ui.pointer_coordinate_drag_delta();
    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let calc_area_range =
        app.cycles[app.index].calc_range_end - app.cycles[app.index].calc_range_start;
    // println!("Dragging.");
    // println!("{}", delta);

    if is_left && app.cycles[app.index].calc_range_start > app.close_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range <= app.min_calc_area_range && delta.x > 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.cycles[app.index].calc_range_start -= diff;
            return;
        }
        println!("moving left.");
        println!("b: {}", app.cycles[app.index].calc_range_start);
        app.cycles[app.index].calc_range_start += delta.x as f64; // Adjust left boundary
        println!("a: {}", app.cycles[app.index].calc_range_start);
    } else if !is_left && app.cycles[app.index].calc_range_end < app.open_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range < app.min_calc_area_range && delta.x < 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.cycles[app.index].calc_range_end += diff;
            return;
        }
        println!("moving right.");
        println!("a: {}", app.cycles[app.index].calc_range_start);
        app.cycles[app.index].calc_range_end += delta.x as f64; // Adjust right boundary
        println!("b: {}", app.cycles[app.index].calc_range_start);
    }
}
