use crate::ui::validation_ui::Adjuster;
use crate::ui::validation_ui::ValidationApp;
use crate::ui::validation_ui::{create_polygon, create_vline, is_inside_polygon};
use fluxrs_core::cycle::cycle::{insert_flux_history, update_fluxes, Cycle};
use fluxrs_core::cycle::gaskey::GasKey;
use fluxrs_core::cycle_navigator::compute_visible_indexes;
use fluxrs_core::errorcode::ErrorCode;
use fluxrs_core::flux::{FluxKind, FluxModel, LinearFlux, PolyFlux, RobustFlux};
use fluxrs_core::gastype::GasType;
use fluxrs_core::instruments::instruments::Instrument;
use fluxrs_core::mode::Mode;

use crate::flux_extension::UiColor;
use crate::gastype_extension::GasColor;
use chrono::DateTime;
use ecolor::Hsva;
use egui::widgets::Label;
use egui::{Align2, Rgba};
use std::collections::HashMap;

use std::ops::RangeInclusive;

use eframe::egui::{Color32, Layout, PointerButton, Pos2, RichText, Ui, Vec2};
use egui_plot::{
    Bar, BarChart, CoordinatesFormatter, Corner, GridInput, GridMark, Line, LineStyle, MarkerShape,
    Plot, PlotBounds, PlotPoint, PlotPoints, PlotTransform, PlotUi, Points, Text,
};

type DataTrace = (HashMap<String, Vec<[f64; 2]>>, HashMap<String, Vec<[f64; 2]>>);
type DataTraceKind =
    (HashMap<String, Vec<(FluxKind, [f64; 2])>>, HashMap<String, Vec<(FluxKind, [f64; 2])>>);

impl ValidationApp {
    pub fn is_gas_enabled(&self, key: &GasKey) -> bool {
        self.enabled_gases.contains(key)
    }

    pub fn is_lin_flux_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_fluxes.contains(key)
    }
    pub fn is_lin_p_val_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_p_val.contains(key)
    }
    pub fn is_lin_rmse_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_rmse.contains(key)
    }
    pub fn is_lin_cv_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_cv.contains(key)
    }
    pub fn is_lin_sigma_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_sigma.contains(key)
    }
    pub fn is_lin_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_adj_r2.contains(key)
    }
    pub fn is_lin_aic_enabled(&self, key: &GasKey) -> bool {
        self.enabled_lin_aic.contains(key)
    }
    pub fn is_poly_flux_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_fluxes.contains(key)
    }
    pub fn is_poly_rmse_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_rmse.contains(key)
    }
    pub fn is_poly_cv_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_cv.contains(key)
    }
    pub fn is_poly_sigma_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_sigma.contains(key)
    }
    pub fn is_poly_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_adj_r2.contains(key)
    }
    pub fn is_poly_aic_enabled(&self, key: &GasKey) -> bool {
        self.enabled_poly_aic.contains(key)
    }
    pub fn is_roblin_rmse_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_rmse.contains(key)
    }
    pub fn is_roblin_cv_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_cv.contains(key)
    }
    pub fn is_roblin_sigma_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_sigma.contains(key)
    }
    pub fn is_roblin_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_adj_r2.contains(key)
    }
    pub fn is_roblin_aic_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_aic.contains(key)
    }
    pub fn is_roblin_flux_enabled(&self, key: &GasKey) -> bool {
        self.enabled_roblin_fluxes.contains(key)
    }

    pub fn is_exp_flux_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_fluxes.contains(key)
    }
    pub fn is_exp_p_val_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_p_val.contains(key)
    }
    pub fn is_exp_rmse_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_rmse.contains(key)
    }
    pub fn is_exp_cv_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_cv.contains(key)
    }
    pub fn is_exp_sigma_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_sigma.contains(key)
    }
    pub fn is_exp_adj_r2_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_adj_r2.contains(key)
    }
    pub fn is_exp_aic_enabled(&self, key: &GasKey) -> bool {
        self.enabled_exp_aic.contains(key)
    }
    // pub fn is_aic_diff_enabled(&self, key: &GasKey) -> bool {
    //     self.enabled_aic_diff.contains(key)
    // }

    pub fn is_calc_r_enabled(&self, key: &GasKey) -> bool {
        self.enabled_calc_r.contains(key)
    }

    pub fn is_measurement_r_enabled(&self, key: &GasKey) -> bool {
        self.enabled_measurement_rs.contains(key)
    }

    pub fn is_conc_t0_enabled(&self, key: &GasKey) -> bool {
        self.enabled_conc_t0.contains(key)
    }
    pub fn mark_dirty(&mut self) {
        if let Some(i) = self.cycle_nav.current_index() {
            self.dirty_cycles.insert(i);
        }
    }
    pub fn is_current_cycle_dirty(&self) -> bool {
        self.cycle_nav.current_index().map(|i| self.dirty_cycles.contains(&i)).unwrap_or(false)
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
                if let Err(e) = update_fluxes(&mut conn, &dirty, &project) {
                    eprintln!("Failed to commit dirty cycles: {e}");
                } else {
                    println!("Committed {} dirty cycles", dirty.len());
                }
            }
        });
    }

    pub fn control_zoom(&mut self, plot_ui: &mut egui_plot::PlotUi, key: &GasKey) {
        let x_open = self.get_measurement_start();
        let x_close = self.get_measurement_end();
        let min_y = self.get_min_y(key);
        let max_y = self.get_max_y(key);
        let y_range = (self.get_max_y(key) - self.get_min_y(key)) * 0.05;
        let y_min = min_y - y_range;
        let y_max = max_y + y_range;
        if self.zoom_to_measurement == 1 {
            let x_min = x_close - 60.;
            let x_max = x_close + 60.;
            plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
        } else if self.zoom_to_measurement == 2 {
            let x_min = x_open - 60.;
            let x_max = x_open + 60.;
            plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]));
            self.should_reset_bounds = true;
        } else if self.should_reset_bounds {
            plot_ui.set_auto_bounds(true);
        }
    }
    pub fn select_cycle_by_timestamp(&mut self, timestamp: f64) {
        if let Some((idx, _)) = self
            .cycles
            .iter()
            .enumerate()
            .find(|(_, cycle)| cycle.get_start_ts() as f64 == timestamp)
        {
            if Some(idx) != self.cycle_nav.current_index() {
                self.commit_current_cycle();
                self.cycle_nav.jump_to_visible_index(idx);
            }
        }
    }
    pub fn render_residual_bars(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        kind: FluxKind,
    ) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let (dt_v, actual) = cycle.get_calc_data2(key);

            // Prepare predictions from the selected model

            let Some(model) = self.get_model(key, kind) else { return };

            let y_pred: Vec<f64> = dt_v.iter().map(|&x| model.predict(x).unwrap_or(0.0)).collect();
            let residuals: Vec<f64> =
                actual.iter().zip(&y_pred).map(|(&y, &y_hat)| y - y_hat).collect();

            let num_bins = 20;
            // let min = residuals.iter().cloned().fold(f64::INFINITY, f64::min);
            // let max = residuals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let max_abs = residuals.iter().cloned().map(f64::abs).fold(0.0, f64::max);

            let min = -max_abs;
            let max = max_abs;
            let bin_width = (max - min) / num_bins as f64;
            // let bin_width = (max - min) / num_bins as f64;

            // Bin the residuals
            let mut bins = vec![0; num_bins];
            for &res in &residuals {
                let index = ((res - min) / bin_width).floor() as usize;
                let index = index.min(num_bins - 1); // clamp to last bin
                bins[index] += 1;
            }

            // Create BarChart from bins
            let bars: Vec<Bar> = bins
                .iter()
                .enumerate()
                .map(|(i, &count)| {
                    let center = min + (i as f64 + 0.5) * bin_width;
                    Bar::new(center, count as f64).width(bin_width * 0.9)
                })
                .collect();

            let bar_chart = BarChart::new(format!("{}{}residual_plot2", key, kind.as_str()), bars)
                .color(key.gas_type.color())
                .name("Residuals Histogram");
            plot_ui.bar_chart(bar_chart);
        } else {
            plot_ui.text(Text::new(
                "no cycle",
                PlotPoint::new(0.0, 0.0),
                RichText::new("No cycle selected").size(20.0),
            ));
        }
    }
    pub fn render_residual_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        kind: FluxKind,
    ) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            // let dt_v = cycle.get_calc_dt2(&key);
            // let actual = cycle.get_calc_gas_v2(&key);
            let (dt_v, actual) = cycle.get_calc_data2(key);

            // let gas_nopt: Vec<f64> = actual.iter().map(|x| x.unwrap_or(0.0)).collect();
            // let x0 = dt_v.get(0).copied().unwrap_or(0.0);
            let x0 = dt_v.first().unwrap();

            // Prepare predictions from the selected model
            let y_pred: Vec<f64> = match self.get_model(key, kind) {
                Some(model) => {
                    if let Some(lin) = model.as_any().downcast_ref::<LinearFlux>() {
                        dt_v.iter().map(|&x| lin.model.calculate(x)).collect()
                    } else if let Some(poly) = model.as_any().downcast_ref::<PolyFlux>() {
                        dt_v.iter().map(|&x| poly.model.calculate(x - poly.x_offset)).collect()
                    } else if let Some(rob) = model.as_any().downcast_ref::<RobustFlux>() {
                        dt_v.iter().map(|&x| rob.model.calculate(x - x0)).collect()
                    } else {
                        return; // Unsupported model type
                    }
                },
                None => return,
            };

            // Compute residuals
            let residuals: Vec<f64> =
                actual.iter().zip(&y_pred).map(|(&y, &y_hat)| y - y_hat).collect();

            let points: Vec<[f64; 2]> =
                // y_pred.iter().zip(&standardized).map(|(&x, &res)| [x, res / -1.]).collect();
                y_pred.iter().zip(&residuals).map(|(&x, &res)| [x, res]).collect();

            if !points.is_empty() {
                plot_ui.points(
                    Points::new("Residuals", PlotPoints::from(points))
                        .name(format!("{:?} {} Residuals", kind.as_str(), key))
                        .shape(MarkerShape::Circle)
                        .color(key.gas_type.color())
                        .radius(2.0),
                );
            }
        } else {
            plot_ui.text(Text::new(
                "no cycle",
                PlotPoint::new(0.0, 0.0),
                RichText::new("No cycle selected").size(20.0),
            ));
        }
    }
    pub fn render_standardized_residuals_plot(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        kind: FluxKind,
    ) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            // let dt_v = cycle.get_calc_dt2(&key);
            // let actual = cycle.get_calc_gas_v2(&key);
            let (dt_v, actual) = cycle.get_calc_data2(key);

            // Prepare predictions from the selected model
            let Some(model) = self.get_model(key, kind) else { return };

            let y_pred: Vec<f64> = dt_v.iter().map(|&x| model.predict(x).unwrap_or(0.0)).collect();

            // Compute residuals
            let residuals: Vec<f64> =
                actual.iter().zip(&y_pred).map(|(&y, &y_hat)| y - y_hat).collect();

            // Standardize residuals
            let mean = residuals.iter().copied().sum::<f64>() / residuals.len() as f64;
            let variance =
                residuals.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / residuals.len() as f64;
            let std = variance.sqrt();
            let standardized: Vec<f64> = residuals.iter().map(|r| (r - mean) / std).collect();

            // Plot standardized residuals vs predicted values
            let points: Vec<[f64; 2]> =
                y_pred.iter().zip(&standardized).map(|(&x, &res)| [x, res]).collect();

            if !points.is_empty() {
                let max_abs =
                    standardized.iter().copied().map(f64::abs).fold(0.0, f64::max).min(3.0);

                let y_min = -max_abs;
                let y_max = max_abs;

                let (x_min, x_max) = points
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(xmin, xmax), &[x, _]| {
                        (xmin.min(x), xmax.max(x))
                    });

                let x_padding = (x_max - x_min) * 0.05;
                let y_padding = (y_max - y_min) * 0.05;

                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                    [x_min - x_padding, y_min - y_padding],
                    [x_max + x_padding, y_max + y_padding],
                ));
                plot_ui.points(
                    Points::new(
                        format!("{} {}standardized residuals", kind.as_str(), key),
                        PlotPoints::from(points),
                    )
                    .name(format!("{} {} residuals stand", kind.as_str(), key))
                    .shape(MarkerShape::Circle)
                    .color(key.gas_type.color())
                    .radius(2.0),
                );
            }
        } else {
            plot_ui.text(Text::new(
                "no cycle",
                PlotPoint::new(0.0, 0.0),
                RichText::new("No cycle selected").size(20.0),
            ));
        }
    }

    pub fn render_gas_plot(&self, plot_ui: &mut egui_plot::PlotUi, key: &GasKey) {
        // BUG: if there's a gap in the data dragging stops working properly, items cant be dragged
        // over the plots if the start or end of the measurement is over the gap
        let dpw = self.get_dragger_width(key);

        let dark_green = Color32::DARK_GREEN;
        let red = Color32::RED;
        let error_color = Color32::from_rgba_unmultiplied(255, 50, 50, 55);

        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            // let si = cycle.get_calc_start_i(gas_type);
            // let ei = cycle.get_calc_end_i(gas_type);
            // let calc_start = cycle.get_measurement_gas_v2(gas_type)[si];
            // let calc_end = cycle.get_measurement_gas_v2(gas_type)[ei];
            let dead_s = self.get_deadband(key);
            let calc_start = cycle.get_calc_start(key);
            let calc_end = cycle.get_calc_end(key);
            let min_y = self.get_min_y(key);
            let max_y = self.get_max_y(key);

            let deadband = create_polygon(
                cycle.get_adjusted_close(),
                cycle.get_adjusted_close() + dead_s,
                min_y,
                max_y,
                Color32::from_rgba_unmultiplied(255, 0, 0, 30),
                Color32::BLACK,
                "deadband",
            );

            let left_polygon = create_polygon(
                calc_start,
                calc_start + dpw,
                min_y,
                max_y,
                self.calc_area_adjust_color,
                self.calc_area_stroke_color,
                "Extend left",
            );

            let right_polygon = create_polygon(
                calc_end - dpw,
                calc_end,
                min_y,
                max_y,
                self.calc_area_adjust_color,
                self.calc_area_stroke_color,
                "Extend right",
            );

            let main_polygon = create_polygon(
                calc_start + dpw,
                calc_end - dpw,
                min_y,
                max_y,
                self.calc_area_color,
                self.calc_area_stroke_color,
                "Move",
            );

            let dashed = LineStyle::Dashed { length: 10.0 };
            let solid = LineStyle::Solid;
            let adj_x_open = cycle.get_adjusted_open();
            let adj_x_close = cycle.get_adjusted_close();
            let x_open = cycle.get_open();
            let x_close = cycle.get_close();

            let adj_open_line = create_vline(adj_x_open, red, solid, "Lagtime");
            let adj_close_line = create_vline(adj_x_close, dark_green, solid, "Close time");
            let open_line = create_vline(x_open, red, dashed, "Unadjusted open");
            let close_line = create_vline(x_close, dark_green, dashed, "Unadjusted close");

            let mut gas_invalid = false;
            for ((g, _), record) in cycle.fluxes.iter() {
                if g.gas_type == key.gas_type && !record.is_valid {
                    gas_invalid = true
                }
            }
            if !cycle.is_valid || gas_invalid {
                let error_polygon = create_polygon(
                    cycle.get_start(),
                    cycle.get_end(),
                    min_y,
                    max_y,
                    error_color,
                    error_color,
                    "error_area",
                );
                plot_ui.polygon(error_polygon);
                let errors = ErrorCode::from_mask(cycle.error_code.0);
                let mut error_messages: Vec<String> =
                    errors.iter().map(|error| error.to_string()).collect();

                if gas_invalid {
                    error_messages.push("Gas marked as invalid".to_owned());
                }
                let msg = error_messages.join("\n");
                let has_errors = format!("haserrors{}", key.gas_type);
                plot_ui.text(
                    Text::new(
                        has_errors,
                        PlotPoint::new(self.get_start(), max_y),
                        RichText::new(msg).size(20.0),
                    )
                    .highlight(true)
                    .anchor(Align2::LEFT_TOP)
                    .color(Color32::from_rgba_unmultiplied(250, 128, 128, 255)),
                );
            } else if cycle.is_valid {
                plot_ui.polygon(deadband);
                plot_ui.polygon(main_polygon);
                plot_ui.polygon(left_polygon);
                plot_ui.polygon(right_polygon);
            }
            if let Some(data) = cycle.gas_v.get(key) {
                let dt_v = &cycle.get_dt_v(&key.id);
                let diag_v = &cycle.diag_v.get(&key.id).unwrap();

                let mut normal_points = Vec::new();
                let mut highlighted_points = Vec::new();

                for ((x, val_opt), &diag) in
                    dt_v.iter().copied().zip(data.iter().copied()).zip(diag_v.iter())
                {
                    if let Some(y) = val_opt {
                        if diag != 0 {
                            highlighted_points.push([x, y]);
                        } else {
                            normal_points.push([x, y]);
                        }
                    }
                }

                if !normal_points.is_empty() {
                    plot_ui.points(
                        Points::new("normals", PlotPoints::from(normal_points))
                            .name(format!("{}", key.gas_type))
                            .shape(MarkerShape::Circle)
                            .color(key.gas_type.color())
                            .radius(2.0),
                    );
                }

                if !highlighted_points.is_empty() {
                    plot_ui.points(
                        Points::new("errorpoints", PlotPoints::from(highlighted_points))
                            .name(format!("{} (Error)", key.gas_type))
                            .shape(MarkerShape::Circle)
                            .color(egui::Color32::RED)
                            .radius(3.0),
                    );
                }

                if self.show_linfit {
                    self.plot_model_fit(plot_ui, key, FluxKind::Linear);
                }
                if self.show_roblinfit {
                    self.plot_model_fit(plot_ui, key, FluxKind::RobLin);
                }
                if self.show_polyfit {
                    self.plot_model_fit(plot_ui, key, FluxKind::Poly);
                }
                if self.show_expfit {
                    self.plot_model_fit(plot_ui, key, FluxKind::Exponential);
                }

                plot_ui.vline(adj_open_line);
                plot_ui.vline(adj_close_line);
                plot_ui.vline(open_line);
                plot_ui.vline(close_line);
            } else {
                let half_way_x = self.get_start() + ((self.get_end() - self.get_start()) / 2.0);
                let bad_plot = format!("bad_plot {}", key.gas_type);
                plot_ui.text(Text::new(
                    bad_plot,
                    PlotPoint::new(half_way_x, 0.0),
                    RichText::new("No data points").size(20.0),
                ));
            }
        } else {
            // No visible cycle selected
            plot_ui.text(Text::new(
                "no cycle",
                PlotPoint::new(0.0, 0.0),
                RichText::new("No cycle selected").size(20.0),
            ));
        }
    }

    pub fn get_min_y(&self, key: &GasKey) -> f64 {
        self.cycle_nav
            .current_cycle(&self.cycles)
            .and_then(|cycle| cycle.min_y.get(key))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn get_max_y(&self, key: &GasKey) -> f64 {
        self.cycle_nav
            .current_cycle(&self.cycles)
            .and_then(|cycle| cycle.max_y.get(key))
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
                if let Err(e) = update_fluxes(&mut conn, &[cycle.clone()], &project) {
                    eprintln!("[error] Failed to update cycle: {e}");
                }
                if let Err(e) = insert_flux_history(&mut conn, &[cycle], &project) {
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
                    if let Err(e) = update_fluxes(&mut conn, &[cycle_clone], &project) {
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
        // println!("Update plots");
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
            self.show_bad,
            self.p_val_thresh as f64,
            self.rmse_thresh as f64,
            self.r2_thresh as f64,
            self.t0_thresh as f64,
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
            self.show_bad,
            self.p_val_thresh as f64,
            self.rmse_thresh as f64,
            self.r2_thresh as f64,
            self.t0_thresh as f64,
        );

        // Update the current cycle’s diagnostics
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.check_errors();
        }
    }

    pub fn create_traces<F>(&self, key: &GasKey, selector: F) -> DataTrace
    where
        F: Fn(&Cycle, &GasKey) -> f64, // Selector function with gas_type
    {
        let mut valid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();
        let mut invalid_traces: HashMap<String, Vec<[f64; 2]>> = HashMap::new();

        // Iterate through the visible cycles using their indexes
        for &index in &self.cycle_nav.visible_cycles {
            if let Some(cycle) = self.cycles.get(index) {
                let chamber_id = cycle.chamber_id.clone(); // Get chamber ID
                let value = selector(cycle, key); // Extract value using selector
                let start_time = cycle.get_start_ts() as f64; // Get timestamp

                // BUG: Thresholds need to be enabled/disabled within the app, otherwise it causes
                // issues with showing which measurements are valid.
                if let Some(best_kind) = cycle.best_model_by_aic(key) {
                    let gas_key = GasKey::from((&cycle.main_gas, &cycle.instrument.id.unwrap()));
                    let is_valid = cycle.is_valid_by_threshold(
                        &gas_key,
                        best_kind,
                        self.p_val_thresh as f64,
                        self.r2_thresh as f64,
                        self.rmse_thresh as f64,
                        self.t0_thresh as f64,
                    ) && cycle.is_valid;
                    // ) && cycle.error_code.0 == 0;

                    if is_valid {
                        valid_traces.entry(chamber_id).or_default().push([start_time, value]);
                    } else {
                        invalid_traces.entry(chamber_id).or_default().push([start_time, value]);
                    }
                } else {
                    // Handle case when no model is available — treat as invalid
                    invalid_traces.entry(chamber_id).or_default().push([start_time, value]);
                }
            }
        }

        (valid_traces, invalid_traces)
    }

    pub fn get_close_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_close_offset() as f64
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_open_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_open_offset() as f64
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_end_offset(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_end_offset() as f64
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
    pub fn get_min_calc_area_len(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_min_calc_len()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    // pub fn get_measurement_datas(&mut self) {
    //     if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
    //         cycle.get_measurement_datas();
    //     }
    // }
    pub fn get_is_valid(&self) -> bool {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_is_valid()
        } else {
            false // Return 0.0 if no valid cycle is found
        }
    }

    pub fn get_measurement_start(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_adjusted_close()
        } else {
            0.0 // Return 0.0 if no valid cycle is found
        }
    }
    pub fn get_calc_end(&self, key: &GasKey) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_calc_end(key)
        } else {
            0.0
        }
    }
    pub fn get_open_lag_s(&self) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_open_lag()
        } else {
            0.0
        }
    }
    pub fn get_slope(&self, key: &GasKey, kind: FluxKind) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_slope(key, kind).unwrap()
        } else {
            0.0
        }
    }
    pub fn get_intercept(&self, key: &GasKey, kind: FluxKind) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_intercept(key, kind).unwrap()
        } else {
            0.0
        }
    }
    pub fn get_lin_flux(&self, key: &GasKey) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_lin_flux(key).unwrap()
        } else {
            0.0
        }
    }

    pub fn get_calc_start(&self, key: &GasKey) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_calc_start(key)
        } else {
            0.0
        }
    }
    pub fn drag_left_to(&mut self, key: &GasKey, new_start: f64) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.drag_left_to(key, new_start)
        }
    }
    pub fn drag_right_to(&mut self, key: &GasKey, new_end: f64) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.drag_right_to(key, new_end)
        }
    }
    pub fn drag_main(&mut self, key: &GasKey, dx_steps: f64) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.drag_main(key, dx_steps)
        }
    }
    pub fn stick_calc_to_range_start(&mut self, key: &GasKey) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.stick_calc_to_range_start(key)
        }
    }
    pub fn stick_calc_to_range_start_for_all(&mut self) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.stick_calc_to_range_start_for_all(&cycle.gases)
        }
    }
    pub fn bounds_for(&self, key: &GasKey) -> (f64, f64) {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.timing.bounds_for(key)
        } else {
            (0.0, 0.0)
        }
    }

    pub fn get_deadband(&self, key: &GasKey) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_deadband(key)
        } else {
            0.0
        }
    }
    pub fn calc_area_can_move(&self, key: &GasKey) -> bool {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.calc_area_can_move(key)
        } else {
            false
        }
    }

    pub fn get_model(&self, key: &GasKey, kind: FluxKind) -> Option<&dyn FluxModel> {
        self.cycle_nav.current_cycle(&self.cycles).and_then(|cycle| cycle.get_model(key, kind))
    }
    // pub fn get_model(&self, key: GasKey, kind: FluxKind) -> Option<&dyn FluxModel> {
    //     if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
    //         Some(cycle.get_model(gas_type, kind).unwrap())
    //     } else {
    //         None
    //     }
    // }

    pub fn set_calc_start(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_calc_start(key, x);
        }
    }
    pub fn set_calc_start_all(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.set_calc_start_all(&cycle.gases, x);
            cycle.compute_all_fluxes();
        }
    }

    pub fn set_calc_end_all(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.set_calc_end_all(&cycle.gases, x);
            cycle.compute_all_fluxes();
        }
    }
    // pub fn print_first_dt(&mut self) {
    //     if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
    //         println!("first: {}", cycle.dt_v.first().unwrap());
    //     }
    // }
    // pub fn print_last_dt(&mut self) {
    //     if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
    //         println!("last: {}", cycle.dt_v.last().unwrap());
    //     }
    // }
    pub fn reload_gas(&mut self) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.reload_gas_data();
        }
    }
    pub fn reset_cycle(&mut self) {
        let mode = self.get_project_mode();
        if let Some(c) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            c.manual_adjusted = false;
            c.override_valid = None;
            c.set_close_lag(0.);
            c.set_open_lag(0.);
            c.reset_deadbands(self.selected_project.as_ref().unwrap().deadband);
            c.set_end_lag_only(0.);
            c.set_start_lag_only(0.);
            c.error_code.0 = 0;
            c.reload_gas_data();
            c.check_diag();
            c.check_missing();

            if !c.has_error(ErrorCode::ErrorsInMeasurement)
                || !c.has_error(ErrorCode::TooFewMeasurements)
            {
                c.search_open_lag(
                    &GasKey::from((&c.main_gas, &c.main_instrument.id.unwrap())).clone(),
                );
                match mode {
                    Mode::AfterDeadband => c.set_calc_ranges(),
                    Mode::BestPearsonsR => c.find_best_r_indices(),
                }
                c.calculate_concentration_at_t0();
                c.calculate_measurement_rs();
                c.check_main_r();
                // c.find_highest_r_windows();
                c.compute_all_fluxes();
                c.calculate_max_y();
                c.calculate_min_y();
                c.check_errors();
            }
        }
    }

    pub fn set_calc_end(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.timing.set_calc_end(key, x);
        }
    }

    pub fn set_calc_range_to_best_r(&mut self, key: &GasKey) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.find_best_r_indices_for_gas(key);
        }
    }

    pub fn set_all_calc_range_to_best_r(&mut self) {
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.find_best_r_indices();
        }
    }
    pub fn decrement_calc_start(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.get_calc_start(key);
            let new_value = s - x;
            cycle.set_calc_start(key, new_value);
        }
    }
    pub fn decrement_calc_starts(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            // NOTE: Get rid of clone
            for &key in &cycle.gases.clone() {
                let s = cycle.get_calc_start(&key);
                let new_value = s - x;
                cycle.set_calc_start(&key, new_value);
            }
        }
    }
    pub fn decrement_calc_ends(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            // NOTE: Get rid of clone
            for &key in &cycle.gases.clone() {
                let s = cycle.get_calc_end(&key);
                let new_value = s - x;
                cycle.set_calc_end(&key, new_value);
            }
        }
    }
    pub fn increment_calc_starts(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            // NOTE: Get rid of clone
            for &key in &cycle.gases.clone() {
                let s = cycle.get_calc_start(&key);
                let new_value = s + x;
                cycle.set_calc_start(&key, new_value);
            }
        }
    }
    pub fn increment_calc_ends(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            // NOTE: Get rid of clone
            for &key in &cycle.gases.clone() {
                let s = cycle.get_calc_end(&key);
                let new_value = s + x;
                cycle.set_calc_end(&key, new_value);
            }
        }
    }
    pub fn increment_calc_start(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.get_calc_start(key);
            let new_value = s + x;
            cycle.set_calc_start(key, new_value);
        }
    }

    pub fn increment_calc_end(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        // NOTE: Get rid of clone
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let s = cycle.get_calc_end(key);
            let new_value = s + x;
            cycle.set_calc_end(key, new_value);
        }
    }
    pub fn increment_deadband_gas(&mut self, key: &GasKey, x: f64) {
        self.mark_dirty();
        // NOTE: Get rid of clone
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            let deadband = cycle.get_deadband(key);
            cycle.set_deadband(key, deadband + x);
        }
    }
    pub fn increment_deadband(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            // NOTE: Get rid of clone
            for gas in cycle.gases.clone() {
                let deadband = cycle.get_deadband(&gas);
                cycle.set_deadband(&gas, deadband + x);
            }
        }
    }

    pub fn increment_deadband_constant_calc(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.set_deadband_constant_calc(x);
        }
    }
    pub fn increment_open_lag(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.increment_open_lag(x);
        }
    }
    pub fn increment_close_lag(&mut self, x: f64) {
        self.mark_dirty();
        if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
            cycle.increment_close_lag(x);
        }
    }
    pub fn get_calc_range(&self, key: &GasKey) -> f64 {
        if let Some(cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            cycle.get_calc_range(key)
        } else {
            0.0
        }
    }
    pub fn create_traces_fluxkind<F>(&self, key: &GasKey, selector: F) -> DataTraceKind
    where
        F: Fn(&Cycle, &GasKey) -> f64,
    {
        let mut valid_traces: HashMap<String, Vec<(FluxKind, [f64; 2])>> = HashMap::new();
        let mut invalid_traces: HashMap<String, Vec<(FluxKind, [f64; 2])>> = HashMap::new();

        for &index in &self.cycle_nav.visible_cycles {
            if let Some(cycle) = self.cycles.get(index) {
                let chamber_id = cycle.chamber_id.clone();
                let start_time = cycle.get_start_ts() as f64;
                let main_gas = cycle.main_gas;
                let id = &cycle.instrument.id.unwrap();

                // Get best model kind (lowest AIC among available models)
                let best_model = FluxKind::all()
                    .iter()
                    .filter_map(|kind| {
                        cycle
                            .get_model(&GasKey::from((&main_gas, id)), *kind)
                            .and_then(|m| m.aic().map(|aic| (*kind, aic)))
                    })
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

                if let Some((best_kind, _)) = best_model {
                    let value = selector(cycle, key);

                    let is_valid = cycle.is_valid_by_threshold(
                        &GasKey::from((&cycle.main_gas, &cycle.instrument.id.unwrap())),
                        best_kind,
                        self.p_val_thresh as f64,
                        self.r2_thresh as f64,
                        self.rmse_thresh as f64,
                        self.t0_thresh as f64,
                    ) && cycle.is_valid;
                    // ) && cycle.error_code.0 == 0;
                    let entry = (best_kind, [start_time, value]);

                    if is_valid {
                        valid_traces.entry(chamber_id).or_default().push(entry);
                    } else {
                        invalid_traces.entry(chamber_id).or_default().push(entry);
                    }
                }
            }
        }

        (valid_traces, invalid_traces)
    }
    pub fn render_best_flux_plot<F>(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        selector: F,
    ) where
        F: Fn(&Cycle, &GasKey) -> f64, // Selector function for extracting data
    {
        let (valid_traces, invalid_traces) = self.create_traces_fluxkind(key, selector);
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

            if let Some(points) = valid_traces.get(chamber_id) {
                // Group points by model
                let mut grouped: HashMap<FluxKind, Vec<[f64; 2]>> = HashMap::new();
                for (kind, point) in points {
                    grouped.entry(*kind).or_default().push(*point);
                }

                for (kind, point_list) in grouped {
                    let shape = marker_shape_for_flux_kind(kind);
                    let label = format!("{:?} {}", kind, chamber_id);

                    plot_ui.points(
                        Points::new(label.clone(), PlotPoints::from(point_list))
                            .name(label)
                            .shape(shape)
                            .radius(3.)
                            .color(*color),
                    );
                }
            }

            if let Some(points) = invalid_traces.get(chamber_id) {
                // Use a special style for invalids (no need to group)
                let plot_points =
                    PlotPoints::from(points.iter().map(|(_, pt)| *pt).collect::<Vec<_>>());

                plot_ui.points(
                    Points::new(format!("{} invalid", chamber_id), plot_points)
                        .shape(MarkerShape::Cross)
                        .radius(3.)
                        .color(*color),
                );
            }
        }

        // **Handle hovering logic (consider both valid & invalid traces)**
        let all_traces = self.merge_traces_fluxkind(valid_traces.clone(), invalid_traces.clone());
        let transform = plot_ui.transform();
        if let Some(cursor_screen_pos) = plot_ui.ctx().pointer_latest_pos() {
            hovered_point = find_closest_point_screen_space_fluxkind(
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
                if let Some(new_y) = all_traces
                    .values()
                    .flatten()
                    .filter(|(_, p)| p[0] == x_coord)
                    .map(|(_, p)| p[1])
                    .last()
                {
                    self.selected_point = Some([x_coord, new_y]);
                }

                // **Find the matching cycle index**
                self.select_cycle_by_timestamp(x_coord);
            }
        }

        // **Force `selected_point` to update whenever `index` changes**
        if let Some(current_cycle) = self.cycle_nav.current_cycle(&self.cycles) {
            let x_coord = current_cycle.get_start_ts() as f64;

            if let Some(new_y) = all_traces
                .values()
                .flatten()
                .filter(|(_, p)| p[0] == x_coord)
                .map(|(_, p)| p[1])
                .last()
            {
                self.selected_point = Some([x_coord, new_y]); // Keep x, update y
            }
        }

        // Draw updated selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new("currentpt", PlotPoints::from(vec![selected]))
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
                    Points::new("hovered_pt", PlotPoints::from(vec![hovered]))
                        .name("Closest")
                        .shape(MarkerShape::Circle)
                        .radius(5.0)
                        .filled(false)
                        .color(egui::Color32::GREEN),
                );
            }
        }
    }

    pub fn render_attribute_plot<F>(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        selector: F,
        plot_name: &str,
        symbol: Option<MarkerShape>,
    ) where
        F: Fn(&Cycle, &GasKey) -> f64, // Selector function for extracting data
    {
        let mut marker = MarkerShape::Circle;
        if symbol.is_some() {
            marker = symbol.unwrap();
        }
        let mut marker_size = 3.;
        if marker == MarkerShape::Circle {
            marker_size = 2.;
        }
        let (valid_traces, invalid_traces) = self.create_traces(key, selector);
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
                    Points::new("valid_pts", plot_points)
                        .name(format!("{} {}", plot_name, chamber_id))
                        .shape(marker)
                        .radius(marker_size)
                        .color(*color), // Normal color for valid points
                );
            }

            // **Render Invalid Traces (e.g., different color or shape)**
            if let Some(points) = invalid_traces.get(chamber_id) {
                let plot_points = PlotPoints::from(points.clone());

                plot_ui.points(
                    Points::new("invalid_pts", plot_points)
                    .name(format!("{} {} (Invalid)", plot_name, chamber_id))
                    .shape(MarkerShape::Cross) // Different shape for invalid points
                    .radius(marker_size)
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
            let x_coord = current_cycle.get_start_ts() as f64;

            if let Some(new_y) =
                all_traces.values().flatten().filter(|p| p[0] == x_coord).map(|p| p[1]).last()
            {
                self.selected_point = Some([x_coord, new_y]); // Keep x, update y
            }
        }

        // Draw updated selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new("currentpt", PlotPoints::from(vec![selected]))
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
                    Points::new("hovered_pt", PlotPoints::from(vec![hovered]))
                        .name("Closest")
                        .shape(MarkerShape::Circle)
                        .radius(5.0)
                        .filled(false)
                        .color(egui::Color32::GREEN),
                );
            }
        }
    }
    fn merge_traces_fluxkind(
        &self,
        valid_traces: HashMap<String, Vec<(FluxKind, [f64; 2])>>,
        invalid_traces: HashMap<String, Vec<(FluxKind, [f64; 2])>>,
    ) -> HashMap<String, Vec<(FluxKind, [f64; 2])>> {
        let mut merged_traces = valid_traces;

        for (key, mut points) in invalid_traces {
            merged_traces.entry(key).or_default().append(&mut points);
        }

        merged_traces
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
        let main_gas = self.selected_project.as_ref().unwrap().main_gas.unwrap();
        let id = self.selected_project.as_ref().unwrap().instrument.id.unwrap();

        let (valid_traces, invalid_traces) =
            self.create_traces(&(GasKey::from((&main_gas, &id))), |cycle, _| cycle.get_open_lag());
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
                    Points::new("valid_pts", PlotPoints::from(points.clone()))
                        .name(format!("{} (Valid)", chamber_id))
                        .shape(MarkerShape::Circle)
                        .radius(2.)
                        .color(color),
                );
            }

            if let Some(points) = invalid_traces.get(chamber_id) {
                plot_ui.points(
                    Points::new("invalid_pts", PlotPoints::from(points.clone()))
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
                hovered_point =
                    find_closest_point_screen_space(transform, Some(cursor_pos), &lag_traces, 20.0);
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
                self.current_ydelta += dy;
                let steps = self.current_ydelta.trunc();

                let new_y = dragged[1] + steps;
                self.current_ydelta -= steps;

                self.dragged_point = Some([dragged[0], new_y]);

                // Set lag on currently selected cycle
                if let Some(cycle) = self.cycle_nav.current_cycle_mut(&mut self.cycles) {
                    if cycle.get_start_ts() as f64 == dragged[0] {
                        cycle.increment_open_lag(steps);
                        // cycle.set_open_lag(new_y);
                        if self.mode_pearsons() {
                            self.set_all_calc_range_to_best_r();
                        }
                        self.stick_calc_to_range_start_for_all();
                    }
                }
            }
        }

        // Drag stopped
        if response.drag_stopped() {
            self.mark_dirty();
            self.dragged_point = None;
            self.current_ydelta = 0.;
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
            let x = cycle.get_start_ts() as f64;
            if let Some(y) = lag_traces.values().flatten().find(|p| p[0] == x).map(|p| p[1]) {
                self.selected_point = Some([x, y]);
            }
        }

        // Draw selected point
        if let Some(selected) = self.selected_point {
            plot_ui.points(
                Points::new("selected_pt", PlotPoints::from(vec![selected]))
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
                    Points::new("hovered_pt", PlotPoints::from(vec![hovered]))
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
    pub fn get_dragger_width(&self, key: &GasKey) -> f64 {
        (self.get_calc_range(key) * 0.3).min(40.)
    }
    pub fn render_residual_plot_ui(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        key: &GasKey,
        kind: FluxKind,
    ) {
        self.render_residual_plot(plot_ui, key, kind);
    }
    pub fn render_gas_plot_ui(&mut self, plot_ui: &mut egui_plot::PlotUi, key: &GasKey) {
        let dpw = self.get_dragger_width(key);

        self.render_gas_plot(plot_ui, key);

        if let Some(pointer_pos) = plot_ui.pointer_coordinate() {
            let drag_delta = plot_ui.pointer_coordinate_drag_delta();

            let primary_pressed =
                plot_ui.ctx().input(|i| i.pointer.button_pressed(PointerButton::Primary));
            let primary_down =
                plot_ui.ctx().input(|i| i.pointer.button_down(PointerButton::Primary));
            let primary_released =
                plot_ui.ctx().input(|i| i.pointer.button_released(PointerButton::Primary));

            let calc_start = self.get_calc_start(key);
            let calc_end = self.get_calc_end(key);
            let min_y = self.get_min_y(key);
            let max_y = self.get_max_y(key);

            let inside_left =
                is_inside_polygon(pointer_pos, calc_start, calc_start + dpw, min_y, max_y);
            let inside_right =
                is_inside_polygon(pointer_pos, calc_end - dpw, calc_end, min_y, max_y);
            let inside_main =
                is_inside_polygon(pointer_pos, calc_start + dpw, calc_end - dpw, min_y, max_y);

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
            let is_valid = self.get_is_valid();

            if primary_released {
                self.dragging = Adjuster::None;
                self.current_delta = 0.0;
                self.current_z_delta = 0.0;
            }

            if !self.dragging.is_dragged() && primary_pressed {
                // choose exactly one target on mouse-down
                if is_valid && inside_left {
                    self.dragging = Adjuster::Left;
                } else if is_valid && inside_right {
                    self.dragging = Adjuster::Right;
                } else if is_valid && inside_main {
                    self.dragging = Adjuster::Main;
                } else if inside_open_lag && !inside_right {
                    self.dragging = Adjuster::OpenLag;
                } else if inside_close_lag && !inside_left {
                    self.dragging = Adjuster::CloseLag;
                } else {
                    self.dragging = Adjuster::None;
                }
            }
            let dx = drag_delta.x as f64;
            let can_move = self.calc_area_can_move(key);
            let zoomed_dx = dx * plot_ui.transform().dpos_dvalue_x();

            match self.dragging {
                Adjuster::Left => {
                    if inside_left && can_move && dragged {
                        self.current_delta += dx;
                        let steps = self.current_delta.trunc();
                        self.current_delta -= steps;
                        if steps != 0.0 {
                            let (min_b, max_b) = self.bounds_for(key);
                            let end = self.get_calc_end(key);
                            let new_start = self.get_calc_start(key) + steps;
                            let (s, e) = clamp_resize_left(
                                (min_b, max_b),
                                new_start,
                                end,
                                self.get_min_calc_area_len(),
                            );
                            self.set_calc_start_all(s);
                            self.set_calc_end_all(e);
                        }
                    }
                },

                Adjuster::Right => {
                    if inside_right && can_move && dragged {
                        self.current_delta += dx;
                        let steps = self.current_delta.trunc();
                        self.current_delta -= steps;
                        if steps != 0.0 {
                            let (min_b, max_b) = self.bounds_for(key);
                            let start = self.get_calc_start(key);
                            let new_end = self.get_calc_end(key) + steps;
                            let (s, e) = clamp_resize_right(
                                (min_b, max_b),
                                start,
                                new_end,
                                self.get_min_calc_area_len(),
                            );
                            self.set_calc_start_all(s);
                            self.set_calc_end_all(e);
                        }
                    }
                },

                Adjuster::Main => {
                    if inside_main && dragged {
                        self.current_delta += dx;
                        let steps = self.current_delta.trunc();
                        self.current_delta -= steps;
                        if steps != 0.0 {
                            let (min_b, max_b) = self.bounds_for(key);
                            let s0 = self.get_calc_start(key) + steps;
                            let e0 = self.get_calc_end(key) + steps;
                            let (s, e) = clamp_translate((min_b, max_b), s0, e0);
                            self.set_calc_start_all(s);
                            self.set_calc_end_all(e);
                        }
                    }
                },
                Adjuster::CloseLag => {
                    if inside_close_lag && dragged {
                        let delta = if self.zoom_to_measurement == 2 {
                            self.current_z_delta += zoomed_dx;
                            let steps = self.current_z_delta.trunc();
                            self.current_z_delta -= steps;
                            steps
                        } else {
                            self.current_delta += dx;
                            let steps = self.current_delta.trunc();
                            self.current_delta -= steps;
                            steps
                        };

                        let is_moving = delta != 0.0;
                        let can_move_left_after_adjust = !can_move && delta < 0.0;

                        if is_moving && (can_move || can_move_left_after_adjust) {
                            self.increment_close_lag(delta);

                            // Anchor calc window to new start of range (stick-to-beginning)
                            self.stick_calc_to_range_start_for_all();

                            if self.mode_pearsons() {
                                self.set_all_calc_range_to_best_r();
                            }
                        }
                    }
                },

                Adjuster::OpenLag => {
                    if inside_open_lag && dragged {
                        let delta = if self.zoom_to_measurement == 1 {
                            self.current_z_delta += zoomed_dx;
                            let steps = self.current_z_delta.trunc();
                            self.current_z_delta -= steps;
                            steps
                        } else {
                            self.current_delta += dx;
                            let steps = self.current_delta.trunc();
                            self.current_delta -= steps;
                            steps
                        };

                        if delta != 0.0 {
                            self.increment_open_lag(delta);

                            // Anchor calc window to new start of range (stick-to-beginning)
                            self.stick_calc_to_range_start_for_all();

                            if self.mode_pearsons() {
                                self.set_all_calc_range_to_best_r();
                            }
                        }
                    }
                },

                Adjuster::None => {},
            }

            // Mark dirty / update once something is being dragged
            let dragging_polygon =
                matches!(self.dragging, Adjuster::Left | Adjuster::Right | Adjuster::Main)
                    && primary_down;
            let dragging_lag =
                matches!(self.dragging, Adjuster::OpenLag | Adjuster::CloseLag) && primary_down;

            // --- Then: mutate the cycle safely ---
            if dragging_polygon {
                self.mark_dirty();
                self.cycle_nav.update_current_cycle(&mut self.cycles, |cycle| {
                    cycle.update_calc_attributes(key);
                })
            }
            if dragging_lag {
                self.mark_dirty();
                self.cycle_nav.update_current_cycle(&mut self.cycles, |cycle| {
                    cycle.update_measurement_attributes(key);
                })
            };

            self.control_zoom(plot_ui, key);
        }
    }
    pub fn handle_drag_polygon(&mut self, plot_ui: &mut PlotUi, is_left: bool, key: &GasKey) {
        // BUG: Dragging when the start/end markers are out of data bounds will cause the plot to
        // shrink/enlargen and then the dragged items can lose focus
        let dx = self.current_delta.trunc();
        self.current_delta -= dx;

        let calc_start = self.get_calc_start(key);
        let calc_end = self.get_calc_end(key);
        let calc_range = calc_end - calc_start;

        let close_time = self.get_measurement_start();
        let open_time = self.get_measurement_end();
        // TODO: minimum calc range should be adjustable in app, so automated calculation uses what
        // is defined within the project and manual validation can use another
        let at_min_range = calc_range <= self.get_min_calc_area_len();

        if is_left {
            let can_move_left = calc_start >= close_time;
            let not_shrinking = !at_min_range || dx < 0.0;

            if can_move_left && not_shrinking {
                self.increment_calc_start(key, dx);
            }
        } else {
            let can_move_right = calc_end <= open_time;
            let not_shrinking = !at_min_range || dx > 0.0;

            if can_move_right && not_shrinking {
                self.increment_calc_end(key, dx);
            }
        }
    }

    pub fn render_legend(&mut self, ui: &mut Ui, _traces: &HashMap<String, Color32>) {
        let legend_width = ui.available_width();
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
                ui.add(Label::new("Legend").selectable(false));

                if self.visible_traces.is_empty() {
                    self.visible_traces = sorted_traces.iter().map(|s| (s.clone(), true)).collect();
                }

                // Split sorted_traces into chunks of 30
                let max_per_column = 30;
                let columns: Vec<_> =
                    sorted_traces.chunks(max_per_column).map(|chunk| chunk.to_vec()).collect();

                ui.horizontal_wrapped(|ui| {
                    for column in columns {
                        ui.vertical(|ui| {
                            for chamber_id in column {
                                let mut visible =
                                    self.visible_traces.get(&chamber_id).copied().unwrap_or(true);

                                ui.horizontal(|ui| {
                                    let color = *self.chamber_colors.get(&chamber_id).unwrap();

                                    let response = ui.checkbox(&mut visible, "");

                                    if response.clicked() {
                                        self.toggle_visibility(&chamber_id);
                                        self.update_plots();
                                    }

                                    if response.double_clicked() {
                                        self.visible_traces
                                            .iter_mut()
                                            .for_each(|(_, v)| *v = false);
                                        self.visible_traces.insert(chamber_id.clone(), true);
                                        self.update_plots();
                                    }

                                    let (rect, _) =
                                        ui.allocate_at_least(color_box_size, egui::Sense::hover());
                                    ui.painter().rect_filled(rect, 2.0, color);
                                    ui.label(&chamber_id);
                                });
                            }
                        });
                    }
                });

                if ui.button("Select All").clicked() {
                    for key in sorted_traces {
                        self.visible_traces.insert(key, true);
                    }
                    self.update_plots();
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
            return;
        }

        self.visible_traces.insert(chamber_id.clone(), !is_visible);
    }

    pub fn plot_model_fit(&self, plot_ui: &mut egui_plot::PlotUi, key: &GasKey, kind: FluxKind) {
        let x_min = self.get_calc_start(key);
        let x_max = self.get_measurement_end();
        let num_points = 50;

        let label = format!("{}{}{}", key.gas_type, key.id, kind.as_str());
        if let Some(model) = self.get_model(key, kind) {
            let points: PlotPoints = (0..=num_points)
                .filter_map(|i| {
                    let t = i as f64 / num_points as f64;
                    let x_real = x_min + t * (x_max - x_min);
                    model.predict(x_real).map(|y| [x_real, y])
                })
                .collect();

            let color = kind.color();
            let style = kind.style();
            let stroke = kind.stroke();
            plot_ui.line(Line::new(label, points).color(color).stroke(stroke).style(style));
        }
    }
}
pub fn init_attribute_plot(
    attribute: String,
    key: &GasKey,
    instrument: Instrument,
    w: f32,
    h: f32,
) -> egui_plot::Plot {
    let attrib = attribute.clone();
    Plot::new(format!("{}{}{}", key.gas_type, key.id, attrib))
        // .coordinates_formatter(
        //     Corner::LeftBottom,
        //     CoordinatesFormatter::new(move |value, _| {
        //         let timestamp = value.x as i64;
        //         let datetime = DateTime::from_timestamp(timestamp, 0)
        //             .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        //             .unwrap_or_else(|| format!("{:.1}", timestamp));
        //
        //         format!("Time: {}\n{} {}: {:.5}", datetime, gas_type, attrib, value.y)
        //     }),
        // )
        .label_formatter(|_, _| String::new())
        .allow_drag(false)
        .width(w)
        .height(h)
        .x_axis_formatter(format_x_axis)
        .y_axis_label(format!("{} {} {}", key.gas_type, instrument.serial , attribute))
}
pub fn init_residual_plot(gas_type: &GasType, kind: FluxKind, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}{}residual_plot", gas_type, kind.as_str()))
        .width(w)
        .height(h)
        .y_axis_label(format!("{}", gas_type))
}
pub fn init_standardized_residuals_plot(
    key: &GasKey,
    instrument: Instrument,
    kind: FluxKind,
    w: f32,
    h: f32,
) -> egui_plot::Plot {
    Plot::new(format!("{}{}{}standardized_residual_plot",key.id, key.gas_type, kind.as_str()))
        .width(w)
        .height(h)
        .x_axis_formatter(|_val, _range| String::new()) // Hide tick labels.width(w)
        .allow_drag(false)
        .allow_zoom(false)
        .y_axis_label(format!("{}{}",key.gas_type,instrument.serial ))
}
pub fn init_residual_bars(
    key: &GasKey,
    instrument: Instrument,
    kind: FluxKind,
    w: f32,
    h: f32,
) -> egui_plot::Plot {
    Plot::new(format!("{}{}{}residual_bars", key.id, key.gas_type, kind.as_str()))
        .width(w)
        .height(h)
        .allow_drag(false)
        .allow_zoom(false)
        .y_axis_label(format!("{} {}", key.gas_type, instrument.serial))
}
pub fn init_gas_plot(
    key: &GasKey,
    instrument: Instrument,
    start: f64,
    end: f64,
    w: f32,
    h: f32,
) -> egui_plot::Plot {
    let _x_axis_formatter = |mark: GridMark, _range: &std::ops::RangeInclusive<f64>| -> String {
        let timestamp = mark.value as i64;

        // Round to the nearest 5-minute interval (300 seconds)
        let rounded_timestamp = (timestamp / 300) * 300;

        DateTime::from_timestamp(rounded_timestamp, 0)
            .map(|dt| dt.format("%H:%M").to_string())
            .unwrap_or_else(|| "Invalid".to_string())
    };
    Plot::new(format!("{}{}gas_plot", key.gas_type, key.id))
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
                    key.gas_type,
                    value.y,
                    key.gas_type.unit()
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
        .x_axis_formatter(format_x_axis)
        .allow_drag(false)
        .width(w)
        .height(h)
        .include_x(start)
        .include_x(end)
        .y_axis_label(format!("{} {}", key.gas_type, instrument.serial))
    // .legend(Legend::default().position(Corner::LeftTop))
}

pub fn init_calc_r_plot(gas_type: &GasType, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}calc_r2_plot", gas_type))
        .coordinates_formatter(
            Corner::LeftBottom,
            CoordinatesFormatter::new(move |value, _| {
                let timestamp = value.x as i64;
                let datetime = DateTime::from_timestamp(timestamp, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| format!("{:.1}", timestamp));

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

pub fn init_lag_plot(key: &GasKey, instrument: Instrument, w: f32, h: f32) -> egui_plot::Plot {
    Plot::new(format!("{}{}lag_plot",key.gas_type,key.id))
        // .coordinates_formatter(
        //     Corner::LeftBottom,
        //     CoordinatesFormatter::new(move |value, _| {
        //         let timestamp = value.x as i64;
        //         let datetime = DateTime::from_timestamp(timestamp, 0)
        //             .map(|dt| {
        //                 dt.format("%Y-%m-%d %H:%M:%S").to_string()
        //             })
        //             .unwrap_or_else(|| format!("{:.1}", value.x));
        //
        //
        //         format!("Time: {}\n{} lag: {:.0} sec", datetime, gas_type, value.y)
        //     }),
        // )
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
        .y_axis_label(format!("{} {} lag s", key.gas_type, instrument.serial))
        .x_axis_formatter(format_x_axis)
}
fn _generate_grid_marks(range: GridInput) -> Vec<GridMark> {
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

pub fn find_closest_point_screen_space_fluxkind(
    plot_transform: &PlotTransform,
    cursor_pos: Option<Pos2>,
    traces: &HashMap<String, Vec<(FluxKind, [f64; 2])>>,
    max_screen_distance: f32,
) -> Option<[f64; 2]> {
    let cursor_screen = cursor_pos?;

    let mut closest_point: Option<[f64; 2]> = None;
    let mut min_dist = f32::INFINITY;

    for trace in traces.values() {
        for &(_, point) in trace {
            let screen_pos =
                plot_transform.position_from_point(&PlotPoint::new(point[0], point[1]));

            let screen_dist = ((screen_pos.x - cursor_screen.x).powi(2)
                + (screen_pos.y - cursor_screen.y).powi(2))
            .sqrt();

            if screen_dist < min_dist && screen_dist <= max_screen_distance {
                min_dist = screen_dist;
                closest_point = Some(point);
            }
        }
    }

    closest_point
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
fn marker_shape_for_flux_kind(kind: FluxKind) -> MarkerShape {
    match kind {
        FluxKind::Linear => MarkerShape::Circle,
        FluxKind::Poly => MarkerShape::Square,
        FluxKind::RobLin => MarkerShape::Diamond,
        _ => MarkerShape::Cross, // Fallback
    }
}

pub fn to_round(val: f64) -> f64 {
    if val > 0.0 {
        val.ceil()
    } else {
        val.floor()
    }
}
// Move both ends together (Main)
fn clamp_translate((min_b, max_b): (f64, f64), mut s: f64, mut e: f64) -> (f64, f64) {
    if s < min_b {
        let d = min_b - s;
        s += d;
        e += d;
    }
    if e > max_b {
        let d = e - max_b;
        s -= d;
        e -= d;
    }
    (s, e)
}

// Resize left: pin end, clamp start into [min_b, end]
fn clamp_resize_left((min_b, _): (f64, f64), new_start: f64, end: f64, min_len: f64) -> (f64, f64) {
    // desired start cannot go below min bound
    let mut s = new_start.max(min_b);
    // enforce min length by NOT moving end; s cannot exceed end - min_len
    s = s.min(end - min_len);
    (s, end)
}

// Resize right: pin start, clamp end into [start, max_b]
fn clamp_resize_right(
    (_, max_b): (f64, f64),
    start: f64,
    new_end: f64,
    min_len: f64,
) -> (f64, f64) {
    let mut e = new_end.min(max_b);
    e = e.max(start + min_len);
    (start, e)
}
