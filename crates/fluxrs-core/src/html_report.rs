use super::*;

// pub fn write_cycles_to_html(cycles: &[structs::Cycle]) -> Result<(), Box<dyn Error>> {
//     let mut grouped_cycles: HashMap<chrono::NaiveDate, Vec<&structs::Cycle>> = HashMap::new();
//
//     // Group cycles by date
//     for cycle in cycles {
//         let date = cycle.start_time.date_naive();
//         grouped_cycles
//             .entry(date)
//             // .or_insert_with(Vec::new)
//             .or_default()
//             .push(cycle);
//     }
//
//     let mut all_dates: Vec<chrono::NaiveDate> = grouped_cycles.keys().cloned().collect();
//     all_dates.sort();
//
//     for (i, date) in all_dates.iter().enumerate() {
//         let filename = format!("html/cycles_{}.html", date);
//
//         let mut html = String::from(
//             r#"<!DOCTYPE html>
// <html lang="en">
// <head>
//   <meta charset="UTF-8">
//   <meta name="viewport" content="width=device-width, initial-scale=1.0">
//   <title>Cycle Data for "#,
//         );
//         html.push_str(&date.to_string());
//         html.push_str(
//             r#"</title>
//   <style>
//     body {
//         font-family: Arial, sans-serif;
//         background-color: #121212; /* Dark grayish black */
//         color: #e0e0e0; /* Light gray text */
//         padding: 20px;
//     }
//     table {
//         border-collapse: collapse;
//         width: 50%;
//         background-color: #1f1f1f; /* Slightly lighter black for tables */
//         box-shadow: 0 4px 8px rgba(0,0,0,0.5);
//     }
//     th, td {
//         border: 1px solid #333333;
//         text-align: center;
//         padding: 8px;
//         width: 70px;
//     }
//     th {
//         background-color: #2c2c2c; /* Dark gray headers */
//         color: #f5f5f5;
//     }
//     td {
//         background-color: #1a1a1a; /* Very dark gray for cells */
//     }
//     img {
//         max-width: 200px;
//         height: auto;
//         border: 2px solid #333333;
//     }
//     nav a {
//         color: #9f9f9f;
//         text-decoration: none;
//         margin-right: 10px;
//     }
//     nav a:hover {
//         color: #ffffff;
//         text-decoration: underline;
//     }
//   </style>
// </head>
// <body>
//   <h1>Cycle Data for "#,
//         );
//         html.push_str(&date.to_string());
//         html.push_str(
//             r#"</h1>
//   <nav>"#,
//         );
//
//         // Add link to the previous day if available
//         if i > 0 {
//             let prev_date = all_dates[i - 1];
//             html.push_str(&format!(
//                 r#"<a href="cycles_{}.html">&larr; Previous Day ({})</a> | "#,
//                 prev_date, prev_date
//             ));
//         }
//
//         // Add link to the next day if available
//         if i < all_dates.len() - 1 {
//             let next_date = all_dates[i + 1];
//             html.push_str(&format!(
//                 r#"<a href="cycles_{}.html">Next Day ({}) &rarr;</a>"#,
//                 next_date, next_date
//             ));
//         }
//
//         html.push_str(
//             r#"</nav>
//   <table>
//     <tr>
//       <th>Chamber ID</th>
//       <th>Start Time</th>
//       <th>Lag (s)</th>
//       <th>r</th>
//       <th>total_r</th>
//       <th>is_valid</th>
//       <th>Flux</th>
//       <th>Gas Plot</th>
//     </tr>
// "#,
//         );
//
//         for cycle in grouped_cycles.get(date).unwrap() {
//             let diag_sum: i64 = cycle.diag_v.iter().copied().sum();
//             let plot_path = draw_gas_plot(cycle)?;
//             let mut row = format!(
//                 "<tr style=\"color:greenyellow\">\
//                     <td>{}</td>\
//                     <td>{}</td>\
//                     <td>{}</td>\
//                     <td>{:.4}</td>\
//                     <td>{:.4}</td>\
//                     <td>{}</td>\
//                     <td>{:.4}</td>\
//                     <td><img src=\"{}\" alt=\"Gas Plot\"></td>\
//                 </tr>\n",
//                 cycle.chamber_id,
//                 cycle.start_time.to_rfc3339().replace("+00:00", ""),
//                 cycle.lag_s,
//                 cycle.r,
//                 cycle.calc_r,
//                 if diag_sum == 0 { 1 } else { 0 },
//                 cycle.flux,
//                 plot_path
//             );
//             if diag_sum != 0 {
//                 row = row.replace("greenyellow", "salmon");
//             }
//             if cycle.calc_r < 0.99 {
//                 row = row.replace("greenyellow", "yellow");
//             }
//             html.push_str(&row);
//         }
//
//         html.push_str(
//             r#"
//   </table>
// </body>
// </html>
// "#,
//         );
//
//         let mut file = File::create(&filename)?;
//         file.write_all(html.as_bytes())?;
//         file.flush()?;
//
//         println!("HTML file successfully written to {}", filename);
//     }
//
//     Ok(())
// }
