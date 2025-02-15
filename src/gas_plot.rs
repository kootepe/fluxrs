use super::structs::Cycle;
use plotters::element::DashedPathElement;
use plotters::prelude::*;
use std::error::Error;

pub fn draw_gas_plot(cycle: Cycle) -> Result<(), Box<dyn Error>> {
    let name = cycle.start_time.to_string();
    let mut root = String::from("images/");
    root.push_str(&name);
    root.push_str(".png");
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
    let ymin = *gas_data.iter().min_by(|a, b| a.total_cmp(b)).unwrap();
    let ymax = *gas_data.iter().max_by(|a, b| a.total_cmp(b)).unwrap();
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
    let root_area = SVGBackend::new(&root, (87, 50)).into_drawing_area();
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
    // ctx.draw_series(
    //     rect.iter()
    //         .map(|point| Rectangle::new(*point, col.mix(0.6))),
    // )
    // .unwrap();
    ctx.draw_series(std::iter::once(Rectangle::new(rect, style)))?;
    // ctx.draw_series(closet.iter().map(|point| Circle::new(*point, 5, RED)))
    //     .unwrap();
    // ctx.draw_series(closet.iter().map(|(x, y)| {
    //     EmptyElement::at((*x, *y)) // Use the guest coordinate system with EmptyElement
    //     + Circle::new((0, 0), 10, BLUE) // Use backend coordinates with the rest
    //     + Cross::new((4, 4), 3, RED)
    //     + Pixel::new((4, -4), RED)
    //     + TriangleMarker::new((-4, -4), 4, RED)
    // }))
    // .unwrap();
    // root_area.draw(
    //     &(EmptyElement::at((ymin as i32, xmax as i32))
    //         + Circle::new((-5, -10), 6, RED)),
    // )?;
    // calced_v.push(cycle)
    Ok(())
}
