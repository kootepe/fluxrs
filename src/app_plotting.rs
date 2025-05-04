use crate::cycle::{insert_flux_history, update_fluxes, Cycle};
use crate::errorcode::ErrorCode;
pub use crate::instruments::GasType;
use crate::validation_app::ValidationApp;
use crate::validation_app::{create_polygon, create_vline, handle_drag_polygon, is_inside_polygon};
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use ecolor::Hsva;
use egui::{Align2, Rgba};
use std::collections::HashMap;

use std::ops::RangeInclusive;

use eframe::egui::{Color32, Id, Layout, PointerButton, Pos2, RichText, Stroke, Ui, Vec2};
use egui_plot::{
    CoordinatesFormatter, Corner, GridInput, GridMark, LineStyle, MarkerShape, Plot, PlotBounds,
    PlotPoint, PlotPoints, PlotTransform, Points, Text,
};

type DataTrace = (HashMap<String, Vec<[f64; 2]>>, HashMap<String, Vec<[f64; 2]>>);

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

    pub fn mark_dirty(&mut self) {
        if let Some(i) = self.cycle_nav.current_index() {
            self.dirty_cycles.insert(i);
        }
    }

    pub fn commit_all_dirty_cycles(&mut self) {
        let Some(project) = self.selected_project.clone() else { return };

        let dirty: Vec<_> =
            self.dirty_cycles.drain().filter_map(|i| self.cycles.get(i).cloned()).collect();

        if dirty.is_empty() {
            return;
        }

        self.runtime.spawn_blocking(move || {
            if let Ok(mut conn) = rusqlite::Connection::open("fluxrs.db") {
                if let Err(e) = update_fluxes(&mut conn, &dirty, project) {
                    eprintln!("Failed to commit dirty cycles: {e}");
                } else {
                    println!("Committed {} dirty cycles", dirty.len());
                }
            }
        });
    }

    pub fn select_cycle_by_timestamp(&mut self, timestamp: f64) {
        if let Some((idx, _)) = self
            .cycles
            .iter()
            .enumerate()
            .find(|(_, cycle)| cycle.start_time.timestamp() as f64 == timestamp)
        {
            if Some(idx) != self.cycle_nav.current_index() {
                self.commit_current_cycle();
                self.cycle_nav.jump_to_visible_index(idx);
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render_gas_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        gas_type: GasType,
        calc_area_color: Color32,
        calc_area_stroke_color: Color32,
        calc_area_adjust_color: Color32,
        main_id: Id,
        left_id: Id,
        right_id: Id,
    ) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let calc_start = self.get_calc_start(gas_type);
            let calc_end = self.get_calc_end(gas_type);
            let min_y = self.get_min_y(&gas_type);
            let max_y = self.get_max_y(&gas_type);
            let left_polygon = create_polygon(
                calc_start,
                calc_start + self.drag_panel_width,
                min_y,
                max_y,
                calc_area_adjust_color,
                calc_area_stroke_color,
                "Extend left",
                left_id,
            );

            let right_polygon = create_polygon(
                calc_end - self.drag_panel_width,
                calc_end,
                min_y,
                max_y,
                calc_area_adjust_color,
                calc_area_stroke_color,
                "Extend right",
                right_id,
            );

            let main_polygon = create_polygon(
                calc_start + self.drag_panel_width,
                calc_end - self.drag_panel_width,
                min_y,
                max_y,
                calc_area_color,
                calc_area_stroke_color,
                "Move",
                main_id,
            );

            let dashed = LineStyle::Dashed { length: 10.0 };
            let solid = LineStyle::Solid;
            let adj_x_open = cycle.get_adjusted_open();
            let adj_x_close = cycle.get_adjusted_close();
            let x_open = cycle.get_open();
            let x_close = cycle.get_close();

            let adj_open_line = create_vline(adj_x_open, Color32::DARK_GREEN, solid, "Lagtime");
            let adj_close_line = create_vline(adj_x_close, Color32::RED, solid, "Close time");
            let open_line = create_vline(x_open, Color32::DARK_GREEN, dashed, "Unadjusted open");
            let close_line = create_vline(x_close, Color32::RED, dashed, "Unadjusted close");

            if cycle.is_valid {
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);
            } else {
                let error_color = Color32::from_rgba_unmultiplied(255, 50, 50, 55);
                let error_polygon = create_polygon(
                    self.get_start(),
                    self.get_end(),
                    min_y,
                    max_y,
                    error_color,
                    error_color,
                    "error_area",
                    main_id,
                );
                plot_ui.polygon(error_polygon);
                let errors = ErrorCode::from_mask(cycle.error_code.0);
                let error_messages: Vec<String> =
                    errors.iter().map(|error| error.to_string()).collect();

                let msg = error_messages.join("\n");
                let has_errors = Id::new(format!("haserrors{}", gas_type));
                plot_ui.text(
                    Text::new(
                        PlotPoint::new(self.get_start(), max_y),
                        RichText::new(msg).size(20.0),
                    )
                    .highlight(true)
                    .anchor(Align2::LEFT_TOP)
                    .color(Color32::from_rgba_unmultiplied(250, 128, 128, 255))
                    .id(has_errors),
                );
            }

            if let Some(data) = cycle.gas_v.get(&gas_type) {
                let dt_v = cycle.dt_v_as_float();
                let diag_values = &cycle.diag_v;

                let mut normal_points = Vec::new();
                let mut highlighted_points = Vec::new();

                for ((x, y), &diag) in
                    dt_v.iter().copied().zip(data.iter().copied()).zip(diag_values.iter())
                {
                    if diag != 0 {
                        highlighted_points.push([x, y]);
                    } else {
                        normal_points.push([x, y]);
                    }
                }

                if !normal_points.is_empty() {
                    plot_ui.points(
                        Points::new(PlotPoints::from(normal_points))
                            .name(format!("{}", gas_type))
                            .shape(MarkerShape::Circle)
                            .color(gas_type.color())
                            .radius(2.0),
                    );
                }

                if !highlighted_points.is_empty() {
                    plot_ui.points(
                        Points::new(PlotPoints::from(highlighted_points))
                            .name(format!("{} (Error)", gas_type))
                            .shape(MarkerShape::Circle)
                            .color(egui::Color32::RED)
                            .radius(3.0),
                    );
                }

                plot_ui.vline(adj_open_line);
                plot_ui.vline(adj_close_line);
                plot_ui.vline(open_line);
                plot_ui.vline(close_line);
            } else {
                let half_way_x = self.get_start() + ((self.get_end() - self.get_start()) / 2.0);
                let bad_plot = Id::new(format!("bad_plot {}", gas_type));
                plot_ui.text(
                    Text::new(
                        PlotPoint::new(half_way_x, 0.0),
                        RichText::new("No data points").size(20.0),
                    )
                    .id(bad_plot),
                );
            }
        } else {
            // No visible cycle selected
            plot_ui.text(Text::new(
                PlotPoint::new(0.0, 0.0),
                RichText::new("No cycle selected").size(20.0),
            ));
        }
    }

    pub fn get_min_y(&self, gas_type: &GasType) -> f64 {
        self.cycle_nav
            .current_cycle(&self.cycles)
            .and_then(|cycle| cycle.min_y.get(gas_type))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn get_max_y(&self, gas_type: &GasType) -> f64 {
        self.cycle_nav
            .current_cycle(&self.cycles)
            .and_then(|cycle| cycle.max_y.get(gas_type))
            .copied()
            .unwrap_or(0.0)
    }

    /// Commits the current cycle to the DB if a project is selected
    pub fn commit_current_cycle(&mut self) {
        let Some(project) = self.selected_project.clone() else {
            eprintln!("[warn] No project selected, skipping commit.");
            return;
        };

        let Some(current_index) = self.cycle_nav.current_index() else {
            eprintln!("[warn] No current cycle selected.");
            return;
        };

        // Only commit if this cycle is dirty
        if !self.dirty_cycles.contains(&current_index) {
            return;
        }
        println!("Pushing current cycle.");

        let mut cycle = self.cycles[current_index].clone();
        cycle.manual_adjusted = true;

        self.dirty_cycles.remove(&current_index); // it's clean now

        self.runtime.spawn_blocking(move || match rusqlite::Connection::open("fluxrs.db") {
            Ok(mut conn) => {
                if let Err(e) = update_fluxes(&mut conn, &[cycle.clone()], project.clone()) {
                    eprintln!("[error] Failed to update cycle: {e}");
                }
                if let Err(e) = insert_flux_history(&mut conn, &[cycle], project) {
                    eprintln!("[error] Failed to insert history cycle: {e}");
                }
            },
            Err(e) => {
                eprintln!("[error] Failed to open DB: {e}");
            },
        });
    }
    pub fn _update_current_cycle(&mut self) {
        let Some(project) = self.selected_project.clone() else {
            eprintln!("No project selected!");
            return;
        };

        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.manual_adjusted = true;

            // Clone after updating (optional depending on what update_cycle does)
            let cycle_clone = cycle.clone();

            self.runtime.spawn_blocking(move || match rusqlite::Connection::open("fluxrs.db") {
                Ok(mut conn) => {
                    if let Err(e) = update_fluxes(&mut conn, &[cycle_clone], project) {
                        eprintln!("Flux update error: {}", e);
                    }
                },
                Err(e) => {
                    eprintln!("Failed to open database: {}", e);
                },
            });
        }
    }

    pub fn update_plots(&mut self) {
        self.all_traces = self.cycles.iter().map(|cycle| cycle.chamber_id.clone()).collect();

        for chamber_id in &self.all_traces {
            self.chamber_colors
                .entry(chamber_id.clone())
                .or_insert_with(|| generate_color(chamber_id));
        }

        let current_index = self.cycle_nav.current_index();

        // PREVIEW visible indexes before applying them
        let new_visible_indexes = compute_visible_indexes(
            &self.cycles,
            &self.visible_traces,
            self.show_valids,
            self.show_invalids,
        );

        // Only commit if current index is about to become invisible
        if let Some(idx) = current_index {
            if !new_visible_indexes.contains(&idx) && self.dirty_cycles.contains(&idx) {
                self.commit_current_cycle();
            }
        }

        // Now apply the new visible set
        self.cycle_nav.recompute_visible_indexes(
            &self.cycles,
            &self.visible_traces,
            self.show_valids,
            self.show_invalids,
        );

        // Update the current cycle’s diagnostics
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.check_errors();
        }
    }

    pub fn create_traces<F>(&self, gas_type: &GasType, selector: F) -> DataTrace
    where
        F: Fn(&Cycle, &GasType) -> f64, // Selector function with gas_type
    {
        let mut valid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
        let mut invalid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        // Iterate through the visible cycles using their indexes
        for &index in &self.cycle_nav.visible_cycles {
            if let Some(cycle) = self.cycles.get(index) {
                let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
                let value = selector(cycle, gas_type); // Extract value using selector
                let start_time = cycle.start_time.timestamp() as f64; // Get timestamp

                // Sort into valid/invalid traces
                if cycle.is_valid {
                    valid_traces
                        .entry(chamber_id)
                        .or_insert_with(Vec::new)
                        .push([start_time, value]);
                } else {
                    invalid_traces
                        .entry(chamber_id)
                        .or_insert_with(Vec::new)
                        .push([start_time, value]);
                }
            }
        }

        (valid_traces, invalid_traces)
    }

    pub fn get_close_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.close_offset as f64
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_open_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.open_offset as f64
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_end_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.end_offset as f64
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_start(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_start()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_end(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_end()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_measurement_end(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_adjusted_open()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }

    pub fn get_measurement_start(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_adjusted_close()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_calc_end(&self, gas_type: GasType) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_calc_end(gas_type)
        } else {
            0.0
        }
    }

    pub fn get_calc_start(&self, gas_type: GasType) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_calc_start(gas_type)
        } else {
            0.0
        }
    }

    pub fn set_calc_start(&mut self, gas_type: GasType, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_calc_start(gas_type, x);
        }
    }

    pub fn set_calc_end(&mut self, gas_type: GasType, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_calc_end(gas_type, x);
        }
    }

    pub fn decrement_calc_start(&mut self, gas_type: GasType, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.calc_range_start.get(&gas_type).unwrap_or(&0.0);
            let new_value = s - x;
            cycle.calc_range_start.insert(gas_type, new_value);
        }
    }

    pub fn increment_calc_start(&mut self, gas_type: GasType, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.get_calc_start(gas_type);
            let new_value = s + x;
            cycle.set_calc_start(gas_type, new_value);
        }
    }

    pub fn increment_calc_end(&mut self, gas_type: GasType, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.get_calc_end(gas_type);
            let new_value = s + x;
            cycle.set_calc_end(gas_type, new_value);
        }
    }
    pub fn increment_open_lag(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_open_lag(cycle.open_lag_s + x);
        }
    }
    pub fn increment_close_lag(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_close_lag(cycle.close_lag_s + x);
        }
    }

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

                // **Find the matching cycle index**
                self.select_cycle_by_timestamp(x_coord);
            }
        }

        // **Force `selected_point` to update whenever `index` changes**
        if let Some(current_cycle) = self.cycle_nav.current_cycle(&self.cycles) {
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
    fn merge_traces(
        &self,
        valid_traces: HashMap<String, Vec<[f64; 2]>>,
        invalid_traces: HashMap<String, Vec<[f64; 2]>>,
    ) -> HashMap<String, Vec<[f64; 2]>> {
        let mut merged_traces = HashMap::new();

        //   Insert all valid traces
        for (key, points) in valid_traces {
            merged_traces.entry(key).or_insert_with(Vec::new).extend(points);
        }

        //   Insert all invalid traces (merge if key already exists)
        for (key, points) in invalid_traces {
            merged_traces.entry(key).or_insert_with(Vec::new).extend(points);
        }

        merged_traces
    }
    pub fn render_lag_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
        let main_gas = self.main_gas.unwrap();

        let (valid_traces, invalid_traces) =
            self.create_traces(&main_gas, |cycle, _| cycle.open_lag_s);
        let lag_traces = self.merge_traces(valid_traces.clone(), invalid_traces.clone());

        let mut hovered_point: Option<[f64; 2]> = None;

        // === Draw points ===
        let mut chamber_ids: Vec<&String> = lag_traces.keys().collect();
        chamber_ids.sort();
        for chamber_id in chamber_ids {
            let color = *self
                .chamber_colors
                .entry(chamber_id.clone())
                .or_insert_with(|| generate_color(chamber_id));

            if let Some(points) = valid_traces.get(chamber_id) {
                plot_ui.points(
                    Points::new(PlotPoints::from(points.clone()))
                        .name(format!("{} (Valid)", chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
                        .color(color),
                );
            }

            if let Some(points) = invalid_traces.get(chamber_id) {
                plot_ui.points(
                    Points::new(PlotPoints::from(points.clone()))
                        .name(format!("{} (Invalid)", chamber_id))
                        .shape(MarkerShape::Cross)
                        .radius(3.)
                        .color(color),
                );
            }
        }

        let transform = plot_ui.transform();
        let response = plot_ui.response();

        // === Detect hovered point ===
        if let Some(cursor_pos) = plot_ui.ctx().pointer_latest_pos() {
            if self.dragged_point.is_none() {
                hovered_point = find_closest_point_screen_space(
                    &transform,
                    Some(cursor_pos),
                    &lag_traces,
                    80.0,
                );
            }
        }

        // Begin dragging
        if response.drag_started_by(egui::PointerButton::Primary) {
            if let Some(hovered) = hovered_point {
                self.dragged_point = Some(hovered);
            }
        }

        // Dragging in progress
        if let Some(dragged) = self.dragged_point {
            if response.dragged_by(egui::PointerButton::Primary) {
                let delta = response.drag_delta();
                let dy = delta.y as f64 * transform.dvalue_dpos()[1];
                let new_y = dragged[1] + dy;

                self.dragged_point = Some([dragged[0], new_y]);

                // Set lag on currently selected cycle
                if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                    if cycle.start_time.timestamp() as f64 == dragged[0] {
                        cycle.set_open_lag(new_y);
                    }
                }
            }
        }

        // Drag stopped
        if response.drag_stopped() {
            self.mark_dirty();
            self.dragged_point = None;
        }

        // Clicked on point — select corresponding cycle
        if let Some(hovered) = hovered_point {
            if response.clicked() || response.dragged_by(egui::PointerButton::Primary) {
                let x_coord = hovered[0];
                self.selected_point = Some([
                    x_coord,
                    lag_traces
                        .values()
                        .flatten()
                        .find(|p| p[0] == x_coord)
                        .map(|p| p[1])
                        .unwrap_or(0.0),
                ]);

                // Use CycleNavigator to jump
                self.select_cycle_by_timestamp(x_coord);
            }
        }

        // Sync selected point with current cycle
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let x = cycle.start_time.timestamp() as f64;
            if let Some(y) = lag_traces.values().flatten().find(|p| p[0] == x).map(|p| p[1]) {
                self.selected_point = Some([x, y]);
            }
        }

        // Draw selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new(PlotPoints::from(vec![selected]))
                    .name("Selected Point")
                    .shape(MarkerShape::Circle)
                    .radius(5.)
                    .filled(false)
                    .color(Color32::RED),
            );
        }

        // Draw hovered point (if distinct)
        if let Some(hovered) = hovered_point {
            if Some(hovered) != self.selected_point {
                plot_ui.points(
                    Points::new(PlotPoints::from(vec![hovered]))
                        .name("Hovered Point")
                        .shape(MarkerShape::Circle)
                        .radius(5.)
                        .filled(false)
                        .color(Color32::GREEN),
                );
            }
        }
    }

    pub fn _create_lag_plot(&self) -> Plot {
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
            calc_area_color,
            calc_area_stroke_color,
            calc_area_adjust_color,
            main_id,
            left_id,
            right_id,
        );

        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            let drag_delta = plot_ui.pointer_coordinate_drag_delta();

            let calc_start = self.get_calc_start(gas_type);
            let calc_end = self.get_calc_end(gas_type);
            let min_y = self.get_min_y(&gas_type);
            let max_y = self.get_max_y(&gas_type);

            let inside_left = is_inside_polygon(
                pointer_pos,
                calc_start,
                calc_start + drag_panel_width,
                min_y,
                max_y,
            );
            let inside_right =
                is_inside_polygon(pointer_pos, calc_end - drag_panel_width, calc_end, min_y, max_y);
            let inside_main = is_inside_polygon(
                pointer_pos,
                calc_start + drag_panel_width,
                calc_end - drag_panel_width,
                min_y,
                max_y,
            );

            // let x_open = self.get_start() + self.get_open_offset() + lag_s;
            let x_open = self.get_measurement_end();
            let x_close = self.get_measurement_start();
            let inside_open_lag = is_inside_polygon(
                pointer_pos,
                x_open - 20.,
                x_open + 20.,
                f64::NEG_INFINITY,
                f64::INFINITY,
            );
            let inside_close_lag = is_inside_polygon(
                pointer_pos,
                x_close - 20.,
                x_close + 20.,
                f64::NEG_INFINITY,
                f64::INFINITY,
            );

            let dragged = plot_ui.response().dragged_by(PointerButton::Primary);
            let at_start = calc_start == self.get_measurement_start();
            let at_end = calc_end == self.get_measurement_end();
            let over_start = calc_start < self.get_measurement_start();
            let over_end = calc_end > self.get_measurement_end();
            let range_len = calc_end - calc_start;
            let cycle_len = self.get_measurement_end() - self.get_measurement_start();

            // if range_len > cycle_len {
            //     self.set_calc_start(gas_type, self.get_measurement_start());
            //     self.set_calc_end(gas_type, self.get_measurement_end());
            // }

            // Decide what dragging action is happening
            let dragging_left = inside_left && dragged;
            let dragging_right = inside_right && dragged;
            let dragging_main = inside_main && dragged;
            let dragging_open_lag = inside_open_lag && dragged && !inside_right;
            let dragging_close_lag = inside_close_lag && dragged && !inside_left;
            let dragging_polygon = dragging_left || dragging_right || dragging_main;
            let mut dx = drag_delta.x as f64;
            let moving_right = dx > 0.;
            let moving_left = dx < 0.;

            let calc_range = calc_end - calc_start;

            // --- First: mutate `self` only ---
            if dragging_left {
                println!("Dragging left");
                handle_drag_polygon(plot_ui, self, true, &gas_type);
            }
            if dragging_right {
                println!("Dragging right");
                handle_drag_polygon(plot_ui, self, false, &gas_type);
            }

            if dragging_main {
                let calc_start = self.get_calc_start(gas_type);
                let calc_end = self.get_calc_end(gas_type);
                let measurement_start = self.get_measurement_start();
                let measurement_end = self.get_measurement_end();

                let mut clamped_dx = dx;

                // Prevent dragging past left bound
                if moving_left && calc_start + dx < measurement_start {
                    clamped_dx = measurement_start - calc_start;
                }

                // Prevent dragging past right bound
                if moving_right && calc_end + dx > measurement_end {
                    clamped_dx = measurement_end - calc_end;
                }

                if clamped_dx.abs() > f64::EPSILON {
                    self.increment_calc_start(gas_type, clamped_dx);
                    self.increment_calc_end(gas_type, clamped_dx);
                }
            }

            if dragging_open_lag {
                self.increment_open_lag(dx);
            }
            if dragging_close_lag {
                self.increment_close_lag(dx);
            }

            // --- Then: mutate the cycle safely ---
            if dragging_polygon {
                self.mark_dirty();
                self.cycle_nav.update_current_cycle(&mut self.cycles, |cycle| {
                    cycle.update_calc_attributes(gas_type);
                })
            };

            // if dragging_polygon {
            //     self.update_current_cycle();
            // }
            // --- Plot Bounds Adjustments ---
            let y_range = (self.get_max_y(&gas_type) - self.get_min_y(&gas_type)) * 0.05;
            let y_min = min_y - y_range;
            let y_max = max_y + y_range;

            if self.zoom_to_measurement {
                let x_min = x_open - 60.;
                let x_max = x_open + 60.;
                plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
            } else {
                plot_ui.set_auto_bounds(true);
            }
        }
    }

    pub fn render_legend(&mut self, ui: &mut Ui, _traces: &HashMap<String, Color32>) {
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
}

pub fn init_gas_plot(gas_type: &GasType, start: f64, end: f64, w: f32, h: f32) -> egui_plot::Plot {
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
pub fn init_calc_r_plot(gas_type: &GasType, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}calc_r2_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::UNIX_EPOCH
                    .checked_add_signed(Duration::seconds(timestamp))
                    .map(|dt| Utc.from_utc_datetime(&dt).format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| format!("{:.1}", value.x));
                // let datetime = DateTime::from_timestamp(timestamp, 0)
                //     .map(|dt| {
                //         // DateTime::<Utc>::from_utc(dt, Utc).format("%Y-%m-%d %H:%M:%S").to_string()
                //         Utc::from_utc_datetime(&dt).format("%Y-%m-%d %H:%M:%S").to_string()
                //     })
                //     .unwrap_or_else(|| format!("{:.1}", value.x));

                format!("Time: {}\n{} r2: {:.5}", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} calc r2", gas_type))
}
pub fn init_measurement_r_plot(gas_type: &GasType, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}measurement_r2_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                // let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                //     .map(|dt| {
                //         DateTime::<Utc>::from_utc(dt, Utc).format("%Y-%m-%d %H:%M:%S").to_string()
                //     })
                //     .unwrap_or_else(|| format!("{:.1}", value.x));
                let datetime = NaiveDateTime::UNIX_EPOCH
                    .checked_add_signed(Duration::seconds(timestamp))
                    .map(|dt| Utc.from_utc_datetime(&dt).format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| format!("{:.1}", value.x));
                format!("Time: {}\n{} r2: {:.5}", datetime, gas_type, value.y)
            }),
        )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} measurement r2", gas_type))
}
pub fn init_flux_plot(gas_type: &GasType, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}flux_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = NaiveDateTime::UNIX_EPOCH
                    .checked_add_signed(Duration::seconds(timestamp))
                    .map(|dt| Utc.from_utc_datetime(&dt).format("%Y-%m-%d %H:%M:%S").to_string())
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
                // let datetime = NaiveDateTime::from_timestamp_opt(timestamp, 0)
                //     .map(|dt| {
                //         DateTime::<Utc>::from_utc(dt, Utc)
                //             .format("%Y-%m-%d %H:%M:%S")
                //             .to_string()
                //     })
                //     .unwrap_or_else(|| format!("{:.1}", value.x));
                let datetime = NaiveDateTime::UNIX_EPOCH
                    .checked_add_signed(Duration::seconds(timestamp))
                    .map(|dt| Utc.from_utc_datetime(&dt).format("%Y-%m-%d %H:%M:%S").to_string())
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

fn _generate_color(seed: &str) -> Color32 {
    // alternate function for generating plot point colors from hsv values
    let hash = fxhash::hash(seed);

    // Map hash to a hue between 0 and 360
    let hue = (hash % 360) as f32 / 360.0;

    // Use fixed saturation and value for vividness
    let saturation = 0.85;
    let value = 0.9;
    let alpha = 1.;

    let hsva = Hsva::new(hue, saturation, value, alpha);
    Color32::from(Rgba::from(hsva))
}
fn compute_visible_indexes(
    cycles: &[Cycle],
    visible_traces: &HashMap<String, bool>,
    show_valids: bool,
    show_invalids: bool,
) -> Vec<usize> {
    cycles
        .iter()
        .enumerate()
        .filter(|(_, cycle)| {
            let trace_visible = visible_traces.get(&cycle.chamber_id).copied().unwrap_or(true);
            let valid_ok = show_valids || !cycle.is_valid;
            let invalid_ok = show_invalids || cycle.is_valid;
            trace_visible && valid_ok && invalid_ok
        })
        .map(|(i, _)| i)
        .collect()
}
