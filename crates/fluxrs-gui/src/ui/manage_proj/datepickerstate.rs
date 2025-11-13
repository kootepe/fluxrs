use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use egui::Context;

pub struct DateRangePickerState {
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub datepicker_start_open: bool,
    pub datepicker_end_open: bool,
}

impl Default for DateRangePickerState {
    fn default() -> Self {
        Self::new()
    }
}

impl DateRangePickerState {
    fn new() -> Self {
        Self {
            start_date: Utc::now() - TimeDelta::days(7),
            end_date: Utc::now(),
            datepicker_start_open: false,
            datepicker_end_open: false,
        }
    }

    pub fn date_picker(&mut self, ui: &mut egui::Ui, ctx: &Context) {
        let mut picker_start = self.start_date.date_naive();
        let mut picker_end = self.end_date.date_naive();

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
                    let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                        NaiveDateTime::from(picker_start),
                        Utc,
                    );
                    self.datepicker_start_open = false;
                    self.start_date = pick;
                } else {
                    self.datepicker_start_open = true;
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
                    let pick = DateTime::<Utc>::from_naive_utc_and_offset(
                        NaiveDateTime::from(picker_end),
                        Utc,
                    );
                    self.end_date = pick + TimeDelta::seconds(86399);
                    self.datepicker_end_open = true;
                } else {
                    self.datepicker_end_open = false;
                }
            });
        });

        let start_after_end = self.start_date < self.end_date;
        let mut delta_days = TimeDelta::zero();
        let mut days = 0;

        if start_after_end {
            delta_days = self.end_date - self.start_date;
            days = delta_days.to_std().unwrap().as_secs() / 86400;
        }

        if ui
            .add_enabled(start_after_end, egui::Button::new(format!("Next {} days", days)))
            .clicked()
        {
            self.start_date += delta_days;
            self.end_date += delta_days;
        }

        if ui
            .add_enabled(start_after_end, egui::Button::new(format!("Previous {} days", days)))
            .clicked()
        {
            self.start_date -= delta_days;
            self.end_date -= delta_days;
        }
    }
}
