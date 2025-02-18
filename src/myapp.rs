use crate::structs::Cycle;
use eframe::egui::{show_tooltip_at, Color32, Id, PointerButton, Sense, Stroke, Ui};
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
        }
    }
}

// impl eframe::App for MyApp {
//     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//         egui::CentralPanel::default().show(ctx, |ui| {
//             let my_plot = Plot::new("My Plot").legend(egui_plot::Legend::default());
//
//             my_plot.show(ui, |plot_ui| {
//                 let calc_area_range = self.calc_range_end - self.calc_range_start;
//
//                 let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 64);
//                 let calc_area_stroke_color = Color32::from_rgb(64, 242, 106);
//
//                 let main_polygon = create_polygon(
//                     self.calc_range_start,
//                     self.calc_range_end,
//                     self.min_y,
//                     self.max_y,
//                     calc_area_color,
//                     calc_area_stroke_color,
//                     "main_area",
//                 );
//
//                 let left_polygon = create_polygon(
//                     self.calc_range_start,
//                     self.calc_range_start + (calc_area_range * 0.15),
//                     self.min_y,
//                     self.max_y,
//                     calc_area_color,
//                     calc_area_stroke_color,
//                     "left_area",
//                 );
//
//                 let right_polygon = create_polygon(
//                     self.calc_range_end - (calc_area_range * 0.15),
//                     self.calc_range_end,
//                     self.min_y,
//                     self.max_y,
//                     calc_area_color,
//                     calc_area_stroke_color,
//                     "right_area",
//                 );
//
//                 // Draw polygons
//                 plot_ui.polygon(main_polygon);
//                 plot_ui.polygon(left_polygon);
//                 plot_ui.polygon(right_polygon);
//
//                 // Handle dragging
//                 handle_drag_polygon(plot_ui, Id::new("left_area"), self, true);
//                 handle_drag_polygon(plot_ui, Id::new("right_area"), self, false);
//             });
//         });
//     }
// }

fn create_polygon(
    start_x: f64,
    end_x: f64,
    min_y: f64,
    max_y: f64,
    color: Color32,
    stroke: Color32,
    id: &str,
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

fn handle_drag_polygon(plot_ui: &mut PlotUi, id: Id, app: &mut MyApp, is_left: bool) {
    println!("Dragging: {:?}", id);
    if plot_ui.response().hovered() && plot_ui.response().dragged_by(PointerButton::Primary) {
        if let Some(delta) = Some(plot_ui.pointer_coordinate_drag_delta()) {
            if is_left {
                app.calc_range_start += delta.x as f64; // Adjust left boundary
            } else {
                app.calc_range_end += delta.x as f64; // Adjust right boundary
            }
        }
    }
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
                let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
                let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
                let calc_area_range = self.calc_range_end - self.calc_range_start;
                // let calc_area = Polygon::new(PlotPoints::from(vec![
                //     [self.calc_range_start, self.min_y],
                //     [self.calc_range_start, self.max_y],
                //     [self.calc_range_end, self.max_y],
                //     [self.calc_range_end, self.min_y],
                // ]))
                // .name("Calc area")
                // .width(2.0)
                // .fill_color(calc_area_color)
                // .stroke(Stroke::new(2., calc_area_stroke_color))
                // .allow_hover(true);
                //
                // let calc_area_s = Polygon::new(PlotPoints::from(vec![
                //     [self.calc_range_start, self.min_y],
                //     [self.calc_range_start, self.max_y],
                //     [self.calc_range_start + (calc_area_range * 0.15), self.max_y],
                //     [self.calc_range_start + (calc_area_range * 0.15), self.min_y],
                // ]))
                // .name("Calc area")
                // .fill_color(calc_area_color)
                // .stroke(Stroke::new(2., calc_area_stroke_color))
                // .allow_hover(true);
                //
                // let calc_area_e = Polygon::new(PlotPoints::from(vec![
                //     [self.calc_range_end - (calc_area_range * 0.15), self.min_y],
                //     [self.calc_range_end - (calc_area_range * 0.15), self.max_y],
                //     [self.calc_range_end, self.max_y],
                //     [self.calc_range_end, self.min_y],
                // ]))
                // .name("Calc area")
                // .fill_color(calc_area_color)
                // .stroke(Stroke::new(2., calc_area_stroke_color))
                // .allow_hover(true);

                let main_polygon = create_polygon(
                    self.calc_range_start,
                    self.calc_range_end,
                    self.min_y,
                    self.max_y,
                    calc_area_color,
                    calc_area_stroke_color,
                    "main_area",
                );

                let left_polygon = create_polygon(
                    self.calc_range_start,
                    self.calc_range_start + (calc_area_range * 0.15),
                    self.min_y,
                    self.max_y,
                    calc_area_color,
                    calc_area_stroke_color,
                    "left_area",
                );

                let right_polygon = create_polygon(
                    self.calc_range_end - (calc_area_range * 0.15),
                    self.calc_range_end,
                    self.min_y,
                    self.max_y,
                    calc_area_color,
                    calc_area_stroke_color,
                    "right_area",
                );

                // Draw polygons
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);

                // Handle dragging
                handle_drag_polygon(plot_ui, Id::new("left_area"), self, true);
                handle_drag_polygon(plot_ui, Id::new("right_area"), self, false);

                let sense = Sense::drag();

                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);
                // let response = plot_ui.polygon(calc_area);
                // plot_ui.polygon(calc_area_s);
                // plot_ui.polygon(calc_area_e);
                let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                let clicked_id = plot_ui.response();
                let polygon_id = Id::new("area");

                println!("{:?}", plot_ui.pointer_coordinate_drag_delta());
                println!("{:?}", clicked_id.id);
                println!("{:?}", clicked_id.rect);
                // let response = ui.interact(response.rect, polygon_id, sense);
                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let distance = (self.lag_idx - pointer_pos.x).abs();
                    println!("{}", distance);

                    let dragging = plot_ui.response().dragged_by(PointerButton::Primary);

                    let is_inside_polygon = is_inside_polygon(
                        pointer_pos,
                        self.calc_range_start,
                        self.calc_range_end,
                        self.min_y,
                        self.max_y,
                    );

                    if distance <= threshold && dragging {
                        self.lag_idx += drag_delta.x as f64;
                        lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
                        self.close_idx = self.start_time_idx + self.close_offset + lag_s;
                    }

                    if let Some((x, y, _)) =
                        find_nearest_point(&self.gas_plot, pointer_pos, threshold)
                    {
                        // Draw crosshair at nearest point
                        plot_ui.points(
                            Points::new([x, y])
                                .name("Highlighter")
                                .radius(2.)
                                .color(egui::Color32::RED)
                                .shape(MarkerShape::Circle),
                        );
                    }
                    // let mut previous_position = pointer_pos;
                    let distance_start = (self.calc_range_start - pointer_pos.x).abs();
                    println!("rect: {}", distance_start);
                    println!("threshold: {}", threshold);
                    if (is_inside_polygon && dragging) || (distance_start <= threshold && dragging)
                    {
                        threshold = 1000.;
                        self.calc_range_start += drag_delta.x as f64;
                        self.calc_range_end += drag_delta.x as f64;
                        println!("range: {}", calc_area_range);
                        println!("Inside rect");
                        // if pointer_pos.x < self.close_idx {
                        //     self.calc_range_start = self.close_idx;
                        // } else {
                        //     self.calc_range_start = pointer_pos.x;
                        //     self.calc_range_end = pointer_pos.x + calc_area_range;
                        // }
                        // if self.calc_range_end > self.open_idx {
                        //     self.calc_range_end = self.open_idx;
                        // }
                    }
                }
            });
            plot_rect = Some(inner.response.rect);
        });
    }
}
// WARN: BETTER FUNCTION FOR FINDING NEAREST
// when zoomed the elements get activated pretty far away
fn find_nearest_point(
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

// fn calc_area_polygon(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon {
//     let calc_area_color = Color32::from_rgba_unmultiplied(64, 242, 106, 4);
//     let calc_area_stroke_color = Color32::from_rgba_unmultiplied(64, 242, 106, 1);
//     let pts = vec![
//         PlotPoint::new(min_x, min_y),
//         PlotPoint::new(min_x, max_y),
//         PlotPoint::new(max_x, max_y),
//         PlotPoint::new(max_x, min_y),
//     ];
//     // let plot_pts = PlotPoints::new(pts);
//     Polygon::new(PlotPoints::Owned(pts))
//         .name("Calc area")
//         .fill_color(calc_area_color)
//         .stroke(Stroke::new(2., calc_area_stroke_color))
//         .allow_hover(true)
// }
