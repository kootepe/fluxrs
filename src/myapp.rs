use crate::structs::Cycle;
use eframe::egui::{show_tooltip_at, PointerButton, Sense, Ui};
use egui_plot::{
    ClosestElem, Corner, HLine, Legend, Line, MarkerShape, Plot, PlotItem, PlotPoints, Points,
    Polygon, VLine,
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
        let max_y = cycle
            .gas_v
            .iter()
            .cloned()
            .filter(|v| !v.is_nan())
            .fold(f64::INFINITY, f64::min);
        let min_y = cycle
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
        egui::CentralPanel::default().show(ctx, |ui| {
            let my_plot = Plot::new("My Plot")
                .allow_drag(false)
                .width(400.)
                .height(250.)
                .legend(Legend::default().position(Corner::LeftTop));
            let my_plot2 = Plot::new("My Plot")
                .allow_drag(false)
                .width(400.)
                .height(250.)
                .legend(Legend::default().position(Corner::LeftTop));

            let mut lag_s = self.lag_idx - (self.start_time_idx + self.open_offset);
            let inner2 = my_plot2.show(ui, |plot_ui| {
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
                let calc_area = Polygon::new(PlotPoints::from(vec![
                    [self.calc_range_start, self.min_y],
                    [self.calc_range_start, self.max_y],
                    [self.calc_range_end, self.max_y],
                    [self.calc_range_end, self.min_y],
                ]))
                .name("Calc area")
                .width(2.0)
                .allow_hover(true);
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);
                plot_ui.polygon(calc_area);

                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let mut threshold = 60.;
                    let distance = (self.lag_idx - pointer_pos.x).abs();

                    let dragging = plot_ui.response().dragged_by(PointerButton::Primary);

                    if distance <= threshold && dragging {
                        threshold = 1000.;
                        self.lag_idx = pointer_pos.x;
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
                }
            });
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
                let calc_area = Polygon::new(PlotPoints::from(vec![
                    [self.calc_range_start, self.min_y],
                    [self.calc_range_start, self.max_y],
                    [self.calc_range_end, self.max_y],
                    [self.calc_range_end, self.min_y],
                ]))
                .name("Calc area")
                .width(2.0)
                .allow_hover(true);
                plot_ui.vline(max_vl);
                plot_ui.vline(close_vl);
                plot_ui.polygon(calc_area);

                if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
                    let mut threshold = 60.;
                    let distance = (self.lag_idx - pointer_pos.x).abs();

                    let dragging = plot_ui.response().dragged_by(PointerButton::Primary);

                    if distance <= threshold && dragging {
                        threshold = 1000.;
                        self.lag_idx = pointer_pos.x;
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
                }
            });
            plot_rect = Some(inner.response.rect);
            plot_rect = Some(inner2.response.rect);
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
