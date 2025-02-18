use std::any::Any;

use crate::structs::Cycle;
use eframe::egui::{show_tooltip_at, Color32, Id, PointerButton, Pos2, Rect, Sense, Stroke, Ui};
use egui_plot::{
    ClosestElem, Corner, HLine, Legend, Line, MarkerShape, Plot, PlotItem, PlotPoint, PlotPoints,
    PlotUi, Points, Polygon, VLine,
};

#[derive(Default)]
pub struct MyApp {
    gas_plot: Vec<[f64; 2]>, // Add a vecxy tor of values to your struct
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
}

impl MyApp {
    pub fn new(cycle: &mut Cycle) -> Self {
        let gas_plot: Vec<[f64; 2]> = cycle
            .dt_v_as_float()
            .iter()
            .copied() // Copy each f64 from the iterator
            .zip(cycle.gas_v.iter().copied()) // Iterate and copy gas_v
            .map(|(x, y)| [x, y]) // Convert each tuple into an array
            .collect();
        let lag_idx = cycle.max_idx;
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
        Self {
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
        egui::SidePanel::left("my_left_panel").show(ctx, |ui| {
            ui.label("Hello World!");
        });
        let mut threshold = 20.;
        egui::CentralPanel::default().show(ctx, |ui| {
            let my_plot = Plot::new("My Plot")
                .allow_drag(false)
                .width(600.)
                .height(350.)
                .legend(Legend::default().position(Corner::LeftTop));

            let mut lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);

            let calc_area_range = self.calc_range_end - self.calc_range_start;

            // let right_polygon_rect = Rect::from_min_max(
            //     Pos2::new(self.calc_range_start as f32, self.min_y as f32),
            //     Pos2::new((self.calc_range_start + 30.0) as f32, self.max_y as f32),
            // );
            let left_polygon_rect = Rect::from_min_max(
                Pos2::new(self.calc_range_start as f32, self.min_y as f32),
                Pos2::new((self.calc_range_start + 30.0) as f32, self.max_y as f32),
            );
            let main_polygon_rect = Rect::from_min_max(
                Pos2::new(self.calc_range_start as f32, self.min_y as f32),
                Pos2::new((self.calc_range_end) as f32, self.max_y as f32),
            );
            let left_id = Id::new("left_test");
            let main_id = Id::new("main_area");
            let right_id = Id::new("right_area");

            let inner = my_plot.show(ui, |plot_ui| {
                plot_ui.points(
                    Points::new(PlotPoints::from(self.gas_plot.clone()))
                        .name("CH4")
                        .shape(MarkerShape::Circle)
                        .radius(2.),
                );
                let x_max = self.lag_idx;
                let x_close = self.close_idx;
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

                let main_polygon = create_polygon(
                    self.calc_range_start + drag_panel_width,
                    self.calc_range_end - drag_panel_width,
                    self.min_y,
                    self.max_y,
                    calc_area_color,
                    calc_area_stroke_color,
                    "Move",
                    main_id,
                );

                let left_polygon = create_polygon(
                    self.calc_range_start,
                    self.calc_range_start + drag_panel_width,
                    self.min_y,
                    self.max_y,
                    calc_area_adjust_color,
                    calc_area_stroke_color,
                    "Extend left",
                    left_id,
                );

                let right_polygon = create_polygon(
                    self.calc_range_end - drag_panel_width,
                    self.calc_range_end,
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
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);

                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                    // Handle dragging
                    let inside_left = is_inside_polygon(
                        pointer_pos,
                        self.calc_range_start,
                        self.calc_range_start + drag_panel_width,
                        self.min_y,
                        self.max_y,
                    );
                    let inside_right = is_inside_polygon(
                        pointer_pos,
                        self.calc_range_end - drag_panel_width,
                        self.calc_range_end,
                        self.min_y,
                        self.max_y,
                    );
                    let inside_main = is_inside_polygon(
                        pointer_pos,
                        self.calc_range_start + drag_panel_width,
                        self.calc_range_end - drag_panel_width,
                        self.min_y,
                        self.max_y,
                    );

                    let at_min_area = calc_area_range as i64 == self.min_calc_area_range as i64;
                    let after_close = self.calc_range_start >= self.close_idx;
                    let before_open = self.calc_range_end <= self.open_idx;
                    let in_bounds = after_close && before_open;
                    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
                    let at_start = self.calc_range_start == self.close_idx;
                    let at_end = self.calc_range_end == self.open_idx;
                    let range_len = self.calc_range_end - self.calc_range_start;
                    let cycle_len = self.open_idx - self.close_idx;

                    if range_len > cycle_len {
                        self.calc_range_start = self.close_idx;
                        self.calc_range_end = self.open_idx;
                    }
                    if inside_left {
                        handle_drag_polygon(plot_ui, self, true);
                    }
                    if inside_right {
                        handle_drag_polygon(plot_ui, self, false);
                    }

                    if inside_main && in_bounds && dragged && !at_start && !at_end {
                        self.calc_range_start += drag_delta.x as f64;
                        self.calc_range_end += drag_delta.x as f64;
                    }

                    let distance = (self.lag_idx - pointer_pos.x).abs();

                    // let dragging = plot_ui.response().dragged_by(PointerButton::Primary);

                    if distance <= threshold && dragged && !inside_right {
                        self.lag_idx += drag_delta.x as f64;
                        lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                        self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                        self.open_idx = self.start_time_idx + self.open_offset + lag_s;
                        // if self.open_idx == self.calc_range_end {
                        //     self.calc_range_start -= drag_delta.x as f64;
                        // }
                    }
                    limit_to_bounds(plot_ui, self)
                }
            });
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
    let calc_area_range = (app.calc_range_end - app.calc_range_start);
    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
    let at_min_area = calc_area_range as i64 == app.min_calc_area_range as i64;
    // let after_close = app.calc_range_start >= app.close_idx;
    // let before_open = app.calc_range_end <= app.open_idx;
    // let in_bounds = after_close && before_open;
    // let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let at_start = app.calc_range_start <= app.close_idx;
    let at_end = app.calc_range_end >= app.open_idx;
    let positive_drag = drag_delta.x > 0.;
    let negative_drag = drag_delta.x < 0.;
    let max_len = app.open_idx - app.close_idx;

    // println!("{}", drag_delta);
    if at_start && positive_drag && !at_min_area {
        // println!("1");
        app.calc_range_start += drag_delta.x as f64;
        app.calc_range_end += drag_delta.x as f64;
        return;
    }

    if at_end && negative_drag && !at_min_area {
        // println!("2");
        app.calc_range_start += drag_delta.x as f64;
        app.calc_range_end += drag_delta.x as f64;
        return;
    }
    if at_end && negative_drag && !at_min_area {
        // println!("3");
        app.calc_range_start += drag_delta.x as f64;
        app.calc_range_end += drag_delta.x as f64;
        return;
    }

    if app.calc_range_start < app.close_idx {
        // println!("4");
        let diff = (app.calc_range_start - app.close_idx).abs();
        app.calc_range_start = app.close_idx;
        if app.calc_range_end < app.open_idx {
            app.calc_range_end += diff;
        }
        return;
    }
    if app.calc_range_end > app.open_idx {
        // println!("5");
        let diff = (app.calc_range_end - app.open_idx).abs();
        app.calc_range_end = app.open_idx;
        if app.calc_range_start > app.close_idx {
            app.calc_range_start -= diff;
        }
        return;
    }
}
fn handle_drag_polygon(plot_ui: &mut PlotUi, app: &mut MyApp, is_left: bool) {
    let delta = plot_ui.pointer_coordinate_drag_delta();
    let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
    let calc_area_range = app.calc_range_end - app.calc_range_start;
    // println!("dragging polygon");

    if is_left && app.calc_range_start > app.close_idx && dragged {
        // println!("test");
        // do nothing if at min length and trying to make it smaller
        if calc_area_range <= app.min_calc_area_range && delta.x > 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.calc_range_start -= diff;
            return;
        }
        app.calc_range_start += delta.x as f64; // Adjust left boundary
    } else if !is_left && app.calc_range_end < app.open_idx && dragged {
        // do nothing if at min length and trying to make it smaller
        if calc_area_range < app.min_calc_area_range && delta.x < 0. {
            let diff = app.min_calc_area_range - calc_area_range;
            app.calc_range_end += diff;
            return;
        }
        app.calc_range_end += delta.x as f64; // Adjust right boundary
    }
}
