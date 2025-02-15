use super::*;

pub fn write_cycles_to_html(cycles: &[structs::Cycle]) -> Result<(), Box<dyn Error>> {
    let mut grouped_cycles: HashMap<chrono::NaiveDate, Vec<&structs::Cycle>> = HashMap::new();

    // Group cycles by date
    for cycle in cycles {
        let date = cycle.start_time.date_naive();
        grouped_cycles
            .entry(date)
            .or_insert_with(Vec::new)
            .push(cycle);
    }

    let mut all_dates: Vec<chrono::NaiveDate> = grouped_cycles.keys().cloned().collect();
    all_dates.sort();

    for (i, date) in all_dates.iter().enumerate() {
        let filename = format!("html/cycles_{}.html", date);

        let mut html = String::from(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Cycle Data for "#,
        );
        html.push_str(&date.to_string());
        html.push_str(
            r#"</title>
  <style>
    body { font-family: Arial, sans-serif; padding: 20px; }
    table { border-collapse: collapse; }
    th, td { border: 1px solid #ddd; text-align: center; width: 70px; }
    th { background-color: #f2f2f2; }
    nav { margin-bottom: 20px; font-size: 18px; }
  </style>
</head>
<body>
  <h1>Cycle Data for "#,
        );
        html.push_str(&date.to_string());
        html.push_str(
            r#"</h1>
  <nav>"#,
        );

        // Add link to the previous day if available
        if i > 0 {
            let prev_date = all_dates[i - 1];
            html.push_str(&format!(
                r#"<a href="cycles_{}.html">&larr; Previous Day ({})</a> | "#,
                prev_date, prev_date
            ));
        }

        // Add link to the next day if available
        if i < all_dates.len() - 1 {
            let next_date = all_dates[i + 1];
            html.push_str(&format!(
                r#"<a href="cycles_{}.html">Next Day ({}) &rarr;</a>"#,
                next_date, next_date
            ));
        }

        html.push_str(
            r#"</nav>
  <table>
    <tr>
      <th>Chamber ID</th>
      <th>Start Time</th>
      <th>Lag (s)</th>
      <th>r</th>
      <th>Flux</th>
      <th>Gas Plot</th>
    </tr>
"#,
        );

        for cycle in grouped_cycles.get(date).unwrap() {
            let plot_path = draw_gas_plot(cycle)?;
            let row = format!(
                "<tr>\
                    <td>{}</td>\
                    <td>{}</td>\
                    <td>{}</td>\
                    <td>{:.4}</td>\
                    <td>{:.4}</td>\
                    <td><img src=\"{}\" alt=\"Gas Plot\"></td>\
                </tr>\n",
                cycle.chamber_id,
                cycle.start_time.to_rfc3339().replace("+00:00", ""),
                cycle.lag_s,
                cycle.r,
                cycle.flux,
                plot_path
            );
            html.push_str(&row);
        }

        html.push_str(
            r#"
  </table>
</body>
</html>
"#,
        );

        let mut file = File::create(&filename)?;
        file.write_all(html.as_bytes())?;
        file.flush()?;

        println!("HTML file successfully written to {}", filename);
    }

    Ok(())
}
