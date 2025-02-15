use super::structs::Cycle;
use plotters::element::DashedPathElement;
use plotters::prelude::*;
use std::error::Error;

pub const PLOT_HEIGHT: u32 = 50;
pub const PLOT_WIDTH: u32 = PLOT_HEIGHT * 2;

pub fn draw_gas_plot(cycle: &Cycle) -> Result<String, Box<dyn Error>> {
    let name = cycle.start_time.to_string();
    let mut root = String::from("images/");
    root.push_str(&name);
    root.retain(|c| !r#"()-,".;:' "#.contains(c));
    root.push_str(".png");
    let mut wpath = String::from("html/");
    wpath.push_str(&root);
    let time_data: Vec<f64> = cycle.dt_v.iter().map(|x| x.timestamp() as f64).collect();
    let calc_time_data: Vec<f64> = cycle
        .calc_dt_v
        .iter()
        .map(|x| x.timestamp() as f64)
        .collect();
    let xmin = time_data[0];
    let xmax = time_data.last().unwrap();
    let cxmin = calc_time_data[0];
    let cxmax = calc_time_data.last().unwrap();
    let mut closet = Vec::new();
    let gas_data = &cycle.gas_v;
    // let ymin = *gas_data.iter().min_by(|a, b| a.total_cmp(b)).unwrap();
    // let ymax = *gas_data.iter().max_by(|a, b| a.total_cmp(b)).unwrap();
    let ymin = cycle
        .gas_v
        .iter()
        .cloned()
        .filter(|v| !v.is_nan())
        .fold(f64::INFINITY, f64::min);
    let ymax = cycle
        .gas_v
        .iter()
        .cloned()
        .filter(|v| !v.is_nan())
        .fold(f64::NEG_INFINITY, f64::max);

    let datetime_idx = cycle.get_peak_datetime().unwrap().timestamp() as f64;
    closet.push((datetime_idx, ymax));
    closet.push((datetime_idx, ymin));
    let rect = [(*cxmax, ymax), (cxmin, ymin)];
    let data: Vec<(f64, f64)> = time_data
        .iter()
        .zip(gas_data.iter())
        .map(|(&t, &g)| (t, g))
        .collect();
    let style = ShapeStyle {
        color: RED.mix(0.3),
        filled: true,
        stroke_width: 5,
    };
    let root_area = BitMapBackend::new(&wpath, (PLOT_WIDTH, PLOT_HEIGHT)).into_drawing_area();
    root_area.fill(&WHITE).unwrap();

    let xra = xmax - xmin;
    let yra = ymax - ymin;
    let mut ctx = ChartBuilder::on(&root_area)
        // .set_label_area_size(LabelAreaPosition::Left, 50)
        // .set_label_area_size(LabelAreaPosition::Bottom, 50)
        // .margin_right(50)
        // .caption("Scatter Demo", ("sans-serif", 50))
        // .build_cartesian_2d(-10.0..50.0, -10.0..50.0)
        .build_cartesian_2d(
            (xmin - (xra * 0.05))..(*xmax + (xra * 0.05)),
            (ymin - (yra * 0.05))..(ymax + (yra * 0.05)),
        )
        .unwrap();

    ctx.configure_mesh().disable_mesh().draw().unwrap();

    let col = RGBColor(105, 239, 83);
    ctx.draw_series(data.iter().map(|point| Cross::new(*point, 1, col.mix(0.6))))
        .unwrap();
    ctx.draw_series(std::iter::once(DashedPathElement::new(closet, 11, 5, RED)))?;
    // ctx.draw_series(std::iter::once(Rectangle::new(rect, style)))?;
    Ok(root.clone())
}
