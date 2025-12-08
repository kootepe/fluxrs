use crate::ui::main_app::DateRange;
use fluxrs_core::project::Project;

use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone};
use chrono_tz::{Tz, UTC};

pub fn date_picker(ui: &mut egui::Ui, project: &Project, date_range: &mut DateRange) {
    let mut picker_start = date_range.start.date_naive();
    let mut picker_end = date_range.end.date_naive();
    let user_tz = &project.tz;

    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            // Start Date Picker
            ui.label("Pick start date:");
            if ui
                .add(
                    egui_extras::DatePickerButton::new(&mut picker_start)
                        .highlight_weekends(false)
                        .id_salt("start_date"),
                )
                .changed()
            {
                let naive = NaiveDateTime::from(picker_start);
                let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                date_range.start = pick;
            }
        });

        ui.vertical(|ui| {
            ui.label("Pick end date:");
            if ui
                .add(
                    egui_extras::DatePickerButton::new(&mut picker_end)
                        .highlight_weekends(false)
                        .id_salt("end_date"),
                )
                .changed()
            {
                let naive = NaiveDateTime::from(picker_end);
                let pick: DateTime<Tz> = user_tz.clone().from_local_datetime(&naive).unwrap();
                date_range.end = pick + TimeDelta::seconds(86399);
            }
        });
    });

    let start_before_end = date_range.start < date_range.end;

    if start_before_end {
        let delta = date_range.end - date_range.start;

        if let Ok(duration) = delta.to_std() {
            let total_secs = duration.as_secs();

            let days = total_secs / 86_400;
            let hours = (total_secs % 86_400) / 3_600;
            let minutes = (total_secs % 3_600) / 60;
            let seconds = total_secs % 60;

            let duration_str = if days > 0 {
                format!("{}d {:02}h {:02}m {:02}s", days, hours, minutes, seconds)
            } else if hours > 0 {
                format!("{:02}h {:02}m {:02}s", hours, minutes, seconds)
            } else if minutes > 0 {
                format!("{:02}m {:02}s", minutes, seconds)
            } else {
                format!("{:02}s", seconds)
            };
            ui.label(format!("From: {}", date_range.start));
            ui.label(format!("to: {}", date_range.end));

            ui.label(format!("Duration: {}", duration_str));

            // Buttons with full duration string
            if ui.add_enabled(true, egui::Button::new(format!("Next ({})", duration_str))).clicked()
            {
                date_range.start += delta;
                date_range.end += delta;
            }

            if ui
                .add_enabled(true, egui::Button::new(format!("Previous ({})", duration_str)))
                .clicked()
            {
                date_range.start -= delta;
                date_range.end -= delta;
            }
        }
    }
}
