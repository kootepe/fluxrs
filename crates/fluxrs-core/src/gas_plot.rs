pub const PLOT_HEIGHT: u32 = 40;
pub const PLOT_WIDTH: u32 = PLOT_HEIGHT * 2;

// pub fn draw_gas_plot(cycle: &Cycle) -> Result<String, Box<dyn Error>> {
//     let name = cycle.start_time.to_string();
//     let mut root = String::from("images/");
//     root.push_str(&name);
//     root.retain(|c| !r#"()-,".;:' "#.contains(c));
//     root.push_str(".png");
//     let mut wpath = String::from("html/");
//     wpath.push_str(&root);
//     let time_data: Vec<f64> = cycle.dt_v.iter().map(|x| x.timestamp() as f64).collect();
//     let xmin = cycle.start_time.timestamp() as f64;
//     let xmax = cycle.end_time.timestamp() as f64;
//     // WARN: might cause issues if open time or close are not updated properly
//     let cxmin = cycle.calc_range_start;
//     let cxmax = cycle.calc_range_end;
//
//     let gas_data = &cycle.gas_v;
//     let ymin = cycle
//         .gas_v
//         .iter()
//         .cloned()
//         .filter(|v| !v.is_nan())
//         .fold(f64::INFINITY, f64::min);
//     let ymax = cycle
//         .gas_v
//         .iter()
//         .cloned()
//         // .rev()
//         // .take(120)
//         .filter(|v| !v.is_nan())
//         .fold(f64::NEG_INFINITY, f64::max);
//
//     let close_line = vec![
//         (cycle.close_time.timestamp() as f64, 10000000.),
//         (cycle.close_time.timestamp() as f64, 0.),
//     ];
//     // let open_line = vec![
//     //     (cycle.open_time.timestamp() as f64, 10000000.),
//     //     (cycle.open_time.timestamp() as f64, 0.),
//     // ];
//     let max_line = vec![(cycle.max_idx, 10_000_000.), (cycle.max_idx, 0.)];
//     let rect = [(cxmax, 10_000_000.), (cxmin, -10_000_000.)];
//     let data: Vec<(f64, f64)> = time_data
//         .iter()
//         .zip(gas_data.iter())
//         .map(|(&t, &g)| (t, g))
//         .collect();
//     let rect_style = ShapeStyle {
//         color: WHITE.mix(0.30),
//         filled: true,
//         stroke_width: 5,
//     };
//     let bg_col = RGBColor(10, 11, 10);
//     let root_area = BitMapBackend::new(&wpath, (PLOT_WIDTH, PLOT_HEIGHT)).into_drawing_area();
//     root_area.fill(&bg_col).unwrap();
//
//     // let (xrange, yrange) = get_xyrange(&cycle);
//     let mut ctx = ChartBuilder::on(&root_area)
//         // 5% buffer around the data
//         .build_cartesian_2d(
//             (xmin)..(xmax),
//             (ymin)..(ymax),
//             // (xmin - (xrange * 0.05))..(*xmax + (xrange * 0.05)),
//             // (ymin - (yrange * 0.05))..(ymax + (yrange * 0.05)),
//         )
//         .unwrap();
//
//     ctx.configure_mesh().disable_mesh().draw().unwrap();
//
//     let pt_col = RGBColor(105, 239, 83);
//     ctx.draw_series(
//         data.iter()
//             .map(|point| Cross::new(*point, 1, pt_col.mix(0.6))),
//     )
//     .unwrap();
//     ctx.draw_series(std::iter::once(DashedPathElement::new(
//         max_line, 11, 5, RED,
//     )))?;
//     ctx.draw_series(std::iter::once(DashedPathElement::new(
//         close_line, 11, 5, BLACK,
//     )))?;
//     // ctx.draw_series(std::iter::once(DashedPathElement::new(
//     //     open_line, 11, 5, GREEN,
//     // )))?;
//     ctx.draw_series(std::iter::once(Rectangle::new(rect, rect_style)))?;
//     Ok(root.clone())
// }

// pub fn get_xyrange(cycle: &Cycle) -> (f64, f64) {
//     let xmin = cycle
//         .calc_dt_v
//         .iter()
//         .map(|dt| dt.timestamp() as f64)
//         .fold(f64::INFINITY, f64::min);
//     let xmax = cycle
//         .calc_dt_v
//         .iter()
//         .map(|dt| dt.timestamp() as f64)
//         .fold(f64::NEG_INFINITY, f64::max);
//     let ymin = cycle.calc_gas_v.iter().fold(f64::INFINITY, f64::min);
//     let ymax = cycle.calc_gas_v.iter().fold(f64::NEG_INFINITY, f64::max);
//
//     let x_range = xmax - xmin;
//     let y_range = ymax - ymin;
//
//     (x_range, y_range)
// }
// pub fn get_xyrange(cycle: &Cycle) -> (f64, f64) {
//     let xmin = cycle
//         .calc_dt_v
//         .iter()
//         .map(|dt| dt.timestamp() as f64)
//         .reduce(f64::min)
//         .unwrap_or(0.0);
//
//     let xmax = cycle
//         .calc_dt_v
//         .iter()
//         .map(|dt| dt.timestamp() as f64)
//         .reduce(f64::max)
//         .unwrap_or(0.0);
//
//     let ymin = cycle
//         .calc_gas_v
//         .iter()
//         .copied()
//         .reduce(f64::min)
//         .unwrap_or(0.0);
//
//     let ymax = cycle
//         .calc_gas_v
//         .iter()
//         .copied()
//         .reduce(f64::max)
//         .unwrap_or(0.0);
//
//     let x_range = xmax - xmin;
//     let y_range = ymax - ymin;
//
//     (x_range, y_range)
// }
