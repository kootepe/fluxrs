use crate::cycle::cycle::{update_fluxes, Cycle};
use crate::data_formats::chamberdata::Chamber;
use crate::data_formats::heightdata::HeightData;
use crate::data_formats::meteodata::{
    MeteoData, MeteoPoint, MeteoSource, DEFAULT_PRESSURE, DEFAULT_TEMP,
};
use crate::processevent::{ProcessEvent, ProgressEvent};
use crate::project::Project;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;

type HeightDataSet = HeightData;
type ChamberDataSet = HashMap<String, Chamber>;
type MeteoDataSet = MeteoData;

pub struct Datasets {
    pub meteo: MeteoDataSet,
    pub height: HeightDataSet,
    pub chambers: ChamberDataSet,
}

pub struct Infra {
    pub conn: Arc<Mutex<rusqlite::Connection>>,
    pub progress: UnboundedSender<ProcessEvent>,
}

pub struct Recalcer {
    project: Project,
    data: Arc<Datasets>, // Arc so tasks can share cheaply
    infra: Infra,
}
impl Recalcer {
    pub fn new(project: Project, data: Datasets, infra: Infra) -> Self {
        Self { project, data: Arc::new(data), infra }
    }

    pub async fn run_recalculating(&self, mut cycles: Vec<Cycle>) {
        println!("Recalculating.");
        let mut total_inserts = 0;
        let mut total_skips = 0;
        let progsender = self.infra.progress.clone();
        let _ = progsender.send(ProcessEvent::Progress(ProgressEvent::CalculationStarted));

        let total_cycles = cycles.len();
        for c in &mut cycles {
            let old_height = c.chamber.internal_height();

            // set new chamber height
            // BUG: Something should be done with snowheight
            c.chamber.set_height(
                self.data
                    .height
                    .get_nearest_previous_height(c.timing.get_start_utc_ts(), &c.chamber_id)
                    .unwrap_or(old_height),
            );

            let nearest = self.data.meteo.get_nearest(c.get_start_ts());

            let (mut temp_point, mut press_point) = nearest.unwrap_or((
                MeteoPoint {
                    value: Some(DEFAULT_TEMP),
                    source: MeteoSource::Default,
                    distance_from_target: None,
                },
                MeteoPoint {
                    value: Some(DEFAULT_PRESSURE),
                    source: MeteoSource::Default,
                    distance_from_target: None,
                },
            ));

            // Make sure Missing never leaks into pipeline:
            temp_point = temp_point.or_default(DEFAULT_TEMP);
            press_point = press_point.or_default(DEFAULT_PRESSURE);

            // Assign final values to cycle
            c.meteo.temperature = temp_point;
            c.meteo.pressure = press_point;

            if let Some(chamber) = self.data.chambers.get(&c.chamber_id) {
                c.chamber = *chamber
            }
            c.compute_all_fluxes();
            let _ =
                progsender.send(ProcessEvent::Progress(ProgressEvent::Recalced(1, total_cycles)));
        }

        if !cycles.is_empty() {
            let mut conn = self.infra.conn.lock().unwrap();
            match update_fluxes(&mut conn, &cycles) {
                Ok((inserts, skips)) => {
                    total_inserts += inserts;
                    total_skips += skips;
                },
                Err(e) => {
                    let _ = progsender.send(ProcessEvent::Done(Err(e.to_string())));
                },
            }
        }

        let _ = progsender.send(ProcessEvent::Done(Ok(())));
    }
}
